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
#kubectl -n apps wait --for=condition=ready --timeout=60s statefulset/postgres
kubectl -n monitoring wait --for=condition=available --timeout=60s deployment/prometheus
kubectl -n monitoring wait --for=condition=available --timeout=60s deployment/grafana

echo "Generating sample logs..."
NGINX_POD=$(kubectl -n apps get pod -l app=web -o jsonpath="{.items[0].metadata.name}")

if [ -z "$NGINX_POD" ]; then
  echo "Error: No pod found with label app=nginx in namespace apps"
  exit 1
fi

kubectl -n apps exec "$NGINX_POD" -- sh -c 'for i in {1..5}; do echo "Test log entry $i"; sleep 1; done'

echo "Test environment is ready!"
echo "Try these commands to test wake:"
echo "  wake -n apps -l app=nginx"
echo "  wake -n monitoring -l app=monitoring"
echo "  wake -A"
