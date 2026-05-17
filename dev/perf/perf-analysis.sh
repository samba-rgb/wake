#!/bin/bash

# Tool-backed performance analysis runner for Wake.
#
# CPU profiling:
#   Uses samply to capture sampling profiles for sustained wake runs.
#
# Wall-clock analysis:
#   Uses hyperfine to compare finite commands. Since wake normally follows logs
#   forever, WALLCLOCK_WAKE_ARGS should make the workload complete, for example:
#     WALLCLOCK_WAKE_ARGS='-n apps "nginx" --follow false --tail 1000'
#
# Allocation analysis:
#   Uses heaptrack on Linux, or xctrace Allocations on macOS when available.
#
# Filter scenario:
#   Defaults to a real log token from dev/muli-lang-pod.yaml.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WAKE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/perf_analysis_results}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
RUN_DIR="$RESULTS_DIR/$TIMESTAMP"
PROFILES_DIR="$RUN_DIR/samply"
WALLCLOCK_DIR="$RUN_DIR/wallclock"
ALLOC_DIR="$RUN_DIR/allocations"
LOG_DIR="$RUN_DIR/logs"
WAKE_BIN="${WAKE_BIN:-$WAKE_ROOT/target/release/wake}"
PROFILE_DURATION_SECONDS="${PROFILE_DURATION_SECONDS:-30}"
SAMPLY_RATE="${SAMPLY_RATE:-1000}"
HYPERFINE_RUNS="${HYPERFINE_RUNS:-7}"
HYPERFINE_WARMUP="${HYPERFINE_WARMUP:-1}"
WAKE_ARGS="${WAKE_ARGS:-}"
WALLCLOCK_WAKE_ARGS="${WALLCLOCK_WAKE_ARGS:-$WAKE_ARGS --follow false --tail 1000}"
WAKE_FILTER_PATTERN="${WAKE_FILTER_PATTERN:-\"System health-check\"}"
TIMEBOX="$SCRIPT_DIR/timebox.py"

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

require_command() {
  local command_name="$1"
  local install_hint="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name"
    echo "Install: $install_hint"
    exit 1
  fi
}

check_dependencies() {
  local mode="$1"

  if [[ ! -x "$WAKE_BIN" ]]; then
    echo "Wake binary not found at: $WAKE_BIN"
    echo "Build it first with: cargo build --release"
    exit 1
  fi

  require_command "python3" "Use your system package manager to install Python 3."

  if [[ "$mode" == "profile" || "$mode" == "all" ]]; then
    require_command "samply" "cargo install samply"
  fi

  if [[ "$mode" == "wallclock" || "$mode" == "all" ]]; then
    require_command "hyperfine" "brew install hyperfine  # or: cargo install hyperfine"
  fi

  if [[ "$mode" == "alloc" ]]; then
    if ! allocation_tool >/dev/null; then
      echo "Missing allocation profiler."
      echo "Install one of:"
      echo "  macOS: install Xcode Instruments so 'xcrun xctrace' is available"
      echo "  Linux: install heaptrack"
      exit 1
    fi
  fi
}

scenario_args() {
  case "$1" in
    wake)
      printf '%s' ""
      ;;
    wake_ui)
      printf '%s' "--ui"
      ;;
    wake_filter)
      printf '%s' "-i '$WAKE_FILTER_PATTERN'"
      ;;
    wake_web)
      printf '%s' "--web"
      ;;
    *)
      echo "unknown scenario: $1" >&2
      return 1
      ;;
  esac
}

build_wake_command() {
  local scenario="$1"
  WAKE_COMMAND=( "$WAKE_BIN" )

  if [[ -n "$WAKE_ARGS" ]]; then
    # WAKE_ARGS is intentionally shell-style so callers can pass quoted selectors.
    eval "WAKE_COMMAND+=( $WAKE_ARGS )"
  fi

  case "$scenario" in
    wake)
      ;;
    wake_ui)
      WAKE_COMMAND+=( "--ui" )
      ;;
    wake_filter)
      WAKE_COMMAND+=( "-i" "$WAKE_FILTER_PATTERN" )
      ;;
    wake_web)
      WAKE_COMMAND+=( "--web" )
      ;;
    *)
      echo "unknown scenario: $scenario" >&2
      return 1
      ;;
  esac
}

format_command() {
  printf '%q ' "$@"
}

