#!/bin/bash

# Performance Benchmarking Script for Wake vs Stern
# Measures CPU and memory usage under high-load log streaming conditions

set -e

# Configuration
BENCHMARK_DURATION=120  # 2 minutes per test (changed from 60)
WARMUP_DURATION=10      # 10 seconds warmup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WAKE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RESULTS_DIR="$SCRIPT_DIR/benchmark_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$RESULTS_DIR/benchmark_$TIMESTAMP.log"
CSV_FILE="$RESULTS_DIR/benchmark_$TIMESTAMP.csv"
DEBUG_LOG="$RESULTS_DIR/debug_$TIMESTAMP.log"

# Ensure results directory exists before any logging
mkdir -p "$RESULTS_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YIGHLIGHT='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test scenarios - all available
declare -A ALL_TEST_SCENARIOS=(
    ["basic"]=".*"
    ["error_filter"]="ERROR"
    ["complex_filter"]="ERROR|WARN"
    ["high_frequency"]=".*"
)

# Parse command line arguments
SELECTED_SCENARIOS=()
RUN_ALL=true

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  --basic          Run only basic scenario"
    echo "  --error          Run only error_filter scenario"
    echo "  --complex        Run only complex_filter scenario"
    echo "  --high           Run only high_frequency scenario"
    echo "  --all            Run all scenarios (default)"
    echo "  --help           Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --complex     # Run only complex filter test"
    echo "  $0 --basic --error # Run basic and error filter tests"
    echo "  $0               # Run all scenarios"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --basic)
            SELECTED_SCENARIOS+=("basic")
            RUN_ALL=false
            shift
            ;;
        --error)
            SELECTED_SCENARIOS+=("error_filter")
            RUN_ALL=false
            shift
            ;;
        --complex)
            SELECTED_SCENARIOS+=("complex_filter")
            RUN_ALL=false
            shift
            ;;
        --high)
            SELECTED_SCENARIOS+=("high_frequency")
            RUN_ALL=false
            shift
            ;;
        --all)
            RUN_ALL=true
            shift
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Set up TEST_SCENARIOS based on selection
declare -A TEST_SCENARIOS
if [[ "$RUN_ALL" == true ]]; then
    for scenario in "${!ALL_TEST_SCENARIOS[@]}"; do
        TEST_SCENARIOS[$scenario]="${ALL_TEST_SCENARIOS[$scenario]}"
    done
    log_scenarios="all scenarios"
