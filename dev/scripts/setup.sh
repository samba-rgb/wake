#!/bin/bash
set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
MANIFESTS_DIR="$SCRIPT_DIR/../manifests"

if ! command -v kubectl &> /dev/null; then
    echo "Error: kubectl is not installed"
    exit 1
fi

if ! kubectl cluster-info &> /dev/null; then
    echo "Error: Unable to connect to Kubernetes cluster"
    exit 1
fi

echo "Setting up test environment..."

for manifest in $(ls -1 $MANIFESTS_DIR/*.yaml | sort); do
    echo "Applying $manifest..."
    kubectl apply -f "$manifest"
done

echo "Waiting for deployments to be ready..."
kubectl -n apps wait --for=condition=available --timeout=60s deployment/nginx
kubectl -n apps wait --for=condition=available --timeout=60s deployment/log-generator
#kubectl -n apps wait --for=condition=ready --timeout=60s statefulset/postgres
kubectl -n monitoring wait --for=condition=available --timeout=60s deployment/prometheus
kubectl -n monitoring wait --for=condition=available --timeout=60s deployment/grafana

echo "Test environment is ready!"
echo ""
echo "Available pods for testing:"
echo "- Log generator pods (2 replicas) in 'apps' namespace"
echo "- Nginx pods in 'apps' namespace"
echo "- Prometheus and Grafana in 'monitoring' namespace"
echo ""
echo "Try these commands to test wake:"
echo "  # Watch log generator with random logs"
echo "  wake -n apps log-generator"
echo ""
echo "  # Watch all containers in log generator pods"
echo "  wake -n apps log-generator --all-containers"
echo ""
echo "  # Watch only the main generator container"
echo "  wake -n apps log-generator -c generator"
echo ""
echo "  # Watch only the sidecar container"
echo "  wake -n apps log-generator -c sidecar-logger"
echo ""
echo "  # Watch all apps namespace pods"
echo "  wake -n apps '.*'"
echo ""
echo "  # Watch all namespaces"
echo "  wake -A"
echo ""
echo "  # Filter logs by level"
echo "  wake -n apps log-generator --include 'ERROR|WARN'"
echo ""
echo "  # Exclude debug logs"
echo "  wake -n apps log-generator --exclude 'DEBUG|TRACE'"
