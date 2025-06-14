# Performance Testing Environment

This directory contains resources for performance testing Wake with high-throughput log generation.

## Overview

The performance test environment deploys:
- **10 high-throughput log generator pods** (2 replicas each = 20 total pods)
- **Multiple log patterns** to test filtering performance
- **Configurable log rates** from 10-1000 logs per second per pod
- **Memory and CPU stressed containers** to simulate real workloads

## Quick Start

```bash
# Deploy performance test environment
./setup-perf.sh

# Test Wake with high load
wake -n perf-test ".*" --ui

# Test with advanced filtering
wake -n perf-test ".*" --ui -i "(ERROR || WARN) && user"

# Clean up
./cleanup-perf.sh
```

## Performance Test Scenarios

### Scenario 1: Basic Load Test
- 20 pods generating 50 logs/second each (1000 logs/second total)
- Mixed log levels (INFO, WARN, ERROR, DEBUG)
- Basic regex filtering

### Scenario 2: High Throughput Test  
- 20 pods generating 200 logs/second each (4000 logs/second total)
- Advanced pattern filtering with logical operators
- File output + UI mode simultaneously

### Scenario 3: Extreme Load Test
- 20 pods generating 500 logs/second each (10,000 logs/second total)
- Complex multi-pattern filtering
- Memory usage and CPU performance monitoring

## Monitoring Performance

Use these commands to monitor Wake's performance:

```bash
# Monitor memory usage
ps aux | grep wake

# Monitor CPU usage
top -p $(pgrep wake)

# Monitor log processing rate
wake -n perf-test ".*" --dev  # Shows internal metrics
```

## Configuration

Edit `perf-config.yaml` to adjust:
- Number of pods (default: 10 deployments x 2 replicas = 20 pods)
- Log generation rate (default: 50-500 logs/second per pod)
- Log message patterns and complexity
- Resource limits and requests