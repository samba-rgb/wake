# Wake - Future Development Steps

This document outlines planned improvements and feature additions for the Wake project, a Kubernetes log tailing tool inspired by stern.

## 1. Resource Detection for Deployment/StatefulSet/Job Filtering

### Description
Implement comprehensive detection and filtering for Kubernetes resource types beyond direct pod selection.

### Implementation Plan
- Create a `ResourceFilter` struct in a new file `k8s/resource_filter.rs`
- Support all standard Kubernetes workload resources:
  - Deployments
  - StatefulSets
  - DaemonSets
  - Jobs
  - CronJobs
- Track pod ownership through owner references
- Connect to our existing `ResourceType` enum

### Code Example
```rust
// k8s/resource_filter.rs
pub struct ResourceFilter {
    resource_type: Option<ResourceType>,
    name_pattern: Option<String>,
    label_selector: Option<String>,
    field_selector: Option<String>,
}

impl ResourceFilter {
    pub async fn get_matching_pods(&self, client: &Client, namespace: &str) -> Result<Vec<PodInfo>> {
        match &self.resource_type {
            Some(ResourceType::Deployment) => {
                // Find pods owned by a deployment
                self.get_deployment_pods(client, namespace).await
            },
            Some(ResourceType::StatefulSet) => {
                // Find pods owned by a statefulset
                self.get_statefulset_pods(client, namespace).await
            },
            // Other resource types...
            _ => {
                // Default to direct pod selection
                self.get_pods_by_name(client, namespace).await
            }
        }
    }
    
    // Implementation for specific resource types...
}
```

### Directory Structure Changes
```
src/
  k8s/
    resource_filter.rs  (new)
```

## 2. Custom Output Templates

### Description
Add support for customizable output formats using templates, similar to stern's template feature.

### Implementation Plan
- Create a template parsing and rendering system in `output/template.rs`
- Support basic variable substitution (e.g., `{{.PodName}}`)
- Support functions (e.g., `{{formatTime .Timestamp "RFC3339"}}`)
- Support conditional formatting
- Load templates from string or file

### Code Example
```rust
// output/template.rs
pub struct Template {
    raw: String,
    tokens: Vec<Token>,
}

enum Token {
    Text(String),
    Variable(String),
    Function { name: String, args: Vec<String> },
    Conditional { condition: String, then_branch: Vec<Token>, else_branch: Option<Vec<Token>> },
}

impl Template {
    pub fn parse(template_str: &str) -> Result<Self> {
        // Parse template into tokens
    }
    
    pub fn render(&self, entry: &LogEntry) -> Result<String> {
        // Render template with log entry data
    }
}

// Add to output/mod.rs
impl Formatter {
    // Add template support to the existing formatter
}
```

### Template Functions
- `formatTime`: Format timestamps in various formats (RFC3339, RFC822, etc.)
- `toJson`: Convert structured data to JSON format
- `toPrettyJson`: Convert to formatted JSON with indentation
- `colorize`: Apply color formatting based on severity/source
- `truncate`: Limit string length with ellipsis
- `match`: Apply regex matching and extraction
- `default`: Provide default values for missing fields

### Template Variables
Available variables in templates:
- `{{.Namespace}}`: Pod namespace
- `{{.PodName}}`: Name of the pod
- `{{.ContainerName}}`: Container name
- `{{.Message}}`: Log message content
- `{{.Timestamp}}`: Log timestamp
- `{{.Labels}}`: Pod labels map
- `{{.Annotations}}`: Pod annotations
- `{{.NodeName}}`: Node where pod is running
- `{{.ResourceVersion}}`: Pod resource version

### Directory Structure Changes
```
src/
  output/
    template.rs  (new)
```

## 3. Tests to Verify Functionality

### Description
Add comprehensive unit and integration tests to ensure code quality and prevent regressions.

