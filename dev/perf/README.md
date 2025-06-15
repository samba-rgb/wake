# Performance Testing for Wake

This directory contains performance testing tools and configurations for benchmarking Wake against other Kubernetes log streaming tools like Stern.

## Quick Start

1. **Setup the performance test environment:**
   ```bash
   ./setup-perf.sh
   ```

2. **Run the full benchmark suite:**
   ```bash
   ./benchmark.sh
   ```

3. **Generate performance visualizations:**
   ```bash
   python3 visualize_performance.py ./benchmark_results/benchmark_TIMESTAMP.csv
   ```

## Performance Test Environment

The performance test environment generates **~14,000 logs per second** across multiple scenarios:

- **10 deployments** with 2 replicas each (20 pods total)
- **Diverse log patterns:** microservices, databases, security, IoT, financial trading, etc.
- **Multiple log levels:** INFO, DEBUG, WARN, ERROR with realistic distributions
- **Burst patterns:** Some generators include burst modes for peak load testing

## Benchmark Script Features

### `benchmark.sh`

**Comprehensive performance comparison tool that measures:**

- **CPU Usage:** Average and peak CPU consumption
- **Memory Usage:** RSS, VMS, and total memory consumption
- **Performance Metrics:** Collected every 2 seconds during test runs
- **Multiple Scenarios:** Tests different filtering patterns and use cases

**Test Scenarios:**
- `basic`: Stream all logs (`.*`)
- `error_filter`: Filter only ERROR logs
- `complex_filter`: Complex boolean logic `(ERROR || WARN) && (user || auth || payment)`
- `json_filter`: Filter JSON-formatted logs
- `high_frequency`: High-throughput streaming test

**Configuration:**
- **Test Duration:** 5 minutes per scenario (configurable)
- **Warmup Period:** 30 seconds before monitoring starts
- **Cool-down:** 10 seconds between tests

### `visualize_performance.py`

**Advanced visualization and analysis tool that generates:**

1. **CPU Comparison Charts:**
   - CPU usage over time
   - Average CPU by scenario
   - Maximum CPU peaks
   - CPU usage distribution (boxplots)

2. **Memory Comparison Charts:**
   - Memory usage over time
   - Average memory by scenario
   - Maximum memory peaks
   - Memory usage distribution

3. **Performance Summary:**
   - Overall performance scores
   - Resource efficiency scatter plots
   - Winner analysis per scenario

4. **Detailed Analysis Report:**
   - Statistical breakdown by scenario
   - Performance insights and recommendations
   - Data quality metrics

## Usage Examples

### Basic Benchmark
```bash
# Run full benchmark suite
./benchmark.sh

# Results will be saved to ./benchmark_results/
```

### Custom Benchmark Duration
```bash
# Edit benchmark.sh to change duration
BENCHMARK_DURATION=600  # 10 minutes per test
WARMUP_DURATION=60     # 1 minute warmup
```

### Generate Visualizations
```bash
# Install required Python packages
pip install matplotlib pandas seaborn

# Generate charts from benchmark data
python3 visualize_performance.py ./benchmark_results/benchmark_20250615_143022.csv

# Specify custom output directory
python3 visualize_performance.py ./benchmark_results/benchmark_20250615_143022.csv -o ./charts/
```

### Quick Performance Check
```bash
# Run just the high-throughput log generation
./setup-perf.sh

# Test Wake manually
wake -n perf-test "ERROR" --no-ui &
WAKE_PID=$!

# Monitor with top/htop
top -p $WAKE_PID

# Clean up
kill $WAKE_PID
```

## Output Files

After running benchmarks, you'll find these files in `./benchmark_results/`:

```
benchmark_results/
├── benchmark_20250615_143022.log          # Detailed execution log
├── benchmark_20250615_143022.csv          # Raw performance data
├── performance_report_20250615_143022.md  # Summary report
├── cpu_comparison.png                     # CPU performance charts
├── memory_comparison.png                  # Memory performance charts
├── performance_summary.png                # Overall performance summary
└── detailed_analysis.md                   # Statistical analysis
```

## Performance Metrics Collected

**Per Process Monitoring:**
- **CPU Percentage:** Real-time CPU usage
- **Memory (RSS):** Physical memory usage
- **Memory (VMS):** Virtual memory size
- **Process Stats:** From `/proc/PID/stat` and `/proc/PID/status`
- **Logs Processed:** Estimated throughput
- **Duration:** Test execution time

**System Requirements:**
- Linux system with `/proc` filesystem
- `sysstat` package (for `pidstat` - auto-installed)
- Python 3.7+ with pandas, matplotlib, seaborn
- kubectl access to Kubernetes cluster
- Stern installed for comparison testing

## Interpreting Results

### CPU Performance
- **Lower is better** for both average and peak CPU usage
- Look for consistent performance across scenarios
- Watch for CPU spikes during high-load periods

### Memory Performance  
- **Lower is better** for memory consumption
- Check for memory leaks (increasing usage over time)
- RSS (Resident Set Size) shows actual physical memory usage

### Performance Score
- Composite metric combining CPU and memory efficiency
- **Lower scores indicate better performance**
- Weighted formula: `CPU% + (Memory_MB / 10)`

## Troubleshooting

### Common Issues

**"Wake binary not found"**
```bash
cd ../.. && cargo build --release && cd dev/perf
```

**"Stern is not installed"**
```bash
curl -LO https://github.com/stern/stern/releases/latest/download/stern_linux_amd64.tar.gz
tar -xzf stern_linux_amd64.tar.gz
sudo mv stern /usr/local/bin/
```

**"No data in results"**
- Check if perf-test namespace exists: `kubectl get ns perf-test`
- Verify log generators are running: `kubectl get pods -n perf-test`
- Check benchmark log files for error messages

**Python visualization errors**
```bash
pip install --upgrade matplotlib pandas seaborn
```

## Advanced Configuration

### Custom Test Scenarios

Edit `benchmark.sh` to add custom scenarios:

```bash
declare -A TEST_SCENARIOS=(
    ["basic"]=".*"
    ["error_filter"]="ERROR"
    ["custom_service"]="payment-service.*ERROR"
    ["high_volume"]=".*"
)
```

### Performance Tuning

**For higher throughput testing:**
- Increase log generation rates in generator YAML files
- Adjust `sleep` intervals in log generation scripts
- Scale up generator replicas

**For longer duration tests:**
- Modify `BENCHMARK_DURATION` in `benchmark.sh`
- Increase Kubernetes resource limits for generators
- Monitor disk space for log storage

## Integration with CI/CD

```bash
# Example CI pipeline integration
./setup-perf.sh
./benchmark.sh
python3 visualize_performance.py ./benchmark_results/benchmark_*.csv

# Upload results to artifact storage
# Send performance alerts if regression detected
```

This performance testing framework provides comprehensive insights into Wake's efficiency compared to existing tools like Stern, helping identify optimization opportunities and validate performance improvements.