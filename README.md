# Wake

Wake is a command-line tool for tailing multiple pods and containers in Kubernetes clusters, inspired by [stern](https://github.com/stern/stern).

## Features

- Multi-pod and container log tailing for Kubernetes
- Color-coded output for easier log differentiation
- Regular expression filtering for pods and containers
- Support for various Kubernetes resources (pods, deployments, statefulsets, etc.)
- Multiple output formats (text, json, raw)
- Timestamp support

## Next Steps

The following enhancements are planned for future versions:

1. **Resource Detection for Filtering**: Implement comprehensive detection and filtering for Kubernetes resource types:
   - Deployments, StatefulSets, DaemonSets, Jobs, and CronJobs
   - Label-based selection for all resource types
   - Support for resource field selectors

2. **Custom Output Templates**: Add support for user-defined output templates:
   - Go template-like syntax for customizing log output format
   - Template functions for JSON parsing, timestamp formatting
   - Template loading from external files

3. **Testing Framework**: Implement comprehensive tests:
   - Unit tests for core functionality
   - Integration tests with minikube/kind clusters
   - Mock Kubernetes API for testing without a real cluster

4. **Configuration File Support**: Add support for configuration files:
   - YAML configuration format
   - Config loading from standard paths (~/.config/wake/config.yaml)
   - Environment variable overrides

5. **Shell Completion Scripts**:
   - Bash, Zsh, and Fish completion scripts
   - Dynamic completion for namespaces and contexts

6. **Label Filtering Support**:
   - Filter pods by label selectors
   - Support for label expressions and operators
   - Autocomplete for commonly used labels

## Installation

### Building from source

```bash
cargo build --release
```

The binary will be available at `target/release/wake`

## Usage

```
# Tail logs from all pods in the default namespace
wake

# Tail logs from pods matching 'nginx' in the 'web' namespace
wake -n web nginx

# Tail logs from a specific deployment
wake -r deployment/my-app

# Show logs with timestamps
wake -T

# Output logs as JSON
wake -o json
```

### CLI Options

```
Options:
  -n, --namespace <NAMESPACE>  Kubernetes namespace [default: default]
  -A, --all-namespaces         Show logs from all namespaces
  -c, --container <CONTAINER>  Container selector regex [default: .*]
  -k, --kubeconfig <KUBECONFIG>  Path to kubeconfig file
  -x, --context <CONTEXT>      Kubernetes context to use
  -t, --tail <TAIL>            Lines of logs to display from beginning [default: 10]
  -f, --follow                 Follow logs (stream in real time) [default: true]
  -i, --include <INCLUDE>      Filter logs by regex pattern
  -E, --exclude <EXCLUDE>      Exclude logs by regex pattern
  -T, --timestamps             Show timestamps in logs
  -o, --output <OUTPUT>        Output format (text, json, raw) [default: text]
  -r, --resource <RESOURCE>    Use specific resource type filter (pod, deployment, statefulset)
      --template <TEMPLATE>    Custom template for log output
      --since <SINCE>          Since time (e.g., 5s, 2m, 3h)
  -v, --verbosity <VERBOSITY>  Verbosity level for debug output [default: 0]
  -h, --help                   Print help
  -V, --version                Print version
```

## License

MIT License