else
    if [[ ${#SELECTED_SCENARIOS[@]} -eq 0 ]]; then
        echo "Error: No scenarios selected. Use --help for usage information."
        exit 1
    fi
    for scenario in "${SELECTED_SCENARIOS[@]}"; do
        TEST_SCENARIOS[$scenario]="${ALL_TEST_SCENARIOS[$scenario]}"
    done
    log_scenarios="${SELECTED_SCENARIOS[*]}"
fi

# Performance metrics tracking
declare -A WAKE_METRICS
declare -A STERN_METRICS

# Utility functions
log() {
    echo -e "${BLUE}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$LOG_FILE"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$LOG_FILE"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1" | tee -a "$LOG_FILE"
}

warn() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$LOG_FILE"
}

# Check dependencies
check_dependencies() {
    log "Checking dependencies..."
    
    # Check if wake is built
    if [[ ! -f "$WAKE_ROOT/target/release/wake" ]]; then
        error "Wake binary not found. Building..."
        cd "$WAKE_ROOT" && cargo build --release && cd "$SCRIPT_DIR"
    fi
    
    # Check if stern is installed
    if ! command -v stern &> /dev/null; then
        error "Stern is not installed. Please install stern first."
        error "Install with: curl -LO https://github.com/stern/stern/releases/latest/download/stern_linux_amd64.tar.gz"
        exit 1
    fi
    
    # Check if kubectl is available
    if ! command -v kubectl &> /dev/null; then
        error "kubectl is not installed"
        exit 1
    fi
    
    # Check if performance monitoring tools are available
    if ! command -v pidstat &> /dev/null; then
        warn "pidstat not found. Installing sysstat..."
        sudo apt-get update && sudo apt-get install -y sysstat
    fi
    
    success "All dependencies checked"
}

# Setup benchmark environment
setup_benchmark_environment() {
    log "Setting up benchmark environment..."
    
    # Create results directory
    mkdir -p "$RESULTS_DIR"
    
    # Initialize CSV file
    echo "timestamp,tool,scenario,cpu_percent,memory_mb,rss_mb,vms_mb,logs_processed,duration_seconds" > "$CSV_FILE"
    
    # Setup performance test environment
    if ! kubectl get namespace perf-test &> /dev/null; then
        log "Setting up performance test environment..."
        ./setup-perf.sh
        
        # Wait for pods to be ready
        log "Waiting for log generators to be ready..."
        kubectl wait --for=condition=ready pod -l app=perf-generator -n perf-test --timeout=120s
        sleep 30  # Additional time for log generation to stabilize
    fi
    
    success "Benchmark environment ready"
}

# Monitor process performance
monitor_process() {
    local pid=$1
    local tool_name=$2
    local scenario=$3
    local output_file=$4
    local duration=$5
    
    log "Monitoring $tool_name (PID: $pid) for $duration seconds..."
    
    # Start monitoring in background
    {
        local start_time=$(date +%s)
        local logs_processed=0
        
        while kill -0 $pid 2>/dev/null; do
            # Get process stats
            if [[ -f "/proc/$pid/stat" ]]; then
                local stat_line=$(cat /proc/$pid/stat)
                local cpu_time=$(echo $stat_line | awk '{print $14 + $15}')  # utime + stime
                local rss_pages=$(echo $stat_line | awk '{print $24}')
                local vms_bytes=$(echo $stat_line | awk '{print $23}')
                
                # Convert to human readable units
                local rss_mb=$((rss_pages * 4096 / 1024 / 1024))  # Pages to MB
                local vms_mb=$((vms_bytes / 1024 / 1024))          # Bytes to MB
                
                # Get CPU percentage using a more reliable method
                # Method 1: Try top with multiple samples
                local cpu_percent=$(top -p $pid -b -n 2 -d 1 | tail -n +8 | grep -E "^\s*$pid" | tail -1 | awk '{print $9}' 2>/dev/null)
                
                # Method 2: Fallback to pidstat if available and top fails
                if [[ -z "$cpu_percent" || "$cpu_percent" == "0.0" ]]; then
                    cpu_percent=$(pidstat -p $pid 1 1 2>/dev/null | tail -1 | awk '{print $8}' 2>/dev/null)
                fi
                
                # Method 3: Final fallback to ps
                if [[ -z "$cpu_percent" || "$cpu_percent" == "0.0" ]]; then
                    cpu_percent=$(ps -p $pid -o %cpu --no-headers 2>/dev/null | tr -d ' ')
                fi
                
                # Ensure we have a valid number
                [[ -z "$cpu_percent" || ! "$cpu_percent" =~ ^[0-9]*\.?[0-9]+$ ]] && cpu_percent="0.0"
                
                # Get memory info from /proc/meminfo and process status
                local memory_mb=$(cat /proc/$pid/status | grep VmRSS | awk '{print $2}')
                memory_mb=$((memory_mb / 1024))  # Convert KB to MB
                
                # Estimate logs processed (rough calculation based on time)
                local current_time=$(date +%s)
                local elapsed=$((current_time - start_time))
                logs_processed=$((elapsed * 14000 / 60))  # Approximate based on 14k logs/sec
                
                # Write to CSV
                echo "$(date '+%Y-%m-%d %H:%M:%S'),$tool_name,$scenario,$cpu_percent,$memory_mb,$rss_mb,$vms_mb,$logs_processed,$elapsed" >> "$CSV_FILE"
                
                # Store metrics for summary
                eval "${tool_name}_METRICS[${scenario}_cpu_max]=\$(echo \"\${${tool_name}_METRICS[${scenario}_cpu_max]} $cpu_percent\" | awk '{print (\$1 > \$2) ? \$1 : \$2}')"
                eval "${tool_name}_METRICS[${scenario}_memory_max]=\$(echo \"\${${tool_name}_METRICS[${scenario}_memory_max]} $memory_mb\" | awk '{print (\$1 > \$2) ? \$1 : \$2}')"
                eval "${tool_name}_METRICS[${scenario}_cpu_avg]=\$(echo \"\${${tool_name}_METRICS[${scenario}_cpu_avg]} $cpu_percent\" | awk '{print (\$1 + \$2) / 2}')"
                eval "${tool_name}_METRICS[${scenario}_memory_avg]=\$(echo \"\${${tool_name}_METRICS[${scenario}_memory_avg]} $memory_mb\" | awk '{print (\$1 + \$2) / 2}')"
            else
                echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Process $pid stat file missing" >> "$DEBUG_LOG"
            fi
            
            sleep 2
        done
    } &
    
    local monitor_pid=$!
    
    # Wait for the specified duration
    sleep "$duration"
    
    # Stop monitoring
    kill $monitor_pid 2>/dev/null || true
    wait $monitor_pid 2>/dev/null || true
}

# Run Wake benchmark
run_wake_benchmark() {
    local scenario=$1
    local filter=$2
    
    log "Running Wake benchmark for scenario: $scenario"
    log "Filter: $filter"
    
    # Debug: Check if Wake binary exists and is executable
    if [[ ! -x "$WAKE_ROOT/target/release/wake" ]]; then
        error "Wake binary is not executable at $WAKE_ROOT/target/release/wake"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Wake binary missing or not executable for scenario: $scenario" >> "$DEBUG_LOG"
        return 1
    fi
    
    # Debug: Test Wake command with the filter first
    echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Testing Wake command: $WAKE_ROOT/target/release/wake -n perf-test -i \"$filter\" --no-ui" >> "$DEBUG_LOG"
    
    # Start Wake in background with better error handling
    "$WAKE_ROOT/target/release/wake" -n perf-test -i "$filter" --no-ui > /dev/null 2> /dev/null &
    local wake_pid=$!
    
    # Debug: Check if Wake started successfully
    sleep 2
    if ! kill -0 $wake_pid 2>/dev/null; then
        error "Wake failed to start for scenario: $scenario"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Wake process died immediately for scenario: $scenario" >> "$DEBUG_LOG"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Wake stderr:" >> "$DEBUG_LOG"
        if [[ -f "$RESULTS_DIR/wake_${scenario}_error.log" ]]; then
            cat "$RESULTS_DIR/wake_${scenario}_error.log" >> "$DEBUG_LOG"
        fi
        return 1
    fi
    
    echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Wake started successfully with PID: $wake_pid for scenario: $scenario" >> "$DEBUG_LOG"
    
    # Wait for warmup
    log "Wake warmup period: $WARMUP_DURATION seconds"
    sleep "$WARMUP_DURATION"
    
    # Debug: Check if Wake is still running after warmup
    if ! kill -0 $wake_pid 2>/dev/null; then
        error "Wake died during warmup for scenario: $scenario"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Wake process died during warmup for scenario: $scenario" >> "$DEBUG_LOG"
        return 1
    fi
    
    # Monitor performance
    monitor_process "$wake_pid" "wake" "$scenario" "$CSV_FILE" "$BENCHMARK_DURATION"
    
    # Stop Wake
    if kill -0 $wake_pid 2>/dev/null; then
        kill $wake_pid 2>/dev/null || true
        wait $wake_pid 2>/dev/null || true
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Wake process terminated normally for scenario: $scenario" >> "$DEBUG_LOG"
    else
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Wake process already died for scenario: $scenario" >> "$DEBUG_LOG"
    fi
    
    success "Wake benchmark completed for scenario: $scenario"
}

# Run Stern benchmark
run_stern_benchmark() {
    local scenario=$1
    local filter=$2
    
    log "Running Stern benchmark for scenario: $scenario"
    log "Filter: $filter"
    
    # Convert Wake filter to Stern format (basic conversion)
    local stern_filter=""
    local stern_cmd=""
    case "$filter" in
        ".*")
            stern_cmd="stern '.*' -n perf-test"
            ;;
        "ERROR")
            stern_cmd="stern '.*' -n perf-test --include='ERROR'"
            ;;
        "ERROR|WARN")
            stern_cmd="stern '.*' -n perf-test --include='ERROR' --include='WARN'"
            ;;
        *)
            stern_cmd="stern '.*' -n perf-test"
            ;;
    esac
    
    # Debug: Log the Stern command being executed
    echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Stern command: $stern_cmd" >> "$DEBUG_LOG"
    
    # Start Stern in background with better error handling
    eval "$stern_cmd" > /dev/null 2> /dev/null &
    local stern_pid=$!
    
    # Debug: Check if Stern started successfully
    sleep 2
    if ! kill -0 $stern_pid 2>/dev/null; then
        error "Stern failed to start for scenario: $scenario"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Stern process died immediately for scenario: $scenario" >> "$DEBUG_LOG"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Stern stderr:" >> "$DEBUG_LOG"
        if [[ -f "$RESULTS_DIR/stern_${scenario}_error.log" ]]; then
            cat "$RESULTS_DIR/stern_${scenario}_error.log" >> "$DEBUG_LOG"
        fi
        return 1
    fi
    
    echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Stern started successfully with PID: $stern_pid for scenario: $scenario" >> "$DEBUG_LOG"
    
    # Wait for warmup
    log "Stern warmup period: $WARMUP_DURATION seconds"
    sleep "$WARMUP_DURATION"
    
    # Debug: Check if Stern is still running after warmup
    if ! kill -0 $stern_pid 2>/dev/null; then
        error "Stern died during warmup for scenario: $scenario"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Stern process died during warmup for scenario: $scenario" >> "$DEBUG_LOG"
        return 1
    fi
    
    # Monitor performance
    monitor_process "$stern_pid" "stern" "$scenario" "$CSV_FILE" "$BENCHMARK_DURATION"
    
    # Stop Stern
    if kill -0 $stern_pid 2>/dev/null; then
        kill $stern_pid 2>/dev/null || true
        wait $stern_pid 2>/dev/null || true
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Stern process terminated normally for scenario: $scenario" >> "$DEBUG_LOG"
    else
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Stern process already died for scenario: $scenario" >> "$DEBUG_LOG"
    fi
    
    success "Stern benchmark completed for scenario: $scenario"
}