run_samply_profiles() {
  local scenario output log_file wake_log wake_pid samply_status

  for scenario in wake wake_ui wake_filter wake_web; do
    build_wake_command "$scenario"
    output="$PROFILES_DIR/${scenario}.json.gz"
    log_file="$LOG_DIR/${scenario}.samply.log"
    wake_log="$LOG_DIR/${scenario}.wake.log"

    log "Profiling $scenario with samply for ${PROFILE_DURATION_SECONDS}s"
    log "  command: $(format_command "${WAKE_COMMAND[@]}")"

    "${WAKE_COMMAND[@]}" >"$wake_log" 2>&1 &
    wake_pid="$!"
    sleep 1

    if ! kill -0 "$wake_pid" 2>/dev/null; then
      log "  skipped: wake exited before samply could attach; see $wake_log"
      wait "$wake_pid" 2>/dev/null || true
      continue
    fi

    set +e
    python3 "$TIMEBOX" \
      --duration "$PROFILE_DURATION_SECONDS" \
      --signal INT \
      -- samply record \
      --save-only \
      --rate "$SAMPLY_RATE" \
      --output "$output" \
      --pid "$wake_pid" \
      >"$log_file" 2>&1
    samply_status="$?"
    set -e

    kill "$wake_pid" 2>/dev/null || true
    wait "$wake_pid" 2>/dev/null || true

    if [[ -s "$output" ]]; then
      log "  profile written: $output"
    elif [[ "$samply_status" -ne 0 ]]; then
      log "  samply failed for $scenario; see $log_file"
    fi
  done
}

allocation_tool() {
  if command -v heaptrack >/dev/null 2>&1; then
    echo "heaptrack"
    return 0
  fi

  if command -v xcrun >/dev/null 2>&1 && xcrun xctrace help >/dev/null 2>&1; then
    echo "xctrace"
    return 0
  fi

  return 1
}

run_allocation_profiles() {
  local tool scenario args output log_file

  if ! tool="$(allocation_tool)"; then
    log "Skipping allocation profiles: heaptrack/xctrace is not available"
    return 0
  fi

  for scenario in wake wake_ui wake_filter wake_web; do
    args="$(scenario_args "$scenario")"
    log_file="$LOG_DIR/${scenario}.alloc.log"

    log "Profiling allocations for $scenario with $tool"

    if [[ "$tool" == "heaptrack" ]]; then
      output="$ALLOC_DIR/${scenario}.heaptrack"
      heaptrack \
        --output "$output" \
        python3 "$TIMEBOX" \
        --duration "$PROFILE_DURATION_SECONDS" \
        -- bash -lc "exec '$WAKE_BIN' $WAKE_ARGS $args" \
        >"$log_file" 2>&1 || true
    else
      output="$ALLOC_DIR/${scenario}.trace"
      xcrun xctrace record \
        --template "Allocations" \
        --time-limit "${PROFILE_DURATION_SECONDS}s" \
        --output "$output" \
        --launch -- \
        python3 "$TIMEBOX" \
        --duration "$PROFILE_DURATION_SECONDS" \
        -- bash -lc "exec '$WAKE_BIN' $WAKE_ARGS $args" \
        >"$log_file" 2>&1 || true
    fi
  done
}

write_hyperfine_command_file() {
  local command_file="$WALLCLOCK_DIR/commands.txt"
  : > "$command_file"

  local scenario args
  for scenario in wake wake_ui wake_filter wake_web; do
    args="$(scenario_args "$scenario")"
    printf '%s\n' "python3 '$TIMEBOX' --duration '$PROFILE_DURATION_SECONDS' -- bash -lc \"exec '$WAKE_BIN' $WALLCLOCK_WAKE_ARGS $args\"" >> "$command_file"
  done

  echo "$command_file"
}

run_wallclock_benchmarks() {
  local command_file="$1"

  log "Running wall-clock analysis with hyperfine"
  log "  runs: $HYPERFINE_RUNS, warmup: $HYPERFINE_WARMUP"
  log "  finite workload args: ${WALLCLOCK_WAKE_ARGS:-<none>}"

  hyperfine \
    --warmup "$HYPERFINE_WARMUP" \
    --runs "$HYPERFINE_RUNS" \
    --ignore-failure \
    --export-json "$WALLCLOCK_DIR/hyperfine.json" \
    --export-markdown "$WALLCLOCK_DIR/hyperfine.md" \
    --command-name "wake" "$(sed -n '1p' "$command_file")" \
    --command-name "wake --ui" "$(sed -n '2p' "$command_file")" \
    --command-name "wake -i '$WAKE_FILTER_PATTERN'" "$(sed -n '3p' "$command_file")" \
    --command-name "wake --web" "$(sed -n '4p' "$command_file")" \
    > "$WALLCLOCK_DIR/hyperfine.log"
}

