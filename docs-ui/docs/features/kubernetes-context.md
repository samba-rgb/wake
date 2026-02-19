---
sidebar_position: 7
---

# Kubernetes Context Management

Wake provides robust Kubernetes context management capabilities, allowing you to seamlessly work across multiple clusters and namespaces. This feature is essential for teams managing development, staging, and production environments.

## Overview

Wake automatically detects and works with your current Kubernetes configuration, supporting multiple contexts and providing easy switching between different clusters and namespaces.

## Basic Context Usage

### Recommended: Using kubectx for Easy Context Switching

For easier Kubernetes context management, we highly recommend using **kubectx** - a tool that simplifies switching between contexts:

**Install kubectx:**
```bash
# macOS with Homebrew
brew install kubectx

# Linux
curl -Lo kubectx https://github.com/ahmetb/kubectx/releases/latest/download/kubectx
chmod +x kubectx
sudo mv kubectx /usr/local/bin/

# Or using package managers
# Ubuntu/Debian: sudo apt install kubectx
# Arch Linux: pacman -S kubectx
```

**Usage with Wake:**
```bash
# List all contexts (much cleaner than kubectl)
kubectx

# Switch to a context quickly
kubectx staging-cluster

# Now use Wake with the selected context
wake --ui

# Switch and monitor in one workflow
kubectx production && wake --namespace apps --ui
```

**kubectx with kubens (namespace switching):**
```bash
# Install kubens along with kubectx
brew install kubectx  # includes both tools

# Switch context and namespace quickly
kubectx staging-cluster
kubens apps
wake --ui

# Or combine in commands
kubectx prod && kubens production && wake --ui
```

### Using Current Context
```bash
# Wake uses your current kubectl context by default
wake --ui

# Check which context Wake is using
kubectl config current-context
# or with kubectx
kubectx -c
```

### Specifying a Different Context
```bash
# Use a specific context for Wake operations
wake --context staging-cluster --ui

# Combine with namespace selection
wake --context production --namespace apps --ui

# With kubectx workflow (recommended)
kubectx staging-cluster
wake --namespace apps --ui
```

## Context Configuration

### Listing Available Contexts
```bash
# List all available Kubernetes contexts
kubectl config get-contexts

# Wake will work with any context from this list
wake --context <context-name> --ui
```

### Setting Default Context
```bash
# Switch your default context
kubectl config use-context production-cluster

# Now Wake will use this context by default
wake --ui
```

## Advanced Context Management

### Multi-Cluster Operations
```bash
# Monitor logs from staging cluster
wake --context staging --namespace apps --ui

# Switch to production cluster in another terminal
wake --context production --namespace production --ui

# Compare logs across environments
wake --context staging -i "ERROR" --output staging-errors.log &
wake --context production -i "ERROR" --output prod-errors.log &
```

### Context-Specific Configuration
```bash
# Set different configurations for different contexts
wake setconfig --context staging script_outdir /tmp/staging-scripts
wake setconfig --context production script_outdir /secure/prod-scripts

# View context-specific configuration
wake getconfig --context staging
```

## Kubeconfig File Management

### Custom Kubeconfig Files
```bash
# Use a specific kubeconfig file
wake --kubeconfig ~/.kube/custom-config --ui

# Combine with context selection
wake --kubeconfig ~/.kube/staging-config --context staging-cluster --ui
```

### Multiple Kubeconfig Files
```bash
# Development environment
wake --kubeconfig ~/.kube/dev-config --ui

# Staging environment  
wake --kubeconfig ~/.kube/staging-config --ui

# Production environment (default kubeconfig)
wake --ui
```

## Namespace Management

### Cross-Namespace Monitoring
```bash
# Monitor all namespaces in current context
wake --all-namespaces --ui

# Monitor specific namespaces across contexts
wake --context staging --namespace "app-*" --ui
wake --context production --namespace "prod-*" --ui
```

### Namespace Patterns
```bash
# Use regex patterns for namespace selection
wake --context staging --namespace "test-.*" --ui

# Monitor multiple specific namespaces
wake --context production --namespace "frontend,backend,database" --ui
```

## Environment-Specific Workflows

### Development Workflow
```bash
# Quick development cluster access
export KUBECONFIG=~/.kube/dev-config
wake --ui

# Or specify directly
wake --kubeconfig ~/.kube/dev-config --namespace development --ui
```

### Staging Validation
```bash
# Staging environment testing
wake --context staging-cluster \
     --namespace staging \
     --template jfr \
     --template-args 1234 30s
```

### Production Monitoring
```bash
# Production monitoring with restricted access
wake --context production-cluster \
     --namespace production \
     --ui \
     --output-file prod-logs.txt
```

## Security and Best Practices

### Context Isolation
- **Separate Kubeconfig Files**: Use different kubeconfig files for different environments
- **Context Validation**: Always verify your current context before running commands
- **Namespace Restrictions**: Use specific namespaces rather than cluster-wide access when possible

### Configuration Management
```bash
# Verify context before operations
kubectl config current-context
wake --ui

# Use read-only contexts for production monitoring
wake --context prod-readonly --ui
```

### Access Control
```bash
# Limited namespace access
wake --context limited-access --namespace allowed-ns --ui

# Service account-based access
wake --kubeconfig ~/.kube/service-account-config --ui
```

## Troubleshooting Context Issues

### Common Problems

**Context Not Found**
```bash
# Error: context "staging" not found
# Solution: Check available contexts
kubectl config get-contexts
```

**Permission Denied**
```bash
# Error: pods is forbidden
# Solution: Verify RBAC permissions for the context
kubectl auth can-i get pods --context staging
```

**Kubeconfig Issues**
```bash
# Error: invalid configuration
# Solution: Validate kubeconfig file
kubectl config view --kubeconfig ~/.kube/custom-config
```

### Debugging Commands
```bash
# Test context connectivity
kubectl get pods --context staging

# Verify Wake can access the context
wake --context staging --list-containers

# Check namespace permissions
kubectl get namespaces --context production
```

## Integration Examples

### CI/CD Pipeline Integration
```bash
#!/bin/bash
# Deploy and monitor script

# Deploy to staging
kubectl apply -f deployment.yaml --context staging

# Monitor deployment logs
wake --context staging --namespace apps -i "deployment" --ui

# After validation, deploy to production
kubectl apply -f deployment.yaml --context production
wake --context production --namespace production -i "deployment" --ui
```

### Multi-Environment Monitoring
```bash
# Monitor script for multiple environments
#!/bin/bash

echo "Monitoring Development..."
wake --context dev --namespace apps -i "ERROR" --output dev-errors.log &

echo "Monitoring Staging..."
wake --context staging --namespace apps -i "ERROR" --output staging-errors.log &

echo "Monitoring Production..."
wake --context production --namespace apps -i "ERROR" --output prod-errors.log &

wait
```

## Configuration Tips

1. **Use Descriptive Context Names**: Name contexts clearly (e.g., `dev-cluster`, `staging-us-west`, `prod-eu-central`)
2. **Set Default Namespaces**: Configure default namespaces in your kubeconfig for each context
3. **Organize Kubeconfig Files**: Keep separate kubeconfig files for different environments
4. **Test Connectivity**: Always test context connectivity before important operations
5. **Document Access Patterns**: Maintain documentation of which contexts are used for what purposes

With Wake's Kubernetes context management, you can efficiently work across multiple clusters and environments while maintaining security and organization in your log analysis workflows.