#!/bin/bash

# Performance Testing Environment Cleanup Script
# Removes all performance testing resources from the cluster

set -e

echo "=================================================================="
echo "Cleaning up Wake Performance Testing Environment"
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

# Check if perf-test namespace exists
if ! kubectl get namespace perf-test &> /dev/null; then
    echo "⚠️  Performance test namespace 'perf-test' not found"
    echo "   Nothing to clean up"
    exit 0
fi

echo ""
echo "📊 Current performance test environment status:"
kubectl get pods -n perf-test --no-headers | wc -l | xargs echo "   Active pods:"
kubectl get deployments -n perf-test --no-headers | wc -l | xargs echo "   Active deployments:"

echo ""
read -p "🗑️  Are you sure you want to delete the entire perf-test environment? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Cleanup cancelled"
    exit 0
fi

echo ""
echo "🧹 Cleaning up performance test resources..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Delete in reverse order
echo "   Removing generators 9-10..."
kubectl delete -f "$SCRIPT_DIR/04-generators-9-10.yaml" --ignore-not-found=true

echo "   Removing generators 6-8..."
kubectl delete -f "$SCRIPT_DIR/03-generators-6-8.yaml" --ignore-not-found=true

echo "   Removing generators 3-5..."
kubectl delete -f "$SCRIPT_DIR/02-generators-3-5.yaml" --ignore-not-found=true

echo "   Removing generators 1-2..."
kubectl delete -f "$SCRIPT_DIR/01-generators-1-2.yaml" --ignore-not-found=true

echo "   Removing namespace and configuration..."
kubectl delete -f "$SCRIPT_DIR/00-namespace-config.yaml" --ignore-not-found=true

echo ""
echo "⏳ Waiting for resources to be fully removed..."
kubectl wait --for=delete namespace/perf-test --timeout=60s || true

echo ""
echo "🔍 Verifying cleanup..."
if kubectl get namespace perf-test &> /dev/null; then
    echo "⚠️  Namespace still exists (may take a few more seconds to fully terminate)"
    echo "   You can check status with: kubectl get namespace perf-test"
else
    echo "✅ Performance test environment completely removed"
fi

echo ""
echo "📝 Cleanup Summary:"
echo "=================================================================="
echo "   ✅ All performance test deployments removed"
echo "   ✅ All performance test pods terminated"
echo "   ✅ Performance test namespace deleted"
echo "   ✅ Configuration and secrets cleaned up"

echo ""
echo "💡 Next steps:"
echo "   • Run './setup-perf.sh' to recreate the environment"
echo "   • Check remaining resources: kubectl get all --all-namespaces"
echo "=================================================================="