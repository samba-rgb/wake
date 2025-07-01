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
Features advanced pattern syntax with logical operators (&&, ||, !), interactive UI mode with dynamic filtering,\
file output support, autosave configuration, and development mode for debugging. Supports advanced filtering patterns like:\n\
  • Regex patterns: \"ERROR|WARN\"\n\
  • Logical operations: '\"info\" && \"user\"' or '\"debug\" || \"error\"'\n\
  • Negation: '!\"timeout\"'\n\
  • Complex combinations: '(info || debug) && !\"noise\"'\n\
  • Exact text matching: '\"exact phrase\"'\n\
By default, Wake runs in CLI mode. Use --ui to enable interactive UI mode with real-time filter editing,\
or --dev for detailed debugging information.\n\
---\n\
NEW: Run scripts in pods and collect output!\n\
  --script-in <path>         Run a script in all selected pods and collect output.\n\
  --script-outdir <dir>      Directory to save script output (default: timestamped dir in current location).\n\
  Output for each pod is saved as <namespace>_<pod>.stdout.txt and <namespace>_<pod>.stderr.txt.\n\
  You can set a default output directory with:\n\
    wake setconfig script_outdir /path/to/dir\n\
---\n\
Configuration Examples:\n\
  wake setconfig autosave true                            # Enable autosave with auto-generated filenames\n\
  wake setconfig autosave true --path \"/path/to/logs\"    # Enable autosave with custom path\n\
  wake setconfig autosave false                           # Disable autosave\n\
  wake setconfig ui-buffer-expansion 10                   # Set UI buffer expansion to 10x in pause mode\n\
  wake setconfig ui-buffer-expansion 5                    # Set UI buffer expansion to 5x in pause mode\n\
  wake getconfig                                          # Show all current configuration\n\
  wake getconfig autosave                                 # Show only autosave configuration\n\
  wake getconfig ui-buffer-expansion                      # Show only buffer expansion setting"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

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
    #[arg(short = 'i', long = "include", value_name = "PATTERN", help = "Include logs matching pattern (supports advanced syntax: &&, ||, !, quotes, regex), e.g.: \"info\" || \"error\"")]
    pub include: Option<String>,    #[arg(short = 'i', long = "include", help = "Include logs matching pattern (supports advanced syntax: &&, ||, !, quotes, regex), eg :  '\"info\" || \"error\"'")]


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

    /// Hidden author flag (not shown in --help)
    #[arg(long, hide = true, default_value_t = false)]
    pub author: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Configure Wake settings
    #[command(name = "setconfig")]
    SetConfig {
        /// Configuration key to set
        key: String,
        /// Configuration value to set
        value: String,
        /// Optional configuration parameter (e.g., path for autosave)
        #[arg(short, long)]
        path: Option<String>,
    },
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
            command: None,  // Add the missing command field
            pod_selector: ".*".to_string(),
            container: ".*".to_string(),
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
            since: None,
            list_containers: false,
            verbosity: 0,
            all_containers: false,
            threads: None,
            ui: false, // Default to false, will be determined by logic
            no_ui: false, // Default to false
            dev: false, // Default to false
            buffer_size: 20000, // Default buffer size
            script_in: None, // Default to None
            script_outdir: None, // Default to None
            author: false, // Default to false
        }
    }
}