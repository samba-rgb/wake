# <img src="wakeicon.png" alt="Wake Icon" width="100" height="100" style="vertical-align: middle;"> <span style="font-size: 48px;">Wake</span>

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

### Prerequisites

- Rust toolchain (1.70.0 or later)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- Git

### Building from source

1. Clone the repository:
   ```bash
   git clone https://github.com/amba-rgb/wake.git
   cd wake
   ```

2. Build the project:
   ```bash
   # Development build
   cargo build

   # Release build with optimizations
   cargo build --release
   ```

   The binary will be available at:
   - Development build: `target/debug/wake`
   - Release build: `target/release/wake`

3. Run tests (optional):
   ```bash
   cargo test
   ```

### Installation

After building, you can install the binary to your system:

```bash
# Install to ~/.cargo/bin
cargo install --path .

# Or copy the release binary to a location in your PATH
cp target/release/wake ~/.local/bin/
```

### Development Setup

1. Configure your development environment:
   ```bash
   # Set up git hooks
   cargo install cargo-husky
   cargo husky install

   # Install development dependencies
   cargo install cargo-watch  # For auto-recompilation
   cargo install cargo-audit  # For security auditing
   ```

2. Development workflow:
   ```bash
   # Auto-rebuild on changes
   cargo watch -x build

   # Run tests in watch mode
   cargo watch -x test

   # Check code formatting
   cargo fmt --all -- --check

   # Run linter
   cargo clippy
   ```

For setting up a local Kubernetes test environment with sample applications, see the [Development Environment Guide](dev/README.md).

## Usage

```bash
# Tail logs from all pods in the default namespace
wake

# Tail logs from pods matching 'nginx' in the 'web' namespace
wake -n web nginx

# Tail logs from containers matching 'app' in pods matching 'frontend'
wake frontend -c app

# Show logs with timestamps
wake -T

# Output logs as JSON
wake -o json

# Use a regex pattern to filter logs
wake -n monitoring -i "error|warning"
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
  -o, --output <o>        Output format (text, json, raw) [default: text]
  -r, --resource <RESOURCE>    Use specific resource type filter (pod, deployment, statefulset)
      --template <TEMPLATE>    Custom template for log output
      --since <SINCE>          Since time (e.g., 5s, 2m, 3h)
  -v, --verbosity <VERBOSITY>  Verbosity level for debug output [default: 0]
  -h, --help                   Print help
  -V, --version                Print version
```

## Understanding Pod Selectors

The pod selector is a powerful feature that lets you filter which pods to watch. It accepts regular expressions (regex) for flexible matching:

### Pod Selector Examples

```bash
# Watch all pods that start with 'nginx'
wake nginx.*

# Watch specific numbered pods (e.g., web-1, web-2)
wake web-[0-9]+

# Watch multiple specific pods
wake '(auth|api|frontend).*'

# Watch all pods in a specific namespace that contain 'mysql'
wake -n database .*mysql.*

# Combine with container selection
wake nginx.* -c 'main|sidecar'
```

The pod selector is specified as a positional argument and uses standard regex syntax:
- `.*` matches any character sequence (e.g., `nginx.*` matches nginx-1234, nginx-abc, etc.)
- `[0-9]` matches any digit
- `+` means "one or more" of the previous pattern
- `|` means "or" (e.g., `(api|web)` matches either api or web)
- `^` matches start of name, `$` matches end of name

### Best Practices
- Start with broader patterns and narrow them down if needed
- Use the `-v` flag to see which pods are being matched
- Combine with `--container` flag to target specific containers within matched pods
- When using special regex characters, quote your pattern: `wake '(api|web).*'`

## Using Resource Type Selectors

In addition to using regular expressions to select pods, Wake supports selecting pods based on their parent Kubernetes resources. This feature helps you easily watch logs from specific Deployments, StatefulSets, or other resource types without knowing the exact pod names.

### Resource Selector Examples

```bash
# Watch logs from all pods owned by a specific deployment
wake -r deployment/nginx

# Watch logs from a statefulset
wake -r statefulset/mysql

# Watch logs from a daemonset
wake -r daemonset/monitoring-agent

# Watch logs from a specific job
wake -r job/backup-job

# Use shorthand names for resources
wake -r deploy/frontend
wake -r sts/database
wake -r ds/logging-agent
```

### Supported Resource Types

- **pod**: Direct pod selection by name
- **deployment** (alias: deploy): Select pods managed by a Deployment
- **replicaset** (alias: rs): Select pods managed by a ReplicaSet
- **statefulset** (alias: sts): Select pods managed by a StatefulSet
- **daemonset** (alias: ds): Select pods managed by a DaemonSet
- **job**: Select pods owned by a Job

### Combining With Other Selectors

Resource selectors can be combined with container selectors for more precise targeting:

```bash
# Watch the 'api' container in any pod from the 'frontend' deployment
wake -r deployment/frontend -c api

# Watch all containers in pods from the 'database' statefulset
wake -r sts/database --all-containers
```

## License

MIT License