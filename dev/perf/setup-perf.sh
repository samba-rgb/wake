#!/bin/bash

# Performance Testing Environment Setup Script
# Deploys 10 high-throughput log generator deployments for Wake performance testing

set -e

echo "=================================================================="
echo "Setting up Wake Performance Testing Environment"
echo "=================================================================="

# Check if kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo "❌ Error: kubectl is not installed or not in PATH"
    exit 1
fi

# Check if we can connect to a Kubernetes cluster
if ! kubectl cluster-info &> /dev/null; then
    echo "❌ Error: Cannot connect to Kubernetes cluster"
    echo "   Please check your kubeconfig and cluster connectivity"
    exit 1
fi

echo "✅ Kubernetes cluster connection verified"

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo ""
echo "📦 Deploying performance testing namespace and configuration..."
kubectl apply -f "$SCRIPT_DIR/00-namespace-config.yaml"

echo ""
echo "🚀 Deploying log generator pods (this may take a few minutes)..."

# Deploy generators in batches to avoid overwhelming the cluster
echo "   Deploying generators 1-2..."
kubectl apply -f "$SCRIPT_DIR/01-generators-1-2.yaml"

echo "   Deploying generators 3-5..."
kubectl apply -f "$SCRIPT_DIR/02-generators-3-5.yaml"

echo "   Deploying generators 6-8..."
kubectl apply -f "$SCRIPT_DIR/03-generators-6-8.yaml"

echo "   Deploying generators 9-10..."
kubectl apply -f "$SCRIPT_DIR/04-generators-9-10.yaml"

echo ""
echo "⏳ Waiting for pods to be ready..."

# Wait for all deployments to be ready
for i in {01..10}; do
    echo "   Waiting for perf-generator-$i..."
    kubectl rollout status deployment/perf-generator-$i -n perf-test --timeout=300s
done

echo ""
echo "📊 Performance Test Environment Status:"
echo "=================================================================="

# Show pod status
kubectl get pods -n perf-test -o wide

echo ""
echo "📈 Expected Log Throughput:"
echo "   Generator 01: 100 logs/sec × 2 pods =   200 logs/sec"
echo "   Generator 02: 200 logs/sec × 2 pods =   400 logs/sec"
echo "   Generator 03: 500 logs/sec × 2 pods = 1,000 logs/sec"
echo "   Generator 04: 150 logs/sec × 2 pods =   300 logs/sec"
echo "   Generator 05: 125 logs/sec × 2 pods =   250 logs/sec"
echo "   Generator 06: 100 logs/sec × 2 pods =   200 logs/sec"
echo "   Generator 07: 333 logs/sec × 2 pods =   666 logs/sec"
echo "   Generator 08: 250 logs/sec × 2 pods =   500 logs/sec"
echo "   Generator 09: 167 logs/sec × 2 pods =   334 logs/sec"
echo "   Generator 10: Variable burst mode  =   ~400 logs/sec avg"
echo "   ────────────────────────────────────────────────────────────"
echo "   TOTAL EXPECTED THROUGHPUT:        ~4,250 logs/second"

echo ""
echo "🧪 Performance Test Commands:"
echo "=================================================================="
echo "# Basic load test (all generators)"
echo "wake -n perf-test \".*\" --ui"
echo ""
echo "# Test advanced filtering performance"
echo "wake -n perf-test \".*\" --ui -i '(ERROR || WARN) && \"user\"'"
echo ""
echo "# Test file output + UI performance"
echo "wake -n perf-test \".*\" --ui -w perf-test-logs.txt"
echo ""
echo "# Test specific generator groups"
echo "wake -n perf-test \"perf-generator-03.*\" --ui  # Burst mode (1000 logs/s)"
echo "wake -n perf-test \"perf-generator-08.*\" --ui  # Error-heavy logs"
echo "wake -n perf-test \"perf-generator-10.*\" --ui  # Extreme burst mode"
echo ""
echo "# Monitor performance with development mode"
echo "wake -n perf-test \".*\" --ui --dev"

echo ""
echo "🎯 Test Scenarios:"
echo "=================================================================="
echo "1. Baseline Performance:"
echo "   wake -n perf-test \"perf-generator-01.*\" --ui"
echo ""
echo "2. High Throughput (4,000+ logs/sec):"
echo "   wake -n perf-test \".*\" --ui"
echo ""
echo "3. Complex Filtering:"
echo "   wake -n perf-test \".*\" --ui -i '(ERROR || WARN) && \"transaction\"'"
echo ""
echo "4. File Output Performance:"
echo "   wake -n perf-test \".*\" --ui -w /tmp/perf-test.log"
echo ""
echo "5. Memory/CPU Stress Test:"
echo "   wake -n perf-test \".*\" --ui --threads 8"

echo ""
echo "✅ Performance test environment is ready!"
echo "   Use 'kubectl get pods -n perf-test' to monitor pod status"
echo "   Use './cleanup-perf.sh' to clean up when done"
echo "=================================================================="