mod args;

pub use args::{Args, parse_args};
use anyhow::Result;

pub async fn run(args: Args) -> Result<()> {
    // Initialize kubernetes client
    let client = crate::k8s::create_client(&args).await?;
    
    // Set up log watcher
    let watcher = crate::k8s::LogWatcher::new(client, &args);
    
    // Stream the logs
    let log_streams = watcher.stream().await?;
    
    // Create output formatter
    let formatter = crate::output::Formatter::new(&args);
    
    // Process and display logs
    crate::logging::process_logs(log_streams, formatter).await?;
    
    Ok(())
}