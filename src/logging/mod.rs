use anyhow::Result;
use chrono::Local;
use futures::{Stream, StreamExt};
use std::io::{self, Write};
use std::path::Path;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info, Level};
use std::fs::OpenOptions;

use crate::k8s::logs::LogEntry;
use crate::output::{LogDecisionMaker};
use crate::cli::Args;
use crate::filtering::LogFilter;
use crate::config::Config;

// Export the wake_logger module
pub mod wake_logger;

/// Set up logging based on verbosity level
#[allow(dead_code)]
pub fn setup_logger(_verbosity: u8) -> Result<()> {
    // This function is kept for compatibility but logging is now handled in main.rs
    Ok(())
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

/// Main log processing pipeline: Kubernetes logs -> Buffer -> Filtering -> Buffer -> Decision Maker -> Output
pub async fn process_logs(
    mut log_stream: impl Stream<Item = LogEntry> + Unpin + Send + 'static,
    args: &Args,
    _formatter: crate::output::Formatter, // Keep for backward compatibility but not used
) -> Result<()> {
    info!("🚀 Starting log processing pipeline");
    info!("   Pipeline: K8s Logs → Buffer → Filtering → Buffer → Decision Maker → Output");
    
    // Create a channel for the raw logs (first buffer)
    let (raw_tx, raw_rx) = mpsc::channel(1024);
    
    // Set up the thread pool for filtering
    let num_threads = args.threads.unwrap_or_else(LogFilter::recommended_threads);
    
    // Extract advanced filter patterns from args
    let include_pattern = args.include_pattern().transpose()
        .map_err(|e| anyhow::anyhow!("Invalid include pattern: {}", e))?;
    let exclude_pattern = args.exclude_pattern().transpose()
        .map_err(|e| anyhow::anyhow!("Invalid exclude pattern: {}", e))?;
    
    // Create the log filter with advanced patterns
    let filter = LogFilter::new(include_pattern, exclude_pattern, num_threads);
    info!("📝 Created log filter with {} worker threads", num_threads);
    
    // Initialize the decision maker (factory pattern)
    let decision_maker = LogDecisionMaker::new(args).await?;
    info!("🎯 Decision maker initialized - output type: {}", decision_maker.get_output_type());
    
    // Task 1: Forward incoming logs to the first buffer
    tokio::spawn(async move {
        info!("📥 Starting K8s log stream ingestion");
        let mut log_count = 0;
        while let Some(entry) = log_stream.next().await {
            log_count += 1;
            if log_count % 100 == 0 {
                info!("📊 Ingested {} logs from K8s", log_count);
            }
            
            if raw_tx.send(entry).await.is_err() {
                break; // Channel closed, stop processing
            }
        }
        info!("📥 K8s log stream closed. Total logs ingested: {}", log_count);
    });
    
    // Start the filtering process (creates second buffer)
    let filtered_rx = filter.start_filtering2(raw_rx);
    info!("🔍 Log filtering pipeline started");

    // Task 2: Process filtered logs through decision maker
    process_with_decision_maker(filtered_rx, decision_maker).await
}

/// Process filtered logs using the decision maker
async fn process_with_decision_maker(
    mut filtered_rx: mpsc::Receiver<LogEntry>,
    mut decision_maker: LogDecisionMaker,
) -> Result<()> {
    info!("🎯 Starting decision maker processing");
    
    let mut processed_count = 0;
    
    while let Some(entry) = filtered_rx.recv().await {
        processed_count += 1;
        
        // Send log through decision maker
        if let Err(e) = decision_maker.process_log(entry).await {
            error!("Failed to process log through decision maker: {}", e);
            // Continue processing even if one log fails
        }
        
        // Log progress periodically
        if processed_count % 100 == 0 {
            info!("🎯 Decision maker processed {} logs", processed_count);
        }
    }
    
    // Flush any remaining data
    if let Err(e) = decision_maker.flush().await {
        error!("Failed to flush decision maker: {}", e);
    } else {
        info!("✅ Decision maker flushed successfully");
    }
    
    info!("🎯 Decision maker processing complete. Total processed: {}", processed_count);
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

/// Generate a timestamp-based filename in the format wake_log_timestamp(dd_mm_yyyy:hh:mm:ss)
fn generate_log_filename(directory: &str) -> String {
    let timestamp = Local::now().format("%d_%m_%Y:%H_%M_%S").to_string();
    format!("{directory}/wake_log_timestamp_{timestamp}.log")
}

/// Enhanced determine_autosave_path to ensure file creation without errors
pub fn determine_autosave_path(input: &str) -> Result<String> {
    let path = Path::new(input);
    info!("Received input path: {}", input);
    if path.is_file() {
        info!("Input is a file: {}", input);
        Ok(input.to_string())
    } else if path.is_dir() {
        let generated_path = generate_log_filename(input);
        info!("Input is a directory. Generated file path: {}", generated_path);
        // Ensure the file is created in the directory
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&generated_path)?;
        Ok(generated_path)
    } else {
        // If the input is neither a file nor a directory, create a default file in the current directory
        let default_path = generate_log_filename(".");
        info!("Input is invalid. Creating default file: {}", default_path);
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&default_path)?;
        Ok(default_path)
    }
}