mod cli;
mod k8s;
mod logging;
mod output;
mod filtering;
mod ui; // Add the UI module declaration
mod config; // Add the config module declaration

use anyhow::Result;
use tracing_appender;
use tracing_subscriber::layer::SubscriberExt; // Add missing import
use tracing_subscriber::util::SubscriberInitExt; // Add missing import
use tracing_subscriber::filter::LevelFilter; // Add missing import
use tracing_subscriber::Layer; // Add missing Layer trait import

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments first to determine if we're using UI mode
    let args = cli::parse_args();
    
    // Determine if UI will be used - UI should only be enabled when explicitly requested
    let should_use_ui = args.ui && !args.no_ui;
    
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
    // - In UI mode without --dev: suppress ALL logs to stdout/stderr (UI only)
    // - In UI mode with --dev: logs go to file only, not stdout/stderr
    use tracing_subscriber::fmt;
    use tracing::Level;
    
    let log_level = if should_use_ui && !args.dev {
        // In UI mode without dev flag, completely suppress logging to avoid UI interference
        Level::ERROR  // Will be redirected to null writer
    } else if should_use_ui && args.dev {
        // In UI mode with dev flag, use verbosity-based level but default to INFO if verbosity is 0
        if args.verbosity == 0 {
            Level::INFO  // Default to INFO level in dev mode
        } else {
            logging::get_log_level(args.verbosity)
        }
    } else if args.dev && args.verbosity == 0 {
        // In CLI mode with dev flag, default to INFO level if verbosity is 0
        Level::INFO
    } else {
        // In CLI mode, use verbosity-based level to stdout
        logging::get_log_level(args.verbosity)
    };
    
    // When dev mode is enabled, also log to a file
    if args.dev {
        // Use a single log file name instead of timestamped files
        let log_file_path = "wake_dev.log";
        
        if !should_use_ui {
            println!("🔍 Development mode enabled. Logs will be written to: {}", log_file_path);
        }
        
        // Create a file appender that writes to the dev log file
        let file_appender = tracing_appender::rolling::never("", log_file_path);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        if should_use_ui {
            // In UI mode with dev: logs go ONLY to file, not stdout/stderr
            fmt()
                .with_max_level(log_level)
                .with_ansi(false) // No colors in log file
                .with_writer(non_blocking) // Write to file only
                .init();
        } else {
            // In CLI mode with dev: logs go to both stdout and file
            tracing_subscriber::registry()
                .with(fmt::layer().with_filter(LevelFilter::from_level(log_level))) // stdout
                .with(fmt::layer().with_ansi(false).with_writer(non_blocking).with_filter(LevelFilter::from_level(log_level))) // file
                .init();
        }
    } else if should_use_ui {
        // UI mode without dev: use null writer to completely suppress logs
        use std::io::sink;
        use tracing_subscriber::filter::LevelFilter;
        fmt()
            .with_max_level(LevelFilter::OFF) // Completely disable logging
            .with_writer(sink) // Redirect to null
            .init();
    } else {
        // Normal CLI mode logging to stdout only
        fmt()
            .with_max_level(log_level)
            .init();
    }
    
    // Set up signal handling for graceful termination
    logging::setup_signal_handler()?;
    
    // Run the main application logic
    cli::run(args).await
}
