use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io::{self, Write};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use crate::ui::{
    display::DisplayManager,
    filter_manager::DynamicFilterManager,
    input::{InputEvent, InputHandler, InputMode},
};
use futures::Stream;

pub async fn run_app(
    log_stream: Pin<Box<dyn Stream<Item = LogEntry> + Send>>,
    args: Args,
) -> Result<()> {
    info!("=== STARTING UI APP ===");
    info!("UI: Args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    info!("UI: Follow logs: {}, tail: {}, timestamps: {}", 
          args.follow, args.tail, args.timestamps);
    
    // Setup terminal
    info!("UI: Setting up terminal...");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create application state
    info!("UI: Creating display manager and input handler...");
    let mut display_manager = DisplayManager::new(10000, args.timestamps)?;
    let mut input_handler = InputHandler::new(args.include.clone(), args.exclude.clone());
    
    // Set up file writer if output file is specified
    let mut file_writer: Option<Box<dyn Write + Send>> = if let Some(ref output_file) = args.output_file {
        info!("UI mode: Also writing logs to file: {:?}", output_file);
        Some(Box::new(std::fs::File::create(output_file)?))
    } else {
        None
    };
    
    // Create dynamic filter manager
    info!("UI: Creating dynamic filter manager with include: {:?}, exclude: {:?}", 
          args.include, args.exclude);
    let filter_manager = DynamicFilterManager::new(
        args.include.clone(),
        args.exclude.clone(),
        0, // No buffer for retroactive filtering - only apply to new logs
    )?;

    // Create channels for log processing
    info!("UI: Creating log processing channels...");
    let (raw_log_tx, raw_log_rx) = mpsc::channel::<LogEntry>(5000); // Increased from 1000 to 5000

    // Create cancellation token for graceful shutdown
    let cancellation_token = CancellationToken::new();
    let token_clone = cancellation_token.clone();

    // Start the log stream processing task with cancellation support
    info!("UI: Starting log stream processing task...");
    let log_stream_handle = tokio::spawn(async move {
        info!("LOG_STREAM_TASK: Starting to process log stream");
        tokio::pin!(log_stream);
        let mut log_count = 0;
        
        loop {
            tokio::select! {
                // Check for cancellation
                _ = token_clone.cancelled() => {
                    info!("LOG_STREAM_TASK: Received cancellation signal, shutting down gracefully");
                    break;
                }
                // Process log entries
                entry = log_stream.next() => {
                    match entry {
                        Some(log_entry) => {
                            log_count += 1;
                            if log_count <= 10 || log_count % 100 == 0 {
                                info!("LOG_STREAM_TASK: Received log entry #{}: pod={}, container={}, message={}", 
                                      log_count, log_entry.pod_name, log_entry.container_name, 
                                      log_entry.message.chars().take(50).collect::<String>());
                            }
                            
                            if let Err(_) = raw_log_tx.send(log_entry).await {
                                warn!("LOG_STREAM_TASK: Channel closed, stopping log processing");
                                break;
                            }
                        }
                        None => {
                            info!("LOG_STREAM_TASK: Log stream ended");
                            break;
                        }
                    }
                }
            }
        }
        info!("LOG_STREAM_TASK: Completed processing {} logs", log_count);
    });

    // Start the filtering task with cancellation support
    info!("UI: Starting filtering task...");
    let filter_token = cancellation_token.clone();
    let mut filtered_log_rx = filter_manager.start_filtering_with_cancellation(raw_log_rx, filter_token).await;

    // Create formatter for file output
    let formatter = if file_writer.is_some() {
        Some(crate::output::Formatter::new(&args))
    } else {
        None
    };

    info!("UI: Entering main application loop...");
    // Main application loop
    let mut last_render = std::time::Instant::now();
    let render_interval = Duration::from_millis(100); // 10 FPS
    let mut loop_count = 0;

    loop {
        loop_count += 1;
        if loop_count % 1000 == 0 {
            debug!("UI: Main loop iteration #{}", loop_count);
        }
        
        // Handle input events with higher priority - increased timeout for better responsiveness
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(input_event) = input_handler.handle_key_event(key) {
                    match input_event {
                        InputEvent::Quit => break,
                        InputEvent::UpdateIncludeFilter(pattern) => {
                            let pattern_opt = if pattern.is_empty() { None } else { Some(pattern.clone()) };
                            if let Err(e) = filter_manager.update_include_pattern(pattern_opt.clone()).await {
                                error!("Failed to update include pattern: {}", e);
                            } else {
                                // Add a filter change notification to the display (old logs remain)
                                let filter_msg = if pattern.is_empty() {
                                    "── Filter cleared: showing all new logs ──".to_string()
                                } else {
                                    format!("── Filter applied: {} (affects new logs only) ──", pattern)
                                };
                                display_manager.add_system_message(&filter_msg);
                                info!("Include filter updated: {:?}", pattern_opt);
                            }
                        }
                        InputEvent::UpdateExcludeFilter(pattern) => {
                            let pattern_opt = if pattern.is_empty() { None } else { Some(pattern.clone()) };
                            if let Err(e) = filter_manager.update_exclude_pattern(pattern_opt.clone()).await {
                                error!("Failed to update exclude pattern: {}", e);
                            } else {
                                // Add a filter change notification to the display (old logs remain)
                                let filter_msg = if pattern.is_empty() {
                                    "── Exclude filter cleared: showing all new logs ──".to_string()
                                } else {
                                    format!("── Exclude filter applied: {} (affects new logs only) ──", pattern)
                                };
                                display_manager.add_system_message(&filter_msg);
                                info!("Exclude filter updated: {:?}", pattern_opt);
                            }
                        }
                        InputEvent::ScrollUp => {
                            display_manager.scroll_up(1);
                        }
                        InputEvent::ScrollDown => {
                            let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                            display_manager.scroll_down(1, viewport_height);
                        }
                        InputEvent::ScrollToTop => {
                            display_manager.scroll_to_top();
                        }
                        InputEvent::ScrollToBottom => {
                            let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                            display_manager.scroll_to_bottom(viewport_height);
                        }
                        InputEvent::Refresh => {
                            // Only refresh display without changing logs - no retroactive filtering
                            info!("Display refreshed - old logs preserved");
                        }
                        InputEvent::ToggleHelp => {
                            input_handler.mode = if input_handler.mode == InputMode::Help {
                                InputMode::Normal
                            } else {
                                InputMode::Help
                            };
                        }
                    }
                    
                    // Force immediate render after input to improve responsiveness
                    terminal.draw(|f| {
                        display_manager.render(f, &input_handler);
                    })?;
                    last_render = std::time::Instant::now();
                    continue; // Skip log processing this iteration to prioritize UI updates
                }
            }
        }

        // Process new filtered log entries in smaller batches to avoid blocking input
        let mut display_count = 0;
        let mut batch_processed = 0;
        const MAX_BATCH_SIZE: usize = 50; // Process max 50 logs per iteration
        
        while let Ok(entry) = filtered_log_rx.try_recv() {
            display_count += 1;
            batch_processed += 1;
            
            if display_count <= 10 || display_count % 100 == 0 {
                info!("UI_DISPLAY: Processing filtered log entry #{}: pod={}, container={}, message={}", 
                      display_count, entry.pod_name, entry.container_name,
                      entry.message.chars().take(50).collect::<String>());
            }
            
            // Add to UI display FIRST with clean LogEntry
            display_manager.add_log_entry(&entry);
            
            // SEPARATELY format for file output if specified
            if let (Some(writer), Some(fmt)) = (&mut file_writer, &formatter) {
                if let Some(formatted) = fmt.format_without_filtering(&entry) {
                    if let Err(e) = writeln!(writer, "{}", formatted) {
                        error!("Failed to write to output file: {:?}", e);
                    } else {
                        // Flush immediately for real-time file output
                        let _ = writer.flush();
                    }
                }
            }
            
            // Break after processing batch to yield control back to input handling
            if batch_processed >= MAX_BATCH_SIZE {
                break;
            }
        }
        
        if batch_processed > 0 {
            info!("UI_DISPLAY: Processed {} filtered log entries in this batch", batch_processed);
            // Auto-scroll to bottom for new logs after batch processing
            let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
            display_manager.scroll_to_bottom(viewport_height);
        }

        // Render UI at regular intervals
        if last_render.elapsed() >= render_interval {
            terminal.draw(|f| {
                display_manager.render(f, &input_handler);
            })?;
            last_render = std::time::Instant::now();
        }

        // Reduced delay - only sleep if no input was processed
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Signal cancellation to the log stream processing task
    cancellation_token.cancel();

    // Wait for the log stream processing task to complete
    let _ = log_stream_handle.await;

    info!("UI: Cleaning up terminal...");
    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    info!("=== UI APP COMPLETED ===");
    Ok(())
}