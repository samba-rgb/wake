use anyhow::Result;
use arboard::Clipboard;
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Layout, Direction, Constraint},
};
use std::io::{self, Write};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use std::fs::OpenOptions;

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use crate::config::Config;
use crate::ui::{
    display::DisplayManager,
    filter_manager::DynamicFilterManager,
    input::{InputEvent, InputHandler, InputMode},
    template_ui::TemplateUI,
};

use crate::templates::executor::TemplateExecutor;
use crate::templates::registry::TemplateRegistry;
use crate::kernel::{KernelOptimizer, prefetch_log_data};
use futures::Stream;
use crate::logging::wake_logger;

pub async fn run_app(
    log_stream: Pin<Box<dyn Stream<Item = LogEntry> + Send>>,
    args: Args,
) -> Result<()> {
    info!("=== STARTING UI APP ===");
    info!("UI: Args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    info!("UI: Follow logs: {}, tail: {}, timestamps: {}", 
          args.follow, args.tail, args.timestamps);
    
    // **KERNEL OPTIMIZATIONS**: Initialize and apply cross-platform kernel optimizations
    info!("🚀 Initializing kernel-level optimizations...");
    // Initialize kernel optimizations for Linux/macOS performance
    let mut kernel_optimizer = match KernelOptimizer::new(args.dev) {
        Ok(mut optimizer) => {
            if let Err(e) = optimizer.optimize_for_log_streaming() {
                error!("Failed to apply kernel optimizations: {:?}", e);
            }
            Some(optimizer)
        }
        Err(e) => {
            error!("Failed to initialize kernel optimizer: {:?}", e);
            None
        }
    };
    
    // Apply log streaming optimizations
    if let Some(ref mut optimizer) = kernel_optimizer {
        match optimizer.optimize_for_log_streaming() {
            Ok(()) => {
                let stats = optimizer.get_performance_stats();
                info!("🚀 Kernel optimizations applied:");
                info!("   - System: {}", stats.system_name);
                info!("   - Memory efficiency: {:.1}%", stats.memory_efficiency * 100.0);
                info!("   - Network throughput: {:.1}%", stats.network_throughput * 100.0);
                info!("   - CPU optimization: {:.1}%", stats.cpu_utilization);
            }
            Err(e) => {
                warn!("⚠️ Some kernel optimizations failed: {}", e);
            }
        }
    }
    
    // Setup terminal
    info!("UI: Setting up terminal...");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Remove mouse capture entirely to allow native terminal behavior
    execute!(stdout, EnterAlternateScreen)?;
    // Ensure the cursor is set to its default shape
    execute!(stdout, crossterm::cursor::SetCursorStyle::DefaultUserShape)?;
    // Ensure mouse capture is disabled in non-follow mode
    if !args.follow {
        execute!(stdout, crossterm::event::DisableMouseCapture)?;
    }
    // Note: Mouse capture is disabled by default to allow terminal text selection
    // Users can press 'm' to enable application mouse features if needed
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize template system
    let mut template_registry = TemplateRegistry::new();
    template_registry.load_builtin_templates();
    let template_executor = TemplateExecutor::new(template_registry);
    let template_ui = TemplateUI::new(template_executor);

    // Create application state
    info!("UI: Creating display manager and input handler...");
    let mut display_manager = DisplayManager::new(args.buffer_size, args.timestamps, args.dev)?;
    let mut input_handler = InputHandler::new(args.include.clone(), args.exclude.clone());
    
    // Load configuration for autosave functionality
    let config = Config::load().unwrap_or_default();
    info!("UI: Loaded autosave config - enabled: {}, path: {:?}", 
          config.autosave.enabled, config.autosave.path);
    
    // Set up file writer if output file is specified
    let mut file_writer: Option<Box<dyn Write + Send>> = if let Some(ref output_file) = args.output_file {
        info!("UI mode: Also writing logs to file: {:?}", output_file);
        Some(Box::new(std::fs::File::create(output_file)?))
    } else {
        None
    };
    
    // Set up autosave writer if autosave is enabled and no -w flag
    let mut autosave_writer: Option<Box<dyn Write + Send>> = if config.autosave.enabled && args.output_file.is_none() {
        // Get the appropriate autosave path
        let autosave_path = if let Some(ref configured_path) = config.autosave.path {
            // Use the same path resolution logic as non-UI mode
            crate::logging::determine_autosave_path(configured_path)?
        } else {
            // Generate timestamp-based filename
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            format!("wake_{}.log", timestamp)
        };
        
        info!("UI mode: Autosave enabled - writing logs to: {}", autosave_path);
        display_manager.add_system_log(&format!("💾 Autosave enabled: {}", autosave_path));
        
        Some(Box::new(OpenOptions::new()
            .create(true)
            .append(true)
            .open(&autosave_path)?))
    } else if config.autosave.enabled && args.output_file.is_some() {
        info!("UI mode: Autosave config enabled but -w flag takes precedence");
        display_manager.add_system_log("💾 Autosave: Using -w flag file instead of configured path");
        None
    } else {
        info!("UI mode: Autosave disabled in configuration");
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

    // Configure file output mode if file_writer is present
    if file_writer.is_some() {
        display_manager.set_file_output_mode(true);
        info!("UI: File output mode enabled - logs will be saved to file");
    }

    // Start with normal buffer size (follow mode uses original buffer)
    info!("UI: Starting with original buffer size: {} (follow mode)", args.buffer_size);

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
    let render_interval = Duration::from_millis(16); // ~60 FPS max
    let mut pending_logs = Vec::new(); // Buffer for batching log entries
    
    loop {
        // Check for cancellation
        if cancellation_token.is_cancelled() {
            break;
        }

        // Handle input events with better polling
        if let Ok(true) = event::poll(Duration::from_millis(50)) {
            match event::read()? {
                Event::Key(key) => {
                    debug!("UI: Key event received: {:?} in mode {:?}", key, input_handler.mode);
                    
                    // If memory warning is active, only dismiss on specific keys (not mouse events)
                    if display_manager.memory_warning_active {
                        // Only dismiss memory warning on specific keyboard keys, not mouse events
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('q') => {
                                display_manager.dismiss_memory_warning();
                                // Continue processing this input event after dismissing warning
                            }
                            _ => {
                                // For other keys, dismiss warning but continue processing the key
                                display_manager.dismiss_memory_warning();
                            }
                        }
                        // Don't skip input processing - let it continue normally
                    }
                    
                    if let Some(input_event) = input_handler.handle_key_event(key) {
                        debug!("UI: Input event generated: {:?}", input_event);
                        match input_event {
                            InputEvent::Quit => {
                                info!("UI: Quit signal received, performing cleanup before exit");
                                
                                // Explicit buffer cleanup for better performance
                                display_manager.clear_all_buffers();
                                
                                // Signal cancellation to background tasks
                                cancellation_token.cancel();
                                
                                info!("UI: Buffers cleared and shutdown initiated");
                                break;
                            }
                            InputEvent::ToggleAutoScroll => {
                                // Use the new toggle_follow_mode which manages buffer expansion
                                display_manager.toggle_follow_mode();
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
                            InputEvent::UpdateIncludeFilter(pattern) => {

                                // Validate the pattern before proceeding
                                let pattern_opt = if pattern.is_empty() {
                                    None
                                } else if let Err(e) = regex::Regex::new(&pattern) {
                                    // Show an alert dialog for invalid pattern
                                    // display_manager.show_popup_alert(frame, message);
                                    // Log the error message using formatted string
                                    display_manager.add_system_log(&format!("Invalid include filter pattern: {}", e));
                                    None
                                } else {
                                    Some(pattern.clone())
                                };

                                if let Err(e) = filter_manager.update_include_pattern(pattern_opt.clone()).await {
                                    // display_manager.show_popup(&format!(
                                    //     "❌ Failed to update include pattern: {}", e
                                    // ));
                                    error!("Failed to update include pattern: {}", e);
                                } else {
                                    // Add a filter change notification to the display (old logs remain)
                                    let filter_msg = if pattern.is_empty() {
                                        "── Filter cleared: showing all new logs ──".to_string()
                                    } else {
                                        format!("── Filter applied: {} (affects new logs only) ──", pattern)
                                    };
                                    display_manager.add_system_log(&filter_msg);
                                    info!("Include filter updated: {:?}", pattern_opt);
                                }
                            }
                            InputEvent::UpdateExcludeFilter(pattern) => {
                                let pattern_opt = if pattern.is_empty() { None } else { Some(pattern.clone()) };
                                if let Err(e) = filter_manager.update_exclude_pattern(pattern_opt.clone()).await {
                                    // Log the error message using formatted string for exclude filter
                                    display_manager.add_system_log(&format!("Invalid exclude filter pattern: {}", e));
                                    error!("Failed to update exclude pattern: {}", e);
                                } else {
                                    // Add a filter change notification to the display (old logs remain)
                                    let filter_msg = if pattern.is_empty() {
                                        "── Exclude filter cleared: showing all new logs ──".to_string()
                                    } else {
                                        format!("── Exclude filter applied: {} (affects new logs only) ──", pattern)
                                    };
                                    display_manager.add_system_log(&filter_msg);
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
                            InputEvent::ScrollPageUp => {
                                // Scroll up by a full page (minus a couple lines for context)
                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                let page_size = viewport_height.saturating_sub(2).max(1);
                                display_manager.scroll_up(page_size);
                            }
                            InputEvent::ScrollPageDown => {
                                // Scroll down by a full page (minus a couple lines for context)
                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                let page_size = viewport_height.saturating_sub(2).max(1);
                                display_manager.scroll_down(page_size, viewport_height);
                            }
                            InputEvent::ScrollToTop => {
                                display_manager.scroll_to_top();
                            }
                            InputEvent::ScrollToBottom => {
                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                display_manager.scroll_to_bottom(viewport_height);
                            }
                            InputEvent::CopyLogs => {
                                // Copy currently visible logs to clipboard
                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                let logs_text = display_manager.get_visible_logs_as_text(viewport_height);
                                
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        match clipboard.set_text(&logs_text) {
                                            Ok(()) => {
                                                let lines_copied = logs_text.lines().count();
                                                display_manager.add_system_log(&format!(
                                                    "── Copied {} visible log lines to clipboard ──", 
                                                    lines_copied
                                                ));
                                                info!("Successfully copied {} lines to clipboard", lines_copied);
                                            }
                                            Err(e) => {
                                                display_manager.add_system_log(&format!(
                                                    "── Failed to copy to clipboard: {} ──", 
                                                    e
                                                ));
                                                error!("Failed to copy to clipboard: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        display_manager.add_system_log(&format!(
                                            "── Failed to initialize clipboard: {} ──", 
                                            e
                                        ));
                                        error!("Failed to initialize clipboard: {}", e);
                                    }
                                }
                            }
                            InputEvent::CopySelection => {
                                // Smart copy: if there's a selection, copy it; otherwise copy visible logs
                                let selected_text = display_manager.get_selected_logs_as_text();
                                
                                if selected_text != "No text selected" {
                                    // Copy the selection
                                    match Clipboard::new() {
                                        Ok(mut clipboard) => {
                                            match clipboard.set_text(&selected_text) {
                                                Ok(()) => {
                                                    let lines_copied = selected_text.lines().count();
                                                    display_manager.add_system_log(&format!(
                                                        "── Copied {} selected lines to clipboard ──", 
                                                        lines_copied
                                                    ));
                                                    info!("Successfully copied {} selected lines to clipboard", lines_copied);
                                                    // Clear selection after copying
                                                    display_manager.clear_selection();
                                                }
                                                Err(e) => {
                                                    display_manager.add_system_log(&format!(
                                                        "── Failed to copy selection to clipboard: {} ──", 
                                                        e
                                                    ));
                                                    error!("Failed to copy selection to clipboard: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            display_manager.add_system_log(&format!(
                                                "── Failed to initialize clipboard: {} ──", 
                                                e
                                            ));
                                            error!("Failed to initialize clipboard: {}", e);
                                        }
                                    }
                                } else {
                                    // No selection - copy visible logs instead
                                    let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                    let logs_text = display_manager.get_visible_logs_as_text(viewport_height);
                                    
                                    match Clipboard::new() {
                                        Ok(mut clipboard) => {
                                            match clipboard.set_text(&logs_text) {
                                                Ok(()) => {
                                                    let lines_copied = logs_text.lines().count();
                                                    display_manager.add_system_log(&format!(
                                                        "── Copied {} visible log lines to clipboard ──", 
                                                        lines_copied
                                                    ));
                                                    info!("Successfully copied {} lines to clipboard", lines_copied);
                                                }
                                                Err(e) => {
                                                    display_manager.add_system_log(&format!(
                                                        "── Failed to copy to clipboard: {} ──", 
                                                        e
                                                    ));
                                                    error!("Failed to copy to clipboard: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            display_manager.add_system_log(&format!(
                                                "── Failed to initialize clipboard: {} ──", 
                                                e
                                            ));
                                            error!("Failed to initialize clipboard: {}", e);
                                        }
                                    }
                                }
                            }
                            InputEvent::SelectUp => {
                                // Only allow selection extension in pause mode
                                if !display_manager.auto_scroll {
                                    display_manager.select_up();
                                } else {
                                    display_manager.add_system_log("📝 Selection only available in pause mode (press 'f' to pause)");
                                }
                            }
                            InputEvent::SelectDown => {
                                // Only allow selection extension in pause mode
                                if !display_manager.auto_scroll {
                                    let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                    display_manager.select_down(viewport_height);
                                } else {
                                    display_manager.add_system_log("📝 Selection only available in pause mode (press 'f' to pause)");
                                }
                            }
                            InputEvent::ToggleSelection => {
                                // Only allow selection toggle in pause mode
                                if !display_manager.auto_scroll {
                                    if display_manager.hash_selection.is_some() {
                                        display_manager.clear_selection();
                                    } else {
                                        // Start selection at current scroll position - this functionality
                                        // is now handled by mouse clicks or keyboard selection
                                        display_manager.add_system_log("📝 Use mouse click or Shift+arrows to start selection");
                                    }
                                } else {
                                    display_manager.add_system_log("📝 Text selection only available in pause mode (press 'f' to pause)");
                                }
                            }
                            InputEvent::SelectAll => {
                                // Only allow select all in pause mode
                                if !display_manager.auto_scroll {
                                    display_manager.toggle_selection_all();
                                } else {
                                    display_manager.add_system_log("📝 Select all only available in pause mode (press 'f' to pause)");
                                }
                            }
                            // Remove all other selection-related events
                            _ => {
                                // Ignore any remaining unused events
                            }
                        }
                        
                        // Schedule render for next frame instead of immediate render
                        last_render = std::time::Instant::now().checked_sub(render_interval).unwrap_or_else(|| std::time::Instant::now());
                    }
                },
                Event::Mouse(mouse_event) => {
                    use crossterm::event::{MouseEventKind, MouseButton};
                    
                    // If memory warning is active, dismiss it on any mouse event but still process the mouse event
                    if display_manager.memory_warning_active {
                        display_manager.dismiss_memory_warning();
                        display_manager.add_system_log("📊 Memory warning dismissed by mouse event");
                        // Continue processing the mouse event normally - don't skip it
                    }
                    
                    match mouse_event.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            // Handle mouse click for starting selection (only in pause mode)
                            let log_area = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Length(3), // Filter input area
                                    Constraint::Min(0),    // Log display area
                                    Constraint::Length(1), // Status bar
                                ])
                                .split(terminal.size()?)[1]; // Get log area
                            
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            // Handle mouse drag for extending selection with auto-scroll
                            let log_area = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Length(4), // Filter input area - FIXED
                                    Constraint::Min(0),    // Log display area
                                    Constraint::Length(2), // Status bar
                                ])
                                .split(terminal.size()?)[1]; // Get log area

                        
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            // Handle mouse release to end dragging
                            if display_manager.handle_mouse_release() {
                                // Selection completed successfully
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                            // Scroll down by 3 lines for smoother experience
                            display_manager.scroll_down(3, viewport_height);
                        },
                        MouseEventKind::ScrollUp => {
                            // Scroll up by 3 lines for smoother experience
                            display_manager.scroll_up(3);
                        },
                        _ => {}  // Ignore all other mouse events
                    }
                },
                _ => {}  // Ignore other event types
            }
        }

        // Process new filtered log entries in batches with kernel optimizations
        let batch_start = Instant::now();
        const BATCH_TIMEOUT: Duration = Duration::from_millis(10); // Reduced from 20ms
        const MAX_BATCH_SIZE: usize = 50; // Increased from 10
        
        let mut batch_processed = 0;
        while batch_start.elapsed() < BATCH_TIMEOUT && batch_processed < MAX_BATCH_SIZE {
            match tokio::time::timeout(Duration::from_millis(1), filtered_log_rx.recv()).await {
                Ok(Some(entry)) => {
                    // **KERNEL OPTIMIZATION**: Prefetch log data for better cache performance
                    let message_bytes = entry.message.as_bytes();
                    if message_bytes.len() > 64 {
                        prefetch_log_data(message_bytes.as_ptr(), message_bytes.len());
                    }
                    
                    pending_logs.push(entry);
                    batch_processed += 1;
                }
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout - no more logs available
            }
        }

        // Add all pending logs to display in one batch
        if !pending_logs.is_empty() {
            let mut logs_inserted = 0;
            let mut logs_skipped = 0;
            
            // **ENHANCED MEMORY FULL HANDLING**: Track total logs processed vs inserted
            let total_logs_in_batch = pending_logs.len();
            
            for entry in pending_logs.drain(..) {
                // Try to add to display buffer - returns true if inserted, false if skipped
                let inserted = display_manager.add_log_entry(&entry);
                
                if inserted {
                    logs_inserted += 1;
                } else {
                    logs_skipped += 1;
                }
                
                // Always write to file if specified (even if display buffer is full)
                if let Some(ref mut file_writer) = file_writer {
                    if let Some(ref formatter) = formatter {
                        if let Some(formatted) = formatter.format_without_filtering(&entry) {
                            if let Err(e) = writeln!(file_writer, "{}", formatted) {
                                error!("Failed to write to output file: {:?}", e);
                            } else {
                                let _ = file_writer.flush();
                            }
                        }
                    }
                }
                
                // Always write to autosave file if configured (even if display buffer is full)
                if let Some(ref mut autosave_writer) = autosave_writer {
                    let formatter = crate::output::Formatter::new(&args);
                    if let Some(formatted) = formatter.format_without_filtering(&entry) {
                        if let Err(e) = writeln!(autosave_writer, "{}", formatted) {
                            error!("Failed to write to autosave file: {:?}", e);
                            // Continue processing even if autosave fails
                        } else {
                            // Flush autosave writer to ensure data is written
                            if let Err(e) = autosave_writer.flush() {
                                error!("Failed to flush autosave file: {:?}", e);
                            }
                        }
                    }
                }
            }
            
            // **CRITICAL FIX**: Enhanced memory full feedback and user guidance
            if logs_skipped > 0 {
                // Check if we're at memory capacity
                let memory_usage = display_manager.get_memory_usage_percent();
                
                if memory_usage >= 100.0 {
                    // At capacity - provide actionable feedback
                    if logs_skipped % 50 == 0 {  // Reduced frequency for less noise
                        let guidance_msg = if display_manager.file_output_mode {
                            format!("🔄 Buffer full: {} logs saved to file, {} display entries skipped. Press 'f' to resume or scroll to bottom", 
                                   logs_inserted + logs_skipped, logs_skipped)
                        } else {
                            format!("⚠️ Buffer full: {} logs LOST! Consider: 1) Press 'f' to resume 2) Use -w flag for file output 3) Increase --buffer-size", 
                                   logs_skipped)
                        };
                        
                        display_manager.add_system_log(&guidance_msg);
                        info!("MEMORY_FULL: {} logs processed, {} inserted to display, {} skipped", 
                              total_logs_in_batch, logs_inserted, logs_skipped);
                    }
                } else {
                    // Not at full capacity but still skipping - this shouldn't happen
                    display_manager.add_system_log(&format!(
                        "🔍 Unexpected: {} logs skipped at {}% capacity - this may indicate a logic issue", 
                        logs_skipped, memory_usage
                    ));
                }
            }
            
            // Check memory warning after adding logs (outside of render cycle)
            display_manager.check_memory_warning();
            
            // **IMPROVED LOGGING**: More informative status when skipping logs
            if logs_skipped > 0 && logs_skipped % 100 == 0 {
                let status_msg = if display_manager.file_output_mode {
                    format!("UI: Buffer management active - {} inserted to display, {} saved to file only", 
                           logs_inserted, logs_skipped)
                } else {
                    format!("UI: WARNING - {} logs lost due to full buffer, {} inserted to display", 
                           logs_skipped, logs_inserted)
                };
                info!("{}", status_msg);
            }
            
            // Auto-scroll if enabled
            if display_manager.auto_scroll {
                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                display_manager.scroll_to_bottom(viewport_height);
            }
        }

        // Render UI at controlled intervals
        if last_render.elapsed() >= render_interval {
            terminal.draw(|f| {
                display_manager.render(f, &input_handler);
            })?;
            last_render = std::time::Instant::now();
        }

        // Reduced sleep time for better responsiveness
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    // Signal cancellation to the log stream processing task
    cancellation_token.cancel();

    // Wait for the log stream processing task to complete with a timeout
    // Don't wait indefinitely as the underlying stream may not be cancellation-aware
    match tokio::time::timeout(Duration::from_millis(1000), log_stream_handle).await {
        Ok(_) => {
            info!("UI: Log stream task completed gracefully");
        }
        Err(_) => {
            warn!("UI: Log stream task did not complete within timeout, proceeding with cleanup");
            // The task will be dropped when the program exits
        }
    }

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

pub async fn run_monitor_app(args: Args) -> Result<()> {
    info!("=== STARTING MONITOR APP ===");
    info!("Monitor: Args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    
    // Initialize Kubernetes client
    info!("Monitor: Creating Kubernetes client...");
    let client = crate::k8s::client::create_client(&args).await?;
    
    // Get pod regex
    let pod_regex = args.pod_regex()?;
    let container_regex = args.container_regex()?;
    
    // Select pods
    info!("Monitor: Selecting pods...");
    let pods = crate::k8s::pod::select_pods(
        &client,
        &args.namespace,
        &pod_regex,
        &container_regex,
        args.all_namespaces,
        args.resource.as_deref(),
        args.sample,
    ).await?;
    
    if pods.is_empty() {
        wake_logger::error("❌ No pods found matching the criteria.");
        wake_logger::error(&format!("   Namespace: {}", args.namespace));
        wake_logger::error(&format!("   Pod selector: {}", args.pod_selector));
        wake_logger::error(&format!("   Container: {}", args.container));
        return Ok(());
    }
    
    info!("Monitor: Found {} matching pods", pods.len());
    
    // Convert pods to PodInfo for the monitor UI
    let pod_infos: Vec<crate::k8s::pod::PodInfo> = pods.iter().map(|pod| {
        crate::k8s::pod::PodInfo {
            name: pod.name.clone(),
            namespace: pod.namespace.clone(),
            containers: pod.containers.clone(),
            cpu_usage_percent: None,
            memory_usage_percent: None,
            memory_usage_bytes: None,
            memory_limit_bytes: None,
        }
    }).collect();
    
    // Run the monitor UI
    crate::ui::monitor::start_monitor(&args, pod_infos).await?;
    
    info!("=== MONITOR APP COMPLETED ===");
    Ok(())
}

// Helper function to get pod resource usage from kubectl top
async fn get_pod_resource_usage(
    client: &kube::Client,
    namespace: &str,
    pod_name: &str,
) -> Result<Vec<(String, (f64, f64))>> {
    use std::process::Command;
    
    // Use kubectl top pod to get resource usage
    let output = Command::new("kubectl")
        .args(&["top", "pod", pod_name, "-n", namespace, "--containers", "--no-headers"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get pod resource usage: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut container_resources = Vec::new();
    
    // Parse each container's usage - improved parsing logic
    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 { // We need pod, container, CPU, and memory
            let container_name = parts[1].to_string();
            
            // Parse CPU usage (remove "m" suffix and convert to percentage)
            let cpu_str = parts[2];
            let cpu_usage = if cpu_str.ends_with('m') {
                // Convert millicores to percentage (1000m = 1 core = 100%)
                let millicores = cpu_str.trim_end_matches('m')
                    .parse::<f64>()
                    .unwrap_or_else(|_| {
                        warn!("Failed to parse CPU millicores: {}", cpu_str);
                        0.0
                    });
                millicores / 10.0 // 1000m = 100%
            } else {
                // Direct core count to percentage
                cpu_str.parse::<f64>()
                    .unwrap_or_else(|_| {
                        warn!("Failed to parse CPU cores: {}", cpu_str);
                        0.0
                    }) * 100.0 // Convert cores to percentage
            };
            
            // Parse Memory usage
            let mem_str = parts[3]; // Using index 3 for memory in the container case
            
            // Better memory parsing to handle all formats (Ki, Mi, Gi, etc.)
            let mem_value: f64;
            let mem_unit = mem_str.chars()
                .skip_while(|c| c.is_digit(10) || *c == '.')
                .collect::<String>();
            
            let mem_number = mem_str.chars()
                .take_while(|c| c.is_digit(10) || *c == '.')
                .collect::<String>()
                .parse::<f64>()
                .unwrap_or_else(|_| {
                    warn!("Failed to parse memory value: {}", mem_str);
                    0.0
                });
                
            // Convert to MB based on unit
            mem_value = match mem_unit.as_str() {
                "Ki" => mem_number / 1024.0,
                "Mi" => mem_number,
                "Gi" => mem_number * 1024.0,
                "Ti" => mem_number * 1024.0 * 1024.0,
                "K" | "k" => mem_number / 1000.0,
                "M" => mem_number,
                "G" => mem_number * 1000.0,
                "T" => mem_number * 1000.0 * 1000.0,
                _ => mem_number / (1024.0 * 1024.0), // Assume bytes if no unit
            };
            
            // Debug logging to trace the values
            info!(
                "Parsed container metrics: {} - CPU: {}% ({}), Memory: {}MB ({})",
                container_name, cpu_usage, cpu_str, mem_value, mem_str
            );
            
            container_resources.push((container_name, (cpu_usage, mem_value)));
        } else if parts.len() == 3 {
            // This is a case where only one container exists (pod name, cpu, memory)
            let container_name = "default".to_string();
            
            // Parse CPU usage (remove "m" suffix and convert to percentage)
            let cpu_str = parts[1];
            let cpu_usage = if cpu_str.ends_with('m') {
                // Convert millicores to percentage (1000m = 1 core = 100%)
                let millicores = cpu_str.trim_end_matches('m')
                    .parse::<f64>()
                    .unwrap_or_else(|_| {
                        warn!("Failed to parse CPU millicores: {}", cpu_str);
                        0.0
                    });
                millicores / 10.0 // 1000m = 100%
            } else {
                // Direct core count to percentage
                cpu_str.parse::<f64>()
                    .unwrap_or_else(|_| {
                        warn!("Failed to parse CPU cores: {}", cpu_str);
                        0.0
                    }) * 100.0 // Convert cores to percentage
            };
            
            // Parse Memory usage
            let mem_str = parts[2];
            
            // Better memory parsing to handle all formats
            let mem_value: f64;
            let mem_unit = mem_str.chars()
                .skip_while(|c| c.is_digit(10) || *c == '.')
                .collect::<String>();
            
            let mem_number = mem_str.chars()
                .take_while(|c| c.is_digit(10) || *c == '.')
                .collect::<String>()
                .parse::<f64>()
                .unwrap_or_else(|_| {
                    warn!("Failed to parse memory value: {}", mem_str);
                    0.0
                });
                
            // Convert to MB based on unit
            mem_value = match mem_unit.as_str() {
                "Ki" => mem_number / 1024.0,
                "Mi" => mem_number,
                "Gi" => mem_number * 1024.0,
                "Ti" => mem_number * 1024.0 * 1024.0,
                "K" | "k" => mem_number / 1000.0,
                "M" => mem_number,
                "G" => mem_number * 1000.0,
                "T" => mem_number * 1000.0 * 1000.0,
                _ => mem_number / (1024.0 * 1024.0), // Assume bytes if no unit
            };
            
            // Debug logging for the default container case
            info!(
                "Parsed default container metrics: {} - CPU: {}% ({}), Memory: {}MB ({})",
                container_name, cpu_usage, cpu_str, mem_value, mem_str
            );
            
            container_resources.push((container_name, (cpu_usage, mem_value)));
        }
    }
    
    // Log the results for debugging
    info!(
        "Retrieved resource usage for pod {}: {} containers with data", 
        pod_name, container_resources.len()
    );
    
    Ok(container_resources)
}