mod cli;
mod k8s;
mod logging;
mod output;
mod filtering;
mod ui; // Add the UI module declaration

use anyhow::Result;
use tracing_appender;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments first to determine if we're using UI mode
    let args = cli::parse_args();
    
    // Determine if UI will be used (same logic as in cli::run)
    let should_use_ui = if args.output_file.is_some() {
        args.ui
    } else {
        !is_default_run(&args) && !args.list_containers
    };
    
    // Validate include and exclude patterns before proceeding
    if let Some(ref include_pattern) = args.include {
        if let Err(e) = filtering::FilterPattern::parse(include_pattern) {
            eprintln!("❌ Invalid include pattern: '{}'", include_pattern);
            eprintln!("   Error: {}", e);
            eprintln!("\n💡 Pattern syntax help:");
            eprintln!("   • Regex patterns: \"ERROR|WARN\"");
            eprintln!("   • Logical AND: '\"info\" && \"user\"'");
            eprintln!("   • Logical OR: '\"debug\" || \"error\"'");
            eprintln!("   • Negation: '!\"timeout\"'");
            eprintln!("   • Complex: '(info || debug) && !\"noise\"'");
            eprintln!("   • Exact text: '\"exact phrase\"'");
            std::process::exit(1);
        }
    }
    
    if let Some(ref exclude_pattern) = args.exclude {
        if let Err(e) = filtering::FilterPattern::parse(exclude_pattern) {
            eprintln!("❌ Invalid exclude pattern: '{}'", exclude_pattern);
            eprintln!("   Error: {}", e);
            eprintln!("\n💡 Pattern syntax help:");
            eprintln!("   • Regex patterns: \"ERROR|WARN\"");
            eprintln!("   • Logical AND: '\"info\" && \"user\"'");
            eprintln!("   • Logical OR: '\"debug\" || \"error\"'");
            eprintln!("   • Negation: '!\"timeout\"'");
            eprintln!("   • Complex: '(info || debug) && !\"noise\"'");
            eprintln!("   • Exact text: '\"exact phrase\"'");
            std::process::exit(1);
        }
    }
    
    // Always initialize tracing, but configure log level based on UI mode and dev flag
    // - In CLI mode: show logs based on verbosity
    // - In UI mode without --dev: suppress most logs (ERROR only)
    // - In UI mode with --dev: show logs based on verbosity
    use tracing_subscriber::fmt;
    use tracing::Level;
    
    let log_level = if should_use_ui && !args.dev {
        // In UI mode without dev flag, only show errors to avoid interfering with UI
        Level::ERROR
    } else {
        // In CLI mode or UI mode with dev flag, use verbosity-based level
        logging::get_log_level(args.verbosity)
    };
    
    // When dev mode is enabled, also log to a file
    if args.dev {
        // Create a timestamp for the log file name
        use chrono::Local;
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let log_file_path = format!("wake_dev_{}.log", timestamp);
        
        println!("🔍 Development mode enabled. Logs will be written to: {}", log_file_path);
        
        // Create a file appender that writes to the dev log file
        let file_appender = tracing_appender::rolling::never("", &log_file_path);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        // Set up logging to file for dev mode
        fmt()
            .with_max_level(log_level)
            .with_ansi(false) // No colors in log file
            .with_writer(non_blocking) // Write to file
            .init();
    } else {
        // Normal logging to stdout only
        fmt()
            .with_max_level(log_level)
            .init();
    }
    
    // Set up signal handling for graceful termination
    logging::setup_signal_handler()?;
    
    // Run the main application logic
    cli::run(args).await
}

/// Helper function to check if using default run (copied from cli module)
fn is_default_run(args: &cli::Args) -> bool {
    args.pod_selector == ".*" &&
    args.container == ".*" &&
    args.namespace == "default" &&
    !args.all_namespaces &&
    args.resource.is_none() &&
    !args.list_containers
}
