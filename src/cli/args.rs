use clap::{Parser, Args as ClapArgs};
use regex::Regex;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Pod selector regular expression
    #[arg(default_value = ".*")]
    pub pod_selector: String,

    /// Container selector regular expression
    #[arg(short, long, default_value = ".*")]
    pub container: String,

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

    /// Filter logs by regex pattern
    #[arg(short, long)]
    pub include: Option<String>,

    /// Exclude logs by regex pattern
    #[arg(short = 'E', long)]
    pub exclude: Option<String>,

    /// Show timestamps in logs
    #[arg(short = 'T', long)]
    pub timestamps: bool,

    /// Output format (text, json, raw)
    #[arg(short, long, default_value = "text")]
    pub output: String,

    /// Use specific resource type filter (pod, deployment, statefulset)
    #[arg(short = 'r', long)]
    pub resource: Option<String>,

    /// Custom template for log output
    #[arg(long)]
    pub template: Option<String>,

    /// Since time (e.g., 5s, 2m, 3h)
    #[arg(long)]
    pub since: Option<String>,

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

    pub fn include_regex(&self) -> Option<Result<Regex, regex::Error>> {
        self.include.as_ref().map(|p| Regex::new(p))
    }

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
            resource: None,
            template: None,
            since: None,
            verbosity: 0,
        }
    }
}