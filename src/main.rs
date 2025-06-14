mod cli;
mod k8s;
mod logging;
mod output;
mod filtering;
mod ui; // Add the UI module declaration

use anyhow::Result;

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
    
    fmt()
        .with_max_level(log_level)
        .init();
    
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
