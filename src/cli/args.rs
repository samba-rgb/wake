use clap::{Parser, Subcommand};
use regex::Regex;
use std::path::PathBuf;
use crate::filtering::FilterPattern; // Add import for FilterPattern

/// Helper function to get default namespace from current context
fn get_default_namespace() -> String {
    // Try to get namespace from current context
    if let Some(namespace) = crate::k8s::client::get_current_context_namespace() {
        return namespace;
    }
    
    // Fallback to "default" if we can't read the current context
    "default".to_string()
}

#[derive(Parser, Debug, Clone)]
#[command(
    author, 
    version, 
    about = "Advanced Kubernetes log tailing with intelligent filtering and interactive UI",
    long_about = "Wake is a powerful command-line tool for tailing logs from multiple Kubernetes pods and containers.\n\
\nFeatures:\n\
  | Feature                          | Description                                                                 |\n  |----------------------------------|-----------------------------------------------------------------------------|\n  | Regex patterns                   | \"ERROR|WARN\"                                                              |\n  | Logical operations               | \"info\" && \"user\" or \"debug\" || \"error\"                                |\n  | Negation                         | !\"timeout\"                                                                |\n  | Complex combinations             | \"(info || debug) && !\"noise\"\"                                          |\n  | Exact text matching              | \"exact phrase\"                                                            |\n\nBy default, Wake runs in CLI mode. Use --ui to enable interactive UI mode with real-time filter editing,\
file output support, autosave configuration, and development mode for debugging.\n\
---\n\
Examples:\n\
  wake setconfig autosave true                            # Enable autosave with auto-generated filenames\n\
  wake setconfig autosave true --path \"/path/to/logs\"    # Enable autosave with custom path\n\
  wake setconfig autosave false                           # Disable autosave\n\
  wake setconfig ui-buffer-expansion 10                   # Set UI buffer expansion to 10x in pause mode\n\
  wake setconfig ui-buffer-expansion 5                    # Set UI buffer expansion to 5x in pause mode\n\
  wake getconfig                                          # Show all current configuration\n\
  wake getconfig autosave                                 # Show only autosave configuration\n\
  wake getconfig ui-buffer-expansion                      # Show only buffer expansion setting",
    disable_help_flag = true
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Print help in a tabular format and exit
    #[arg(short = 'h', long = "help")]
    pub help: bool,

    /// Pod selector regular expression
    #[arg(default_value = ".*")]
    pub pod_selector: String,

    /// Container selector regular expression
    #[arg(short, long, default_value = ".*")]
    pub container: String,

    /// Limit processing to a random sample of matching pods (e.g., -s 1)
    #[arg(short = 's', long = "sample", value_parser = |v: &str| {
        v.parse::<usize>()
            .map_err(|_| String::from("Sample value must be a positive integer"))
            .and_then(|val| {
                if val >= 1 {
                    Ok(val)
                } else {
                    Err(String::from("Sample value must be at least 1"))
                }
            })
    }, help = "Randomly sample up to N matching pods (default: all)")]
    pub sample: Option<usize>,

    /// List all containers in matched pods (without streaming logs)
    #[arg(short = 'L', long)]
    pub list_containers: bool,
    
    /// Show logs from all containers in the pod (similar to kubectl logs --all-containers)
    #[arg(long = "all-containers")]
    pub all_containers: bool,

    /// Kubernetes namespace 
    #[arg(short, long, default_value_t = get_default_namespace())]
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
    ///   - Logical AND: "error && user"
    ///   - Logical OR: "debug || trace"
    ///   - Negation: "!timeout"
    ///   - Exact text: "\"exact phrase\""
    ///   - Complex: "(info || debug) && !noise"
    /// Note: Always use single quotes (' ') around patterns with logical operators.
    /// Examples:
    ///   - Correct: -i '"info" || "error"'
    ///   - Incorrect: -i "info" || "error"
    /// This filter INCLUDES logs that match the pattern
    #[arg(short = 'i', long = "include", help = "Include logs matching pattern (supports advanced syntax: &&, ||, !, quotes, regex), eg :  '\"info\" || \"error\"'")]
    pub include: Option<String>,    


    /// Exclude logs using advanced pattern syntax (supports &&, ||, !, quotes, regex)
    /// Same syntax as --include but for exclusion. This filter EXCLUDES logs that match the pattern
    /// Examples:
    ///   - Exclude debug: "debug"
    ///   - Exclude multiple: "debug || trace"
    ///   - Exclude errors from specific pod: "error && pod-name"
    #[arg(short = 'e', long = "exclude", help = "Exclude logs matching pattern (supports advanced syntax: &&, ||, !, quotes, regex) eg: '\"debug\" || \"trace\"'")]
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
    #[arg(short, long)]
    pub resource: Option<String>,

    /// Custom template for log output
    #[arg(long)]
    pub template: Option<String>,

    /// Execute a template with given arguments
    #[arg(long = "exec-template", value_name = "TEMPLATE", help = "Execute a predefined template (e.g., jfr, heap-dump, thread-dump)")]
    pub execute_template: Option<String>,

    /// Arguments to pass to the template
    #[arg(long = "template-args", value_name = "ARGS", help = "Arguments to pass to the template", num_args = 0..)]
    pub template_args: Vec<String>,

    /// List all available templates
    #[arg(long = "list-templates", help = "List all available predefined templates")]
    pub list_templates: bool,

    /// Output directory for template execution results
    #[arg(long = "template-output", value_name = "DIR", help = "Directory to save template execution results")]
    pub template_output: Option<PathBuf>,

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

    /// Buffer size for log storage (e.g., 10k, 20k, 30k). Higher values use more memory but allow longer history in selection mode
    #[arg(long, default_value = "20000", help = "Number of log entries to keep in memory (10k, 20k, 30k, etc.)")]
    pub buffer_size: usize,

    /// Verbosity level for debug output
    #[arg(short, long, default_value = "0")]
    pub verbosity: u8,

    /// Path to a script to run in each selected pod
    #[arg(long = "script-in", value_name = "PATH", help = "Path to a script to run in each selected pod (copied and executed as /tmp/wake-script.sh)")]
    pub script_in: Option<PathBuf>,

    /// Output directory for script results (overrides config)
    #[arg(long = "script-outdir", value_name = "DIR", help = "Directory to save script output tar (overrides config)")]
    pub script_outdir: Option<PathBuf>,

    /// Monitor CPU and memory usage with top-like display
    #[arg(short = 'm', long = "monitor", help = "Monitor CPU and memory usage with a top-like display")]
    pub monitor: bool,

    /// Select metrics source (api or kubectl)
    #[arg(long = "metrics-source", help = "Select metrics source: 'api' (metrics.k8s.io API) or 'kubectl' (kubectl top command)", value_parser = ["api", "kubectl"], default_value = "kubectl")]
    pub metrics_source: String,

    /// Hidden author flag (not shown in --help)
    #[arg(long, hide = true, default_value_t = false)]
    pub author: bool,

    /// Show command history or search commands with intelligent TF-IDF powered search
    /// Two modes:
    /// 1. History mode: --his (no arguments) - Shows your recent wake commands with timestamps
    /// 2. Search mode: --his "query" - Intelligent search for commands using keywords
    /// 
    /// Search Examples:
    ///   --his "config"        # Find configuration commands
    ///   --his "ui mode"       # Find UI-related commands  
    ///   --his "error logs"    # Find error logging commands
    ///   --his "namespace"     # Find namespace commands
    ///   --his "save logs"     # Find file output commands
    /// 
    /// Features:
    /// • Smart matching based on meaning, not just exact text
    /// • Built-in knowledge of hundreds of wake command patterns
    /// • Contextual suggestions when no exact matches found
    /// • Command history automatically saved (last 150 commands)
    #[arg(long = "his", value_name = "QUERY", help = "Show command history or search for commands with intelligent TF-IDF search (e.g., --his \"error logs\")", num_args = 0..=1, default_missing_value = "")]
    pub history: Option<String>,

    /// Enable web mode - send logs to HTTP endpoint instead of terminal
    #[arg(long, help = "Send filtered logs to web endpoint via HTTP")]
    pub web: bool,

    /// Show interactive guide or help content
    #[arg(long, help = "Display interactive guide and help content")]
    pub guide: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Configure Wake settings with interactive UI
    #[command(name = "setconfig")]
    SetConfig,
    /// Display current Wake configuration
    #[command(name = "getconfig")]
    GetConfig {
        /// Optional specific configuration key to display (e.g., autosave)
        key: Option<String>,
    },
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
    #[allow(dead_code)]
    pub fn include_regex(&self) -> Option<Result<Regex, regex::Error>> {
        self.include.as_ref().map(|p| Regex::new(p))
    }

    #[deprecated(note = "Use exclude_pattern() instead for advanced filtering support")]
    #[allow(dead_code)]
    pub fn exclude_regex(&self) -> Option<Result<Regex, regex::Error>> {
        self.exclude.as_ref().map(|p| Regex::new(p))
    }
}

// Implement Default for Args
impl Default for Args {
    fn default() -> Self {
        Self {
            command: None,
            help: false,
            pod_selector: ".*".to_string(),
            container: ".*".to_string(),
            sample: None, // default to no sampling
            list_containers: false,
            all_containers: false,
            namespace: get_default_namespace(), // Use helper function to get default namespace
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
            execute_template: None,
            template_args: Vec::new(),
            list_templates: false,
            template_output: None,
            since: None,
            threads: None,
            ui: false, // Default to false, will be determined by logic
            no_ui: false, // Default to false
            dev: false, // Default to false
            buffer_size: 20000, // Default buffer size
            verbosity: 0,
            script_in: None, // Default to None
            script_outdir: None, // Default to None
            author: false, // Default to false
            history: None, // Default to None
            monitor: false, // Default to false
            metrics_source: "kubectl".to_string(), // Default to kubectl
            web: false, // Default to false
            guide: false, // Default to false
        }
    }
}