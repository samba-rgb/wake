# Performance Benchmark Report

**Date:** 2025-06-15 19:09:47
**Duration per test:** 120s
**Warmup duration:** 10s
**Log generation rate:** ~14,000 logs/second

## Test Environment

- **Kubernetes cluster:** kind-kind
- **Wake version:** wake 0.2.0
- **Stern version:** version: 1.28.0
commit: 9763d953d86d8f60cf9a5e5da3531e6a1aa47c5c
built at: 2024-01-09T00:32:07Z
- **System:** Linux sam-power-house 6.12.28-1-MANJARO #1 SMP PREEMPT_DYNAMIC Fri, 09 May 2025 10:53:27 +0000 x86_64 GNU/Linux

## Test Scenarios


### Scenario: complex_filter
**Filter:** `ERROR|WARN`

| Metric | Wake | Stern | Winner |
|--------|------|-------|--------|
| Max CPU (%) | 15.0 | 0.0 | N/A |
| Avg CPU (%) | 13.7 | 0.0 | N/A |
| Max Memory (MB) | 18.0 | 4.0 | Stern |
| Avg Memory (MB) | 18.0 | 4.0 | Stern |


## Raw Data

The complete performance data is available in: `/home/samba/Desktop/projects/wake/dev/perf/benchmark_results/benchmark_20250615_190512.csv`

## Visualizations

To generate graphs from the data:

```bash
# Install required tools
pip install matplotlib pandas

# Generate performance graphs
python3 visualize_performance.py /home/samba/Desktop/projects/wake/dev/perf/benchmark_results/benchmark_20250615_190512.csv
```

## Summary

Performance benchmark completed successfully.
Wake demonstrated 1 wins vs Stern's 0 wins across all scenarios.

Note: This was a 2-minute test. Check the debug log for any issues with Wake execution.

