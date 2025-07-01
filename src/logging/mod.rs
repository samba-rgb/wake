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
use crate::output::Formatter;
use crate::cli::Args;
use crate::filtering::LogFilter;
use crate::config::Config;

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

/// Processes a stream of log entries using the provided formatter and threaded filtering
pub async fn process_logs(
    mut log_stream: impl Stream<Item = LogEntry> + Unpin + Send + 'static,
    args: &Args,
    formatter: Formatter,
) -> Result<()> {
    info!("Starting to process log stream with threaded filtering");
    
    // Load configuration for autosave functionality
    let config = Config::load().unwrap_or_default();
    
    // Create a channel for the raw logs
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
    info!("Created log filter with {} worker threads", num_threads);
    
    // Determine output strategy based on autosave config and CLI args
    let (primary_writer, autosave_writer) = setup_output_writers(args, &config)?;
    
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
    let filtered_rx = filter.start_filtering(raw_rx);

    // Process filtered logs
    process_filtered_logs(filtered_rx, formatter, primary_writer, autosave_writer).await
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
    format!("{}/wake_log_timestamp_{}.log", directory, timestamp)
}

/// Enhanced determine_autosave_path to ensure file creation without errors
fn determine_autosave_path(input: &str) -> Result<String> {
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

/// Set up output writers based on autosave config and CLI args
fn setup_output_writers(args: &Args, config: &Config) -> Result<(Box<dyn Write + Send>, Option<Box<dyn Write + Send>>)> {
    // Determine autosave file path using the priority logic:
    // 1. If -w flag is provided, that takes precedence
    // 2. If autosave is enabled and no -w flag, use configured path or auto-generated filename
    
    let autosave_writer: Option<Box<dyn Write + Send>> = if config.autosave.enabled {
        let autosave_path = if let Some(ref _output_file) = args.output_file {
            // If -w flag is provided, logs go to that file (primary), no separate autosave needed
            None
        } else {
            // No -w flag, use autosave configuration
            if let Some(ref configured_path) = config.autosave.path {
                let resolved_path = if Path::new(configured_path).is_dir() {
                    determine_autosave_path(configured_path)?
                } else {
                    configured_path.clone()
                };
                Some(resolved_path)
            } else {
                // Use determine_autosave_path to handle file or directory input
                // Use the current working directory as the base path
                let autosave_path = determine_autosave_path(std::env::current_dir()?.to_str().unwrap())?;
                Some(autosave_path)
            }
        };

        if let Some(path) = autosave_path {
            info!("Autosave enabled: writing logs to {}", path);
            Some(Box::new(OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?))
        } else {
            None
        }
    } else {
        None
    };
    
    // Primary output writer (file if -w specified, or stdout)
    let primary_writer: Box<dyn Write + Send> = if let Some(ref output_file) = args.output_file {
        info!("Primary output: writing to file {:?}", output_file);
        Box::new(std::fs::File::create(output_file)?)
    } else if autosave_writer.is_some() {
        info!("Primary output: console (with autosave to file)");
        Box::new(io::stdout())
    } else {
        info!("Primary output: console only");
        Box::new(io::stdout())
    };
    
    Ok((primary_writer, autosave_writer))
}

/// Processes filtered logs and writes to primary and autosave outputs
async fn process_filtered_logs(
    mut filtered_rx: mpsc::Receiver<LogEntry>,
    formatter: Formatter,
    mut primary_writer: Box<dyn Write + Send>,
    mut autosave_writer: Option<Box<dyn Write + Send>>,
) -> Result<()> {
    while let Some(entry) = filtered_rx.recv().await {
        // Format the log entry
        if let Some(formatted) = formatter.format_without_filtering(&entry) {
            // Write the formatted log entry to the primary output
            if let Err(e) = writeln!(primary_writer, "{}", formatted) {
                error!("Failed to write to primary output: {:?}", e);
                if e.kind() == io::ErrorKind::BrokenPipe {
                    // This typically happens when the output is piped to another program
                    // that terminates (e.g., `wake logs | head`)
                    info!("Output pipe closed, stopping");
                    break;
                }
                return Err(anyhow::anyhow!("Failed to write to primary output: {:?}", e));
            }
            
            // Flush immediately for real-time output
            if let Err(e) = primary_writer.flush() {
                error!("Failed to flush primary output: {:?}", e);
            }
            
            // Write to autosave output if configured
            if let Some(ref mut autosave_writer) = autosave_writer {
                if let Err(e) = writeln!(autosave_writer, "{}", formatted) {
                    error!("Failed to write to autosave output: {:?}", e);
                    // Continue processing even if autosave fails
                }
                
                // Flush autosave writer to ensure data is written
                if let Err(e) = autosave_writer.flush() {
                    error!("Failed to flush autosave output: {:?}", e);
                    // Continue processing even if autosave flush fails
                }
            }
        }
    }
    
    info!("Log stream processing complete");
    Ok(())
}