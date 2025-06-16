use clap::Parser;
use regex::Regex;
use std::path::PathBuf;
use crate::filtering::FilterPattern; // Add import for FilterPattern

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Pod selector regular expression
    #[arg(default_value = ".*")]
    pub pod_selector: String,

    /// Container selector regular expression
    #[arg(short, long, default_value = ".*")]
    pub container: String,

    /// List all containers in matched pods (without streaming logs)
    #[arg(short = 'L', long)]
    pub list_containers: bool,
    
    /// Show logs from all containers in the pod (similar to kubectl logs --all-containers)
    #[arg(long = "all-containers")]
    pub all_containers: bool,

    /// Kubernetes namespace 
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Show logs from all namespaces
    #[arg(short = 'A', long)]
    pub all_namespaces: bool,

    /// Path to kubeconfig file
    #[arg(short, long)]
    pub kubeconfig: Option<PathBuf>,

    /// Kubernetes context to use
    #[arg(short = 'x', long)]
    pub context: Option<String>,

    /// Lines of logs to display from beginning
    #[arg(short, long, default_value = "10")]
    pub tail: i64,

    /// Follow logs (stream in real time)
    #[arg(short, long, default_value_t = true, action = clap::ArgAction::Set, num_args = 0..=1)]
    pub follow: bool,

    /// Filter logs using advanced pattern syntax (supports &&, ||, !, quotes, regex)
    /// Examples: 
    ///   - Simple regex: "ERROR|WARN"
    ///   - Logical AND: "\"info\" && \"user\""
    ///   - Logical OR: "\"debug\" || \"error\""
    ///   - Negation: "!\"timeout\""
    ///   - Complex: "(info || debug) && !\"noise\""
    #[arg(short, long)]
    pub include: Option<String>,

    /// Exclude logs using advanced pattern syntax (supports &&, ||, !, quotes, regex)
    /// Same syntax as --include but for exclusion
    #[arg(short = 'E', long)]
    pub exclude: Option<String>,

    /// Show timestamps in logs
    #[arg(short = 'T', long)]
    pub timestamps: bool,

    /// Output format (text, json, raw)
    #[arg(short, long, default_value = "text")]
    pub output: String,

    /// Output file path - when specified, logs are written to file instead of stdout
    /// Use with --ui to show both file output and UI
    #[arg(short = 'w', long = "output-file")]
    pub output_file: Option<PathBuf>,

    /// Use specific resource type filter (pod, deployment, statefulset)
    #[arg(short = 'r', long)]
    pub resource: Option<String>,

    /// Custom template for log output
    #[arg(long)]
    pub template: Option<String>,

    /// Since time (e.g., 5s, 2m, 3h)
    #[arg(long)]
    pub since: Option<String>,

    /// Number of threads to use for log filtering (default: 2x CPU cores)
    #[arg(long)]
    pub threads: Option<usize>,

    /// Enable interactive UI mode with dynamic filtering
    #[arg(long)]
    pub ui: bool,

    /// Disable interactive UI mode (force CLI output)
    #[arg(long)]
    pub no_ui: bool,

    /// Enable development mode - shows internal application logs even in UI mode
    #[arg(long)]
    pub dev: bool,

    /// Verbosity level for debug output
    #[arg(short, long, default_value = "0")]
    pub verbosity: u8,
}

pub fn parse_args() -> Args {
    Args::parse()
}

impl Args {
    pub fn pod_regex(&self) -> Result<Regex, regex::Error> {
        Regex::new(&self.pod_selector)
    }

    pub fn container_regex(&self) -> Result<Regex, regex::Error> {
        Regex::new(&self.container)
    }

    // Updated to use advanced FilterPattern instead of simple regex
    pub fn include_pattern(&self) -> Option<Result<FilterPattern, String>> {
        self.include.as_ref().map(|p| FilterPattern::parse(p))
    }

    pub fn exclude_pattern(&self) -> Option<Result<FilterPattern, String>> {
        self.exclude.as_ref().map(|p| FilterPattern::parse(p))
    }

    // Keep the old methods for backward compatibility, but mark as deprecated
    #[deprecated(note = "Use include_pattern() instead for advanced filtering support")]
    pub fn include_regex(&self) -> Option<Result<Regex, regex::Error>> {
        self.include.as_ref().map(|p| Regex::new(p))
    }

    #[deprecated(note = "Use exclude_pattern() instead for advanced filtering support")]
    pub fn exclude_regex(&self) -> Option<Result<Regex, regex::Error>> {
        self.exclude.as_ref().map(|p| Regex::new(p))
    }
}

// Implement Default for Args
impl Default for Args {
    fn default() -> Self {
        Self {
            pod_selector: ".*".to_string(),
            container: ".*".to_string(),
            namespace: "default".to_string(),
            all_namespaces: false,
            kubeconfig: None,
            context: None,
            tail: 10,
            follow: true,
            include: None,
            exclude: None,
            timestamps: false,
            output: "text".to_string(),
            output_file: None,
            resource: None,
            template: None,
            since: None,
            list_containers: false,
            verbosity: 0,
            all_containers: false,
            threads: None,
            ui: false, // Default to false, will be determined by logic
            no_ui: false, // Default to false
            dev: false, // Default to false
        }
    }
}