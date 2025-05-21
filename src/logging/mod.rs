use anyhow::Result;
use futures::{Stream, StreamExt};
use std::io::{self, Write};
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info, Level};

use crate::k8s::logs::LogEntry;
use crate::output::Formatter;
use crate::cli::Args;
use crate::filtering::LogFilter; // Fixed import path

/// Set up logging based on verbosity level
pub fn setup_logger(verbosity: u8) -> Result<()> {
    #[cfg(test)]
    {
        let _ = verbosity; // Use verbosity to avoid the unused variable warning
        return Ok(());
    }
    
    #[cfg(not(test))]
    {
        let log_level = get_log_level(verbosity);
        tracing_subscriber::fmt()
            .with_max_level(log_level)
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;
        Ok(())
    }
}

/// Get the appropriate log level based on verbosity
pub fn get_log_level(verbosity: u8) -> Level {
    match verbosity {
        0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        _ => Level::TRACE,
    }
}

/// Processes a stream of log entries using the provided formatter and threaded filtering
pub async fn process_logs(
    mut log_stream: impl Stream<Item = LogEntry> + Unpin + Send + 'static,
    args: &Args,
    formatter: Formatter,
) -> Result<()> {
    info!("Starting to process log stream with threaded filtering");
    
    // Create a channel for the raw logs
    let (raw_tx, raw_rx) = mpsc::channel(1024);
    
    // Set up the thread pool for filtering
    let num_threads = args.threads.unwrap_or_else(LogFilter::recommended_threads);
    
    // Extract regex patterns from args
    let include_pattern = args.include_regex().transpose()?;
    let exclude_pattern = args.exclude_regex().transpose()?;
    
    // Create the log filter
    let filter = LogFilter::new(include_pattern, exclude_pattern, num_threads);
    info!("Created log filter with {} worker threads", num_threads);
    
    // Set up a task to forward incoming logs to the raw channel
    tokio::spawn(async move {
        while let Some(entry) = log_stream.next().await {
            if raw_tx.send(entry).await.is_err() {
                break; // Channel closed, stop processing
            }
        }
        
        info!("Log stream from Kubernetes closed");
    });
    
    // Start the filtering process
    let mut filtered_rx = filter.start_filtering(raw_rx);
    
    // Process filtered logs
    let mut stdout = io::stdout();
    
    while let Some(entry) = filtered_rx.recv().await {
        // Format the log entry
        if let Some(formatted) = formatter.format_without_filtering(&entry) {
            // Write the formatted log entry to stdout
            if let Err(e) = writeln!(stdout, "{}", formatted) {
                error!("Failed to write to stdout: {:?}", e);
                if e.kind() == io::ErrorKind::BrokenPipe {
                    // This typically happens when the output is piped to another program
                    // that terminates (e.g., `wake logs | head`)
                    info!("Output pipe closed, stopping");
                    break;
                }
                return Err(anyhow::anyhow!("Failed to write to stdout: {:?}", e));
            }
        }
    }
    
    info!("Log stream processing complete");
    Ok(())
}

/// Handles signals like CTRL+C for graceful termination
pub fn setup_signal_handler() -> Result<()> {
    let (tx, mut rx) = mpsc::channel(1);
    
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        let _ = tx.send(()).await;
    });
    
    tokio::spawn(async move {
        let _ = rx.recv().await;
        std::process::exit(0);
    });
    
    Ok(())
}