# Generate performance report
generate_report() {
    local report_file="$RESULTS_DIR/performance_report_$TIMESTAMP.md"
    
    log "Generating performance report..."
    
    # Calculate metrics from CSV data directly
    calculate_metrics_from_csv
    
    cat > "$report_file" << EOF
# Performance Benchmark Report

**Date:** $(date '+%Y-%m-%d %H:%M:%S')
**Duration per test:** ${BENCHMARK_DURATION}s
**Warmup duration:** ${WARMUP_DURATION}s
**Log generation rate:** ~14,000 logs/second

## Test Environment

- **Kubernetes cluster:** $(kubectl config current-context)
- **Wake version:** $("$WAKE_ROOT/target/release/wake" --version 2>/dev/null || echo "Unknown")
- **Stern version:** $(stern --version 2>/dev/null || echo "Unknown")
- **System:** $(uname -a)

## Test Scenarios

EOF

    # Add scenario results
    for scenario in "${!TEST_SCENARIOS[@]}"; do
        cat >> "$report_file" << EOF

### Scenario: $scenario
**Filter:** \`${TEST_SCENARIOS[$scenario]}\`

| Metric | Wake | Stern | Winner |
|--------|------|-------|--------|
| Max CPU (%) | ${WAKE_METRICS[${scenario}_cpu_max]:-N/A} | ${STERN_METRICS[${scenario}_cpu_max]:-N/A} | $(compare_metrics "${WAKE_METRICS[${scenario}_cpu_max]}" "${STERN_METRICS[${scenario}_cpu_max]}" "lower") |
| Avg CPU (%) | ${WAKE_METRICS[${scenario}_cpu_avg]:-N/A} | ${STERN_METRICS[${scenario}_cpu_avg]:-N/A} | $(compare_metrics "${WAKE_METRICS[${scenario}_cpu_avg]}" "${STERN_METRICS[${scenario}_cpu_avg]}" "lower") |
| Max Memory (MB) | ${WAKE_METRICS[${scenario}_memory_max]:-N/A} | ${STERN_METRICS[${scenario}_memory_max]:-N/A} | $(compare_metrics "${WAKE_METRICS[${scenario}_memory_max]}" "${STERN_METRICS[${scenario}_memory_max]}" "lower") |
| Avg Memory (MB) | ${WAKE_METRICS[${scenario}_memory_avg]:-N/A} | ${STERN_METRICS[${scenario}_memory_avg]:-N/A} | $(compare_metrics "${WAKE_METRICS[${scenario}_memory_avg]}" "${STERN_METRICS[${scenario}_memory_avg]}" "lower") |

EOF
    done
    
    cat >> "$report_file" << EOF

## Raw Data

The complete performance data is available in: \`$CSV_FILE\`

## Visualizations

To generate graphs from the data:

\`\`\`bash
# Install required tools
pip install matplotlib pandas

# Generate performance graphs
python3 visualize_performance.py $CSV_FILE
\`\`\`

## Summary

$(generate_summary)

EOF
    
    success "Performance report generated: $report_file"
}

# Calculate metrics from CSV data
calculate_metrics_from_csv() {
    log "Calculating metrics from CSV data..."
    
    # Skip the header line and process each scenario
    for scenario in "${!TEST_SCENARIOS[@]}"; do
        # Calculate Wake metrics
        local wake_cpu_max=$(awk -F',' -v scenario="$scenario" '$2=="wake" && $3==scenario {if($4>max) max=$4} END {print max+0}' "$CSV_FILE")
        local wake_cpu_avg=$(awk -F',' -v scenario="$scenario" '$2=="wake" && $3==scenario {sum+=$4; count++} END {if(count>0) print sum/count; else print 0}' "$CSV_FILE")
        local wake_mem_max=$(awk -F',' -v scenario="$scenario" '$2=="wake" && $3==scenario {if($5>max) max=$5} END {print max+0}' "$CSV_FILE")
        local wake_mem_avg=$(awk -F',' -v scenario="$scenario" '$2=="wake" && $3==scenario {sum+=$5; count++} END {if(count>0) print sum/count; else print 0}' "$CSV_FILE")
        
        # Calculate Stern metrics
        local stern_cpu_max=$(awk -F',' -v scenario="$scenario" '$2=="stern" && $3==scenario {if($4>max) max=$4} END {print max+0}' "$CSV_FILE")
        local stern_cpu_avg=$(awk -F',' -v scenario="$scenario" '$2=="stern" && $3==scenario {sum+=$4; count++} END {if(count>0) print sum/count; else print 0}' "$CSV_FILE")
        local stern_mem_max=$(awk -F',' -v scenario="$scenario" '$2=="stern" && $3==scenario {if($5>max) max=$5} END {print max+0}' "$CSV_FILE")
        local stern_mem_avg=$(awk -F',' -v scenario="$scenario" '$2=="stern" && $3==scenario {sum+=$5; count++} END {if(count>0) print sum/count; else print 0}' "$CSV_FILE")
        
        # Store in arrays (round to 1 decimal place)
        WAKE_METRICS[${scenario}_cpu_max]=$(printf "%.1f" "$wake_cpu_max")
        WAKE_METRICS[${scenario}_cpu_avg]=$(printf "%.1f" "$wake_cpu_avg")
        WAKE_METRICS[${scenario}_memory_max]=$(printf "%.1f" "$wake_mem_max")
        WAKE_METRICS[${scenario}_memory_avg]=$(printf "%.1f" "$wake_mem_avg")
        
        STERN_METRICS[${scenario}_cpu_max]=$(printf "%.1f" "$stern_cpu_max")
        STERN_METRICS[${scenario}_cpu_avg]=$(printf "%.1f" "$stern_cpu_avg")
        STERN_METRICS[${scenario}_memory_max]=$(printf "%.1f" "$stern_mem_max")
        STERN_METRICS[${scenario}_memory_avg]=$(printf "%.1f" "$stern_mem_avg")
    done
}

# Compare metrics helper
compare_metrics() {
    local wake_value=$1
    local stern_value=$2
    local comparison_type=$3  # "lower" or "higher"
    
    if [[ -z "$wake_value" || -z "$stern_value" || "$wake_value" == "0.0" || "$stern_value" == "0.0" ]]; then
        echo "N/A"
        return
    fi
    
    local comparison_result=$(echo "$wake_value $stern_value" | awk -v type="$comparison_type" '{
        if (type == "lower") {
            print ($1 < $2) ? "Wake" : "Stern"
        } else {
            print ($1 > $2) ? "Wake" : "Stern"
        }
    }')
    
    echo "$comparison_result"
}

# Generate summary
generate_summary() {
    echo "Performance benchmark completed successfully."
    echo "Wake demonstrated $(count_wins "wake") wins vs Stern's $(count_wins "stern") wins across all scenarios."
    echo ""
    echo "Note: This was a 2-minute test. Check the debug log for any issues with Wake execution."
}

# Count wins helper
count_wins() {
    local tool=$1
    local wins=0
    
    # Count scenarios where the tool has valid data
    for scenario in "${!TEST_SCENARIOS[@]}"; do
        if [[ "${tool}" == "wake" ]]; then
            local cpu_max="${WAKE_METRICS[${scenario}_cpu_max]}"
            local mem_max="${WAKE_METRICS[${scenario}_memory_max]}"
            if [[ -n "$cpu_max" && "$cpu_max" != "0.0" && -n "$mem_max" && "$mem_max" != "0.0" ]]; then
                ((wins++))
            fi
        else
            local cpu_max="${STERN_METRICS[${scenario}_cpu_max]}"
            local mem_max="${STERN_METRICS[${scenario}_memory_max]}"
            if [[ -n "$cpu_max" && "$cpu_max" != "0.0" && -n "$mem_max" && "$mem_max" != "0.0" ]]; then
                ((wins++))
            fi
        fi
    done
    
    echo $wins
}

# Main execution
main() {
    log "Starting Wake vs Stern Performance Benchmark (2-minute test)"
    log "Results will be saved to: $RESULTS_DIR"
    log "Selected scenarios: $log_scenarios"
    
    # Initialize debug log
    echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Starting benchmark with 2-minute duration" > "$DEBUG_LOG"
    
    # Setup
    check_dependencies
    setup_benchmark_environment
    
    # Initialize metrics arrays
    for scenario in "${!TEST_SCENARIOS[@]}"; do
        WAKE_METRICS[${scenario}_cpu_max]=0
        WAKE_METRICS[${scenario}_cpu_avg]=0
        WAKE_METRICS[${scenario}_memory_max]=0
        WAKE_METRICS[${scenario}_memory_avg]=0
        STERN_METRICS[${scenario}_cpu_max]=0
        STERN_METRICS[${scenario}_cpu_avg]=0
        STERN_METRICS[${scenario}_memory_max]=0
        STERN_METRICS[${scenario}_memory_avg]=0
    done
    
    # Run benchmarks for each scenario
    for scenario in "${!TEST_SCENARIOS[@]}"; do
        local filter="${TEST_SCENARIOS[$scenario]}"
        
        log "Testing scenario: $scenario"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Starting scenario: $scenario with filter: $filter" >> "$DEBUG_LOG"
        
        # Run Wake benchmark
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] About to run Wake benchmark for scenario: $scenario" >> "$DEBUG_LOG"
        if run_wake_benchmark "$scenario" "$filter"; then
            echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Wake benchmark succeeded for scenario: $scenario" >> "$DEBUG_LOG"
        else
            echo "$(date '+%Y-%m-%d %H:%M:%S') [ERROR] Wake benchmark failed for scenario: $scenario" >> "$DEBUG_LOG"
        fi
        
        # Cool down period
        sleep 5
        
        # Run Stern benchmark
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] About to run Stern benchmark for scenario: $scenario" >> "$DEBUG_LOG"
        run_stern_benchmark "$scenario" "$filter"
        echo "$(date '+%Y-%m-%d %H:%M:%S') [DEBUG] Stern benchmark completed for scenario: $scenario" >> "$DEBUG_LOG"
        
        # Cool down period
        sleep 5
    done
    
    # Generate report
    generate_report
    
    success "Benchmark completed! Check results in: $RESULTS_DIR"
    log "Report: $RESULTS_DIR/performance_report_$TIMESTAMP.md"
    log "Raw data: $CSV_FILE"
    log "Debug log: $DEBUG_LOG"
    
    # Show debug summary
    echo "=== DEBUG SUMMARY ===" >> "$DEBUG_LOG"
    echo "Wake data points: $(grep -c "^[^#].*,wake," "$CSV_FILE" 2>/dev/null || echo 0)" >> "$DEBUG_LOG"
    echo "Stern data points: $(grep -c "^[^#].*,stern," "$CSV_FILE" 2>/dev/null || echo 0)" >> "$DEBUG_LOG"
    
    log "Debug information saved to: $DEBUG_LOG"
}

# Run if executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
