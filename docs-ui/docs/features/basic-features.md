---
sidebar_position: 1
---

# Basic Features

Before exploring Wake's advanced features like the interactive UI and web view, it's important to understand the fundamental concepts that make Wake powerful: **pod selection**, **sampling**, and **basic usage patterns**.

## Pod Selection

Wake uses flexible pod selection mechanisms to help you target exactly the pods you want to monitor.

### Pod Selector Patterns

The pod selector is the first argument to Wake and supports powerful regex patterns. **Quotes are optional for simple selectors**:

```bash
# Match all pods (default)
wake

# Match pods with specific names - quotes optional
wake my-app
wake "my-app"        # equivalent

# Match with simple patterns - quotes optional  
wake api
wake "api"           # equivalent

# Complex patterns with regex - quotes recommended for clarity
wake "api-.*"                    # All pods starting with "api-"
wake ".*-worker"                 # All pods ending with "-worker"
wake "(frontend|backend)"        # Pods containing "frontend" OR "backend"

# When quotes are needed
wake "api-v[0-9]+"              # Regex with special characters
wake "my app"                   # Names with spaces (rare in K8s)
```

**When to use quotes:**
- **Optional**: Simple names without spaces or special characters (`wake api`)
- **Recommended**: Complex regex patterns (`wake "api-.*"`)
- **Required**: Patterns with shell-interpreted characters (`wake "app && log"`)

### Namespace Selection

Control which namespace(s) to search for pods:

```bash
# Current namespace (default)
wake "my-app"

# Specific namespace
wake -n production "my-app"

# All namespaces
wake -A "my-app"

# Multiple namespaces with context switching
kubectx production && wake "api-.*"
```

### Container Selection

When pods have multiple containers, you can target specific ones:

```bash
# All containers in matching pods (default)
wake "my-app"

# Specific container only
wake "my-app" -c "api-server"

# Multiple containers with regex
wake "my-app" -c "(api|worker)"

# List all containers in matching pods
wake "my-app" -L
```

### Resource-Based Selection

Select pods by their owning resources (deployments, statefulsets, etc.):

```bash
# Pods owned by a deployment
wake -r deploy/my-api

# Pods owned by a statefulset
wake -r sts/database

# Pods owned by a daemonset
wake -r ds/log-collector
```

## Sampling

When dealing with large deployments, you might not want logs from all pods. Wake's sampling feature lets you work with manageable subsets.

### Sample Size Selection

```bash
# Get logs from all matching pods (default)
wake "worker-.*"

# Sample only 3 random pods from matches
wake "worker-.*" -s 3

# Sample 1 random pod for quick testing
wake "api-.*" -s 1

# Sample 10 pods from all namespaces
wake -A "my-app" -s 10
```

### Why Use Sampling?

- **Performance**: Reduce log volume in large deployments
- **Testing**: Quick checks without overwhelming output
- **Debugging**: Focus on a representative subset
- **Resource efficiency**: Lower CPU and memory usage

### Sampling Strategies

```bash
# Quick health check - sample 1 pod
wake "health-checker" -s 1

# Representative sample - 20% of pods
wake "worker-.*" -s 5  # if you have ~25 worker pods

# Gradual scaling - start small, then expand
wake "api-.*" -s 1     # Test with 1 pod
wake "api-.*" -s 5     # Expand to 5 pods
wake "api-.*"          # Full deployment when needed
```

## Basic Usage Patterns

### Quick Log Viewing

```bash
# View recent logs from all pods
wake

# View logs from specific app
wake "my-app"

# View logs with timestamps
wake "my-app" --timestamps

# Show more initial log lines
wake "my-app" -t 50
```

### Filtering Logs

```bash
# Show only error logs
wake "my-app" -i "error"

# Show errors and warnings
wake "my-app" -i "error|warn"

# Exclude debug logs
wake "my-app" -e "debug"

# Complex filtering
wake "my-app" -i "error" -e "test"
```

### Output Formats

```bash
# Default text format
wake "my-app"

# JSON format for structured processing
wake "my-app" -o json

# Raw format (no Wake formatting)
wake "my-app" -o raw

# Save to file while watching
wake "my-app" -w /tmp/logs.txt
```

### Time-Based Filtering

```bash
# Logs from last 5 minutes
wake "my-app" --since 5m

# Logs from last hour
wake "my-app" --since 1h

# Logs from last day
wake "my-app" --since 24h

# Show last 100 lines and follow
wake "my-app" -t 100 -f
```

## Practical Examples

### Development Workflow

```bash
# 1. Quick health check of your app
wake "my-app" -s 1 -t 10

# 2. Look for errors in staging
wake -n staging "my-app" -i "error|exception"

# 3. Monitor deployment progress
wake "my-app" -i "started|ready|running"

# 4. Debug specific container
wake "my-app" -c "api-server" -i "error"
```

### Production Debugging

```bash
# 1. Sample a few pods to understand the issue
wake -n production "api-.*" -s 3 -i "error"

# 2. Focus on specific timeframe
wake -n production "api-.*" --since 30m -i "5xx|error"

# 3. Save logs for later analysis
wake -n production "api-.*" -i "error" -w /tmp/prod-errors.log

# 4. Full investigation across all pods
wake -n production "api-.*" -i "error"
```

### Multi-Environment Monitoring

```bash
# Development environment
kubectx dev && wake "my-app" -s 1

# Staging validation
kubectx staging && wake "my-app" -i "error|warn"

# Production monitoring
kubectx production && wake "my-app" -s 5 -i "error"
```

## Performance Considerations

### Efficient Pod Selection

```bash
# Good: Specific patterns
wake "api-server-.*"

# Better: With namespace
wake -n production "api-server-.*"

# Best: With sampling for large deployments
wake -n production "api-server-.*" -s 5
```

### Memory Management

```bash
# Default buffer (good for most cases)
wake "my-app"

# Larger buffer for longer sessions
wake "my-app" --buffer-size 50000

# Smaller buffer for resource-constrained environments
wake "my-app" --buffer-size 5000
```

### Network Efficiency

```bash
# Reduce log volume with filtering
wake "my-app" -i "important|error|warn" -e "debug"

# Limit initial log retrieval
wake "my-app" -t 20 --since 10m
```

## Common Patterns Reference

| Use Case | Command | Description |
|----------|---------|-------------|
| Quick check | `wake "app" -s 1` | One pod, recent logs |
| Error hunting | `wake "app" -i "error\|exception"` | Only error messages |
| Deployment monitor | `wake "app" -i "started\|ready"` | Deployment progress |
| Debug session | `wake "app" -c "container" -t 100` | Specific container, more history |
| Production sample | `wake -n prod "app" -s 5 -i "error"` | Representative error sample |
| Save for analysis | `wake "app" -i "error" -w errors.log` | Error logs to file |

## Next Steps

Once you're comfortable with these basic concepts, you can explore Wake's advanced features:

- **[Interactive UI](./interactive-ui)** - Real-time filtering and navigation
- **[Advanced Patterns](./advanced-patterns)** - Complex filtering with logical operators  
- **[Web View](./web-view)** - Browser-based log viewing and sharing
- **[Template System](./template-system)** - Automated diagnostics and profiling

Understanding pod selection, sampling, and basic usage patterns will make these advanced features much more powerful and intuitive to use.