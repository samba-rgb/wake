# Wake Development Environment

This directory contains resources for setting up a local test environment for Wake development.

## Directory Structure

```
dev/
├── manifests/           # Kubernetes manifest files
│   ├── 00-namespaces.yaml    # Creates apps and monitoring namespaces
│   ├── 01-nginx.yaml         # Nginx deployment with sidecar
│   ├── 02-database.yaml      # PostgreSQL statefulset
│   └── 03-monitoring.yaml    # Prometheus and Grafana deployments
└── scripts/            # Helper scripts
    ├── setup.sh        # Sets up the test environment
    └── cleanup.sh      # Removes test resources
```

## Prerequisites

- A running Kubernetes cluster (minikube, kind, or other)
- kubectl installed and configured
- Proper access rights to create namespaces and resources

## Quick Start

1. Start your Kubernetes cluster:
   ```bash
sudo systemctl start docker.service
sudo systemctl enable docker.service

kind create cluster

>
   # Or using kind
   kind create cluster --name wake-test
   ```

2. Make scripts executable:
   ```bash
   chmod +x scripts/*.sh
   ```

3. Set up the test environment:
   ```bash
   ./scripts/setup.sh
   ```

4. Test wake with various scenarios:
   ```bash
   # Watch nginx pods and their sidecars
   wake -n apps "nginx"
   
   # Watch database pods
   wake -n apps "postgres"
   
   # Watch monitoring stack
   wake -n monitoring
   ```

5. Clean up when done:
   ```bash
   ./scripts/cleanup.sh
   ```

## Test Resources

The test environment includes:

### Apps Namespace
- Nginx deployment (3 replicas)
  - Main nginx container
  - Sidecar container generating logs
- PostgreSQL statefulset
  - Single replica with persistent storage

### Monitoring Namespace
- Prometheus deployment
- Grafana deployment


1. Start your Kubernetes cluster:
   ```bash
   # Using minikube
   sudo minikube start --driver=docker --force

   # Or using kind
   kind create cluster --name wake-test

Stopping Minikube and Docker
Stop Minikube:

Delete Minikube Cluster (Optional):

Stop Docker:

Disable Docker from Starting on Boot (Optional):




sudo minikube start --driver=docker --forcesudo usermod -aG docker $USER

Each deployment has appropriate labels for testing Wake's label filtering capabilities.