write_runbook() {
  cat > "$RUN_DIR/README.md" <<EOF
# Wake perf analysis

Generated: $TIMESTAMP

## Samply profiles

Profiles are in:

\`\`\`
$PROFILES_DIR
\`\`\`

Open a profile with:

\`\`\`
samply load $PROFILES_DIR/wake.json.gz
\`\`\`

## Wall-clock results

Hyperfine output:

\`\`\`
$WALLCLOCK_DIR/hyperfine.md
$WALLCLOCK_DIR/hyperfine.json
\`\`\`

## Allocation profiles

Allocation outputs, when an allocation profiler is available:

\`\`\`
$ALLOC_DIR
\`\`\`

## Inputs

- WAKE_BIN: \`$WAKE_BIN\`
- WAKE_ARGS: \`${WAKE_ARGS:-}\`
- WALLCLOCK_WAKE_ARGS: \`${WALLCLOCK_WAKE_ARGS:-}\`
- WAKE_FILTER_PATTERN: \`$WAKE_FILTER_PATTERN\`
- PROFILE_DURATION_SECONDS: \`$PROFILE_DURATION_SECONDS\`
- SAMPLY_RATE: \`$SAMPLY_RATE\`
- HYPERFINE_RUNS: \`$HYPERFINE_RUNS\`
- HYPERFINE_WARMUP: \`$HYPERFINE_WARMUP\`
EOF
}

usage() {
  cat <<'EOF'
Usage: ./perf-analysis.sh [profile|wallclock|alloc|all]

Environment:
  WAKE_ARGS                 Args shared by sustained samply runs.
  WALLCLOCK_WAKE_ARGS       Args for finite hyperfine runs.
  WAKE_FILTER_PATTERN       Filter used by the wake -i scenario.
  PROFILE_DURATION_SECONDS  Duration for samply and timeboxed runs. Default: 30.
  SAMPLY_RATE               Samply sampling rate. Default: 1000.
  HYPERFINE_RUNS            Wall-clock benchmark runs. Default: 7.
  HYPERFINE_WARMUP          Hyperfine warmup runs. Default: 1.

Allocation tools:
  macOS: xcrun xctrace with the Allocations template
  Linux: heaptrack

Example:
  WAKE_ARGS='-n apps "nginx"' \
  WALLCLOCK_WAKE_ARGS='-n apps "nginx" --follow false --tail 1000' \
  WAKE_FILTER_PATTERN='"System health-check"' \
  ./perf-analysis.sh all
EOF
}

main() {
  local mode="${1:-all}"

  if [[ "$mode" == "--help" || "$mode" == "-h" ]]; then
    usage
    exit 0
  fi

  case "$mode" in
    profile)
      check_dependencies "$mode"
      mkdir -p "$PROFILES_DIR" "$WALLCLOCK_DIR" "$ALLOC_DIR" "$LOG_DIR"
      run_samply_profiles
      ;;
    wallclock)
      check_dependencies "$mode"
      mkdir -p "$PROFILES_DIR" "$WALLCLOCK_DIR" "$ALLOC_DIR" "$LOG_DIR"
      run_wallclock_benchmarks "$(write_hyperfine_command_file)"
      ;;
    alloc)
      check_dependencies "$mode"
      mkdir -p "$PROFILES_DIR" "$WALLCLOCK_DIR" "$ALLOC_DIR" "$LOG_DIR"
      run_allocation_profiles
      ;;
    all)
      check_dependencies "$mode"
      mkdir -p "$PROFILES_DIR" "$WALLCLOCK_DIR" "$ALLOC_DIR" "$LOG_DIR"
      run_samply_profiles
      run_wallclock_benchmarks "$(write_hyperfine_command_file)"
      run_allocation_profiles
      ;;
    *)
      usage
      exit 1
      ;;
  esac

  write_runbook
  log "Analysis output: $RUN_DIR"
}

main "$@"
