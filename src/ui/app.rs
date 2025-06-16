use anyhow::Result;
use arboard::Clipboard;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
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
    let mut display_manager = DisplayManager::new(args.buffer_size, args.timestamps, args.dev)?;
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
                    if let Some(input_event) = input_handler.handle_key_event(key) {
                        debug!("UI: Input event generated: {:?}", input_event);
                        match input_event {
                            InputEvent::Quit => {
                                info!("UI: Quit signal received, breaking main loop");
                                break;
                            }
                            InputEvent::ToggleAutoScroll => {
                                // Toggle auto-scroll mode
                                display_manager.auto_scroll = !display_manager.auto_scroll;
                                let status_message = if display_manager.auto_scroll {
                                    // Immediately scroll to bottom when auto-scroll is enabled
                                    let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                    display_manager.scroll_to_bottom(viewport_height);
                                    "── Auto-scroll enabled: logs will follow new entries ──"
                                } else {
                                    "── Auto-scroll disabled: logs will stay at current position ──"
                                };
                                display_manager.add_system_message(status_message);
                                info!("UI: Auto-scroll toggled to: {}", display_manager.auto_scroll);
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
                                                display_manager.add_system_message(&format!(
                                                    "── Copied {} visible log lines to clipboard ──", 
                                                    lines_copied
                                                ));
                                                info!("Successfully copied {} lines to clipboard", lines_copied);
                                            }
                                            Err(e) => {
                                                display_manager.add_system_message(&format!(
                                                    "── Failed to copy to clipboard: {} ──", 
                                                    e
                                                ));
                                                error!("Failed to copy to clipboard: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        display_manager.add_system_message(&format!(
                                            "── Failed to initialize clipboard: {} ──", 
                                            e
                                        ));
                                        error!("Failed to initialize clipboard: {}", e);
                                    }
                                }
                            }
                            InputEvent::CopySelection => {
                                // Copy selected logs to clipboard
                                let logs_text = display_manager.get_selected_logs_as_text();
                                
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        match clipboard.set_text(&logs_text) {
                                            Ok(()) => {
                                                let lines_copied = logs_text.lines().count();
                                                display_manager.add_system_message(&format!(
                                                    "── Copied {} selected log lines to clipboard ──", 
                                                    lines_copied
                                                ));
                                                info!("Successfully copied {} selected lines to clipboard", lines_copied);
                                                
                                                // Auto-return to normal mode and re-enable follow after successful copy
                                                input_handler.mode = InputMode::Normal;
                                                display_manager.clear_selection();
                                                display_manager.auto_scroll = true;
                                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                                display_manager.scroll_to_bottom(viewport_height);
                                                display_manager.add_system_message("── Returned to normal mode with follow enabled ──");
                                            }
                                            Err(e) => {
                                                display_manager.add_system_message(&format!(
                                                    "── Failed to copy selection to clipboard: {} ──", 
                                                    e
                                                ));
                                                error!("Failed to copy selection to clipboard: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        display_manager.add_system_message(&format!(
                                            "── Failed to initialize clipboard: {} ──", 
                                            e
                                        ));
                                        error!("Failed to initialize clipboard: {}", e);
                                    }
                                }
                            }
                            InputEvent::ToggleSelection => {
                                display_manager.toggle_selection();
                                if display_manager.selection.is_some() {
                                    // When entering selection mode, automatically pause auto-scroll
                                    display_manager.add_system_message("── Selection started - Auto-scroll paused ──");
                                } else {
                                    // When clearing selection, auto-scroll is automatically restored in toggle_selection()
                                    display_manager.add_system_message("── Selection cleared - Auto-scroll restored ──");
                                }
                            }
                            InputEvent::SelectUp => {
                                display_manager.select_up();
                            }
                            InputEvent::SelectDown => {
                                let viewport_height = terminal.size()?.height.saturating_sub(4) as usize;
                                display_manager.select_down(viewport_height);
                            }
                            InputEvent::EnterSelectionMode => {
                                // Expand buffer size when entering selection mode
                                display_manager.enter_selection_mode();
                                // Start selection at current cursor position
                                display_manager.toggle_selection();
                                // Pause auto-scroll to stabilize the view
                                display_manager.auto_scroll = false;
                                display_manager.add_system_message("── Selection mode entered - Buffer expanded, Auto-scroll paused ──");
                            }
                            InputEvent::ExitSelectionMode => {
                                // Clear selection and restore buffer size
                                display_manager.clear_selection();
                                display_manager.exit_selection_mode();
                                display_manager.add_system_message("── Selection mode exited - Buffer restored ──");
                            }
                            // Mouse events are handled directly in the Event::Mouse match above
                            InputEvent::MouseClick(_, _) | InputEvent::MouseDrag(_, _) | InputEvent::MouseRelease(_, _) => {
                                // These events are handled directly in the mouse event processing
                                // No additional action needed here
                            }
                        }
                        
                        // Schedule render for next frame instead of immediate render
                        last_render = std::time::Instant::now().checked_sub(render_interval).unwrap_or_else(|| std::time::Instant::now());
                    }
                },
                Event::Mouse(mouse_event) => {
                    use crossterm::event::{MouseEventKind, MouseButton};
                    
                    match mouse_event.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            // Handle mouse click for selection
                            let log_area = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Length(3), // Filter input area
                                    Constraint::Min(0),    // Log display area
                                    Constraint::Length(1), // Status bar
                                ])
                                .split(terminal.size()?)[1]; // Get log area

                            if display_manager.handle_mouse_click(mouse_event.column, mouse_event.row, log_area) {
                                // Enter selection mode automatically on mouse click and pause auto-scroll
                                input_handler.mode = InputMode::Selection;
                                display_manager.auto_scroll = false;
                                display_manager.add_system_message("── Mouse selection started - Auto-scroll paused ──");
                            }
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            // Handle mouse drag for selection extension
                            let log_area = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Length(3), // Filter input area
                                    Constraint::Min(0),    // Log display area
                                    Constraint::Length(1), // Status bar
                                ])
                                .split(terminal.size()?)[1]; // Get log area

                            display_manager.handle_mouse_drag(mouse_event.column, mouse_event.row, log_area);
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            // Handle mouse release to end selection
                            if display_manager.handle_mouse_release() {
                                if let Some(ref selection) = display_manager.selection {
                                    if selection.is_active {
                                        let lines_selected = selection.end_line - selection.start_line + 1;
                                        display_manager.add_system_message(&format!(
                                            "── Selected {} lines (click & drag to extend, Ctrl+C to copy) ──",
                                            lines_selected
                                        ));
                                    }
                                }
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

        // Process new filtered log entries in batches
        let batch_start = Instant::now();
        const BATCH_TIMEOUT: Duration = Duration::from_millis(10); // Reduced from 20ms
        const MAX_BATCH_SIZE: usize = 50; // Increased from 10
        
        let mut batch_processed = 0;
        while batch_start.elapsed() < BATCH_TIMEOUT && batch_processed < MAX_BATCH_SIZE {
            match tokio::time::timeout(Duration::from_millis(1), filtered_log_rx.recv()).await {
                Ok(Some(entry)) => {
                    pending_logs.push(entry);
                    batch_processed += 1;
                }
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout - no more logs available
            }
        }

        // Add all pending logs to display in one batch
        if !pending_logs.is_empty() {
            for entry in pending_logs.drain(..) {
                display_manager.add_log_entry(&entry);
                
                // Write to file if specified
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
            }
            
            // Check if display manager forced follow mode due to buffer size limit
            if display_manager.auto_scroll && input_handler.mode == InputMode::Selection {
                // Buffer size limit was hit, automatically exit selection mode
                input_handler.mode = InputMode::Normal;
                display_manager.exit_selection_mode();
                display_manager.add_system_message("── Automatically exited selection mode due to buffer size limit ──");
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