### Implementation Plan
- Set up a testing framework with both unit and integration tests
- Create mock Kubernetes API for testing without a real cluster
- Use fixtures for test data
- Test core functionality:
  - CLI argument parsing
  - Log formatting
  - Template rendering
  - Kubernetes API interaction

### Code Example
```rust
// tests/unit/args_test.rs
#[cfg(test)]
mod tests {
    use crate::cli::args::{Args, parse_args};
    
    #[test]
    fn test_default_arguments() {
        let args = Args::parse_from(&["wake"]);
        assert_eq!(args.namespace, "default");
        assert_eq!(args.container, ".*");
        assert_eq!(args.pod_selector, ".*");
        assert_eq!(args.tail, 10);
        assert!(args.follow);
    }
    
    #[test]
    fn test_namespace_argument() {
        let args = Args::parse_from(&["wake", "-n", "kube-system"]);
        assert_eq!(args.namespace, "kube-system");
    }
    
    // More tests...
}
```

### Test Categories
1. Unit Tests
   - CLI argument parsing and validation
   - Template parsing and rendering
   - Resource type detection
   - Label selector parsing
   - Configuration file loading
   - Log entry formatting

2. Integration Tests
   - Kubernetes API interaction
   - Log streaming functionality
   - Resource filtering
   - Multi-container log handling
   - Error handling and recovery
   - Performance with large log volumes

3. End-to-End Tests
   - Full workflow scenarios
   - Configuration file usage
   - Template customization
   - Label-based filtering
   - Resource type filtering
   - Shell completion functionality

4. Performance Tests
   - Log streaming throughput
   - Memory usage monitoring
   - CPU utilization tracking
   - Connection handling
   - Large cluster scalability

### Directory Structure Changes
```
tests/
  unit/
    args_test.rs
    formatter_test.rs
    template_test.rs
  integration/
    k8s_test.rs
  fixtures/
    pod_list.json
    pod_logs.txt
  mocks/
    k8s_client.rs
```

## 4. Configuration File Support

### Description
Implement support for configuration files to store default settings and preferences.

### Implementation Plan
- Create a configuration module in `config/mod.rs`
- Support YAML configuration format
- Look for config files in standard locations:
  - `./wake.yaml`
  - `~/.config/wake/config.yaml`
  - `~/.wake.yaml`
- Allow environment variables to override config
- Merge configuration with command line arguments

### Code Example
```rust
// config/mod.rs
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub default_namespace: Option<String>,
    pub kubeconfig_path: Option<PathBuf>,
    pub context: Option<String>,
    pub default_tail_lines: Option<i64>,
    pub templates: HashMap<String, String>,
    pub colors: Option<bool>,
    // Other options...
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = serde_yaml::from_str(&contents)?;
        Ok(config)
    }
    
    pub fn locate_config_file() -> Option<PathBuf> {
        // Look in standard locations
    }
    
    pub fn merge_with_args(&self, args: &mut Args) {
        // Apply config values to args if not explicitly set on command line
    }
}
```

### Configuration Options
Available configuration options:
```yaml
# Wake Configuration File

# Kubernetes Settings
kubernetes:
  default_namespace: "default"
  kubeconfig_path: "~/.kube/config"
  context: "minikube"
  timeout: 30s
  retry_interval: 5s

# Log Display Settings
display:
  colors: true
  timestamps: false
  output_format: "text"
  max_line_length: 1000
  template: "{{.Timestamp}} {{.Namespace}}/{{.PodName}}/{{.ContainerName}} {{.Message}}"

# Resource Selection
selection:
  default_tail: 10
  follow: true
  exclude_patterns:
    - "health|readiness|liveness"
    - "metrics-scraper"
  include_patterns:
    - "error|warning|fatal"

# Label Settings
labels:
  default_selectors:
    - "app=myapp"
    - "environment=production"
  exclude_labels:
    - "monitoring=disabled"

# Templates
templates:
  compact: "{{.PodName}}/{{.ContainerName}}: {{.Message}}"
  detailed: |
    Pod: {{.PodName}}
    Container: {{.ContainerName}}
    Message: {{.Message}}
    Time: {{formatTime .Timestamp "RFC3339"}}
  json: |
    {
      "pod": "{{.PodName}}",
      "container": "{{.ContainerName}}",
      "message": {{toJson .Message}}
    }
```

### Directory Structure Changes
```
src/
  config/
    mod.rs  (new)
```

## 5. Shell Completion Scripts

### Description
Add support for shell completion scripts to make the CLI more user-friendly.

### Implementation Plan
- Use clap's built-in completion script generation
- Generate completions for:
  - Bash
  - Zsh
  - Fish
- Support dynamic completion for:
  - Namespaces
  - Contexts
  - Resource types

### Code Example
```rust
// src/cli/completions.rs
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

pub fn generate_completion(shell: Shell) -> Result<(), io::Error> {
    let mut app = crate::cli::args::Args::command();
    let app_name = app.get_name().to_string();
    generate(shell, &mut app, app_name, &mut io::stdout());
    Ok(())
}
```

### Dynamic Completion Features
1. Resource Types
   - Complete resource type names (pod, deployment, etc.)
   - Show aliases (deploy, sts, ds)
   - Filter based on cluster capabilities

2. Namespace Completion
   - List available namespaces
   - Show namespace status
   - Cache results for performance

3. Context Completion
   - List available contexts
   - Show current context
   - Include cluster info

4. Label Completion
   - Complete known label keys
   - Suggest common values
   - Support multiple label selectors

### Installation
```bash
# For Bash
wake completion bash > /etc/bash_completion.d/wake

# For Zsh
wake completion zsh > ~/.zsh/completion/_wake

# For Fish
wake completion fish > ~/.config/fish/completions/wake.fish
```

### Directory Structure Changes
```
src/
  cli/
    completions.rs  (new)
```

## 6. Filtering by Labels

### Description
Implement support for filtering pods by Kubernetes labels, a powerful selection mechanism.

### Implementation Plan
- Add label selector options to CLI arguments
- Implement label selection logic in pod filtering
- Support standard Kubernetes label selector syntax:
  - Equality: `key=value`
  - Set-based: `key in (value1, value2)`
  - Existence: `key`
  - Non-existence: `!key`

### Code Example
```rust
// Update cli/args.rs
pub struct Args {
    // Existing fields...
    
    /// Label selector to filter pods (e.g., app=nginx,tier=frontend)
    #[arg(short = 'l', long)]
    pub label_selector: Option<String>,
}

// Add to k8s/pod.rs
pub async fn select_pods_by_label(
    client: &Client,
    namespace: &str,
    label_selector: &str,
) -> Result<Vec<PodInfo>> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    
    let params = ListParams {
        label_selector: Some(label_selector.to_string()),
        ..Default::default()
    };
    
    // List pods with the label selector
    let pod_list = pods.list(&params).await?;
    
    // Process pods to PodInfo
    // ...
}
```

### Label Selector Syntax
1. Equality-based
   - `environment=production`
   - `tier!=frontend`
   - Multiple: `app=nginx,environment=prod`

2. Set-based
   - `environment in (prod,staging)`
   - `tier notin (frontend,backend)`
   - `!version`

3. Complex Selectors
   - `app=nginx,tier in (frontend,backend),!beta`
   - `environment=production,!canary,version notin (v1,v2)`

### Implementation Details
1. Label Parser
   - Tokenize label expressions
   - Parse set operations
   - Validate syntax
   - Convert to Kubernetes API format

2. Label Matcher
   - Implement matching logic
   - Handle multiple selectors
   - Support set operations
   - Cache common patterns

3. Performance Optimizations
   - Index label lookups
   - Cache selector parsing
   - Batch API requests
   - Optimize regex compilation