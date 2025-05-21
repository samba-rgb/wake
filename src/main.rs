mod cli;
mod k8s;
mod logging;
mod output;
mod filtering; // Add the missing module declaration

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    tracing_subscriber::fmt::init();
    
    // Set up signal handling for graceful termination
    logging::setup_signal_handler()?;
    
    // Parse command line arguments
    let args = cli::parse_args();
    
    // Run the main application logic
    cli::run(args).await
}
