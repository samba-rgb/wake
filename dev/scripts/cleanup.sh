#!/bin/bash
set -e

echo "Cleaning up test environment..."

# Delete namespaces (this will delete all resources in them)
kubectl delete namespace apps --ignore-not-found
kubectl delete namespace monitoring --ignore-not-found

echo "Test environment cleaned up!"