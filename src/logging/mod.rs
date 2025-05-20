use anyhow::Result;
use futures::{Stream, StreamExt};
use std::io::{self, Write};
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info, Level};

use crate::k8s::logs::LogEntry;
use crate::output::Formatter;

/// Set up logging based on verbosity level
pub fn setup_logger(verbosity: u8) -> Result<()> {
    let level = get_log_level(verbosity);
    
    #[cfg(test)]
    return Ok(()); // In test environment, don't try to set up global subscriber
    
    #[cfg(not(test))]
    {
        tracing_subscriber::fmt()
            .with_max_level(level)
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

/// Processes a stream of log entries using the provided formatter
pub async fn process_logs(
    mut log_stream: impl Stream<Item = LogEntry> + Unpin,
    formatter: Formatter,
) -> Result<()> {
    info!("Starting to process log stream");
    
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    
    // Process each log entry as it arrives
    while let Some(entry) = log_stream.next().await {
        // Format the entry according to the formatter's rules
        if let Some(formatted) = formatter.format(&entry) {
            // Write to stdout
            if let Err(e) = writeln!(handle, "{}", formatted) {
                error!("Failed to write to stdout: {:?}", e);
                // If we can't write to stdout, there's not much point in continuing
                if e.kind() == io::ErrorKind::BrokenPipe {
                    info!("Output pipe closed, stopping");
                    break;
                }
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