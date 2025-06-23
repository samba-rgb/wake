use anyhow::Result;
use cursive::{Cursive, CursiveExt};
use cursive::views::{LinearLayout, Panel, Dialog, DummyView, TextView, EditView, ScrollView, SelectView};
use cursive::traits::*;
use cursive::event::{Event, Key, MouseEvent, MouseButton};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tracing::{info, error, debug};

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use super::{event_handler, theme};
use super::filter_panel::FilterPanel;
use super::log_view::LogView;
use super::status_bar::StatusBar;

pub struct WakeApp {
    siv: Cursive,
    event_handler: Arc<Mutex<event_handler::EventHandler>>,
    log_view: Arc<Mutex<LogView>>,
    filter_panel: Arc<Mutex<FilterPanel>>,
    args: Args,
    is_paused: Arc<Mutex<bool>>,           
    buffer_size: Arc<Mutex<usize>>,        
    auto_scroll: Arc<Mutex<bool>>,         
    follow_mode: Arc<Mutex<bool>>,         
    selected_log_index: Arc<Mutex<usize>>, 
    mouse_drag_start: Arc<Mutex<Option<(usize, usize)>>>, 
    mouse_drag_end: Arc<Mutex<Option<(usize, usize)>>>,    // NEW: Track drag end position
    is_dragging: Arc<Mutex<bool>>,                         // NEW: Track if currently dragging
    selection_start_index: Arc<Mutex<Option<usize>>>,      // NEW: Start of selection
    selection_end_index: Arc<Mutex<Option<usize>>>,        // NEW: End of selection
}

impl WakeApp {
    pub fn new(args: Args) -> Result<Self> {
        let mut siv = Cursive::default();
        
        // Enable mouse support for drag operations
        siv.set_autorefresh(true);
        
        // Setup theme
        Self::setup_theme(&mut siv);
        
        // Create components with default filter value from CLI args
        let event_handler = Arc::new(Mutex::new(event_handler::EventHandler::new()));
        let log_view = Arc::new(Mutex::new(LogView::new(10000))); // Max 10k logs
        let default_filter = args.include.clone().unwrap_or_default();
        let filter_panel = Arc::new(Mutex::new(FilterPanel::new_with_default(default_filter)));
        
        Ok(Self {
            siv,
            event_handler,
            log_view,
            filter_panel,
            args,
            is_paused: Arc::new(Mutex::new(false)),
            buffer_size: Arc::new(Mutex::new(10000)),
            auto_scroll: Arc::new(Mutex::new(true)),     
            follow_mode: Arc::new(Mutex::new(true)),     
            selected_log_index: Arc::new(Mutex::new(0)), 
            mouse_drag_start: Arc::new(Mutex::new(None)), 
            mouse_drag_end: Arc::new(Mutex::new(None)), 
            is_dragging: Arc::new(Mutex::new(false)), 
            selection_start_index: Arc::new(Mutex::new(None)), 
            selection_end_index: Arc::new(Mutex::new(None)), 
        })
    }
    
    fn setup_theme(siv: &mut Cursive) {
        siv.set_theme(theme::create_wake_theme());
    }
    
    pub fn setup_ui(&mut self) -> Result<()> {
        // Connect filter panel to log view
        {
            let log_view_ref = Arc::clone(&self.log_view);
            let mut filter_panel = self.filter_panel.lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock filter panel: {}", e))?;
            
            filter_panel.set_filter_callback(move |filter_text: &str| {
                debug!("Filter callback triggered with: '{}'", filter_text);
                if let Ok(mut log_view) = log_view_ref.lock() {
                    let filter = if filter_text.is_empty() {
                        None
                    } else {
                        Some(filter_text.to_string())
                    };
                    log_view.set_filter(filter);
                } else {
                    error!("Failed to lock log view for filtering");
                }
            });
        }
        
        // Create a proper log display with selection support
        let log_display = ScrollView::new(
            SelectView::<String>::new()
                .on_select(|_s, item| {
                    // Handle log selection - update selected index
                    info!("Log selected: {}", item);
                })
                .with_name("log_list")
        ).with_name("log_scroll").scrollable();
        
        // Create main layout with proper log view integration
        let main_layout = LinearLayout::vertical()
            .child(
                self.filter_panel.lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock filter panel: {}", e))?
                    .build_view()
                    .with_name("filter_panel")
                    .fixed_height(4)
            )
            .child(DummyView.fixed_height(1)) // Separator
            .child(
                Panel::new(log_display)
                    .title("Logs (Space:Pause, f:Follow, ↑/↓:Navigate, c:Copy, i:Filter, q:Quit)")
                    .with_name("log_panel")
                    .full_screen()
            )
            .child(StatusBar::new().with_name("status_bar").fixed_height(1));
        
        self.siv.add_fullscreen_layer(main_layout);
        
        // Setup global callbacks
        self.setup_global_callbacks()?;
        
        Ok(())
    }
    
    fn setup_global_callbacks(&mut self) -> Result<()> {
        // Quit application
        self.siv.add_global_callback('q', |s| {
            info!("Quit requested - shutting down gracefully");
            s.quit();
        });

        // Help dialog
        self.siv.add_global_callback('h', Self::show_help);
        self.siv.add_global_callback('?', Self::show_help);

        // FIXED: Focus include filter with 'i' key - NO LONGER QUITS
        let log_view_ref = Arc::clone(&self.log_view);
        self.siv.add_global_callback('i', move |s| {
            info!("Include filter edit mode requested via 'i' key");
            
            // Clone the reference for use in the closure
            let log_view_ref_clone = Arc::clone(&log_view_ref);
            
            // Create a proper filter dialog that doesn't quit the app
            s.add_layer(
                Dialog::new()
                    .title("Set Include Filter")
                    .content(
                        LinearLayout::vertical()
                            .child(TextView::new("Enter filter text (supports regex):"))
                            .child(DummyView.fixed_height(1))
                            .child(
                                EditView::new()
                                    .on_submit(move |s, text| {
                                        info!("Filter submitted: '{}'", text);
                                        
                                        // Apply filter to log view
                                        if let Ok(mut log_view) = log_view_ref_clone.lock() {
                                            let filter = if text.trim().is_empty() {
                                                None
                                            } else {
                                                Some(text.trim().to_string())
                                            };
                                            log_view.set_filter(filter);
                                        }
                                        
                                        // Close dialog and return to main view
                                        s.pop_layer();
                                        
                                        // Update status to show filter is active
                                        s.call_on_name("status_bar", |status: &mut StatusBar| {
                                            status.update_filter_status(!text.trim().is_empty());
                                        });
                                    })
                                    .with_name("filter_input")
                                    .fixed_width(50)
                            )
                            .child(DummyView.fixed_height(1))
                            .child(TextView::new("Press Enter to apply, Esc to cancel"))
                    )
                    .button("Apply", |s| {
                        // Get the input text and apply filter
                        let _filter_text = s.call_on_name("filter_input", |view: &mut EditView| {
                            view.get_content().to_string()
                        }).unwrap_or_default();
                        
                        s.pop_layer();
                        info!("Filter applied via button: '{}'", _filter_text);
                    })
                    .button("Clear", |s| {
                        // Clear the filter
                        s.call_on_name("filter_input", |view: &mut EditView| {
                            view.set_content("");
                        });
                    })
                    .button("Cancel", |s| {
                        s.pop_layer();
                    })
            );
            
            // Focus the input immediately
            let _ = s.focus_name("filter_input");
        });

        // Copy selected log with 'c' key - FIXED: Only one callback
        let log_view_ref = Arc::clone(&self.log_view);
        let selection_start_ref = Arc::clone(&self.selection_start_index);
        let selection_end_ref = Arc::clone(&self.selection_end_index);
        self.siv.add_global_callback('c', move |s| {
            info!("Copy selected log(s) requested via 'c' key");
            
            if let Ok(log_view) = log_view_ref.lock() {
                // Check if we have a selection range
                let has_selection = {
                    let start = selection_start_ref.lock().unwrap();
                    let end = selection_end_ref.lock().unwrap();
                    start.is_some() && end.is_some()
                };
                
                if has_selection {
                    // Copy selected range
                    match log_view.copy_selected_range() {
                        Ok(_) => {
                            let start_idx = selection_start_ref.lock().unwrap().unwrap_or(0);
                            let end_idx = selection_end_ref.lock().unwrap().unwrap_or(0);
                            let count = (end_idx as i32 - start_idx as i32).abs() + 1;
                            s.add_layer(
                                Dialog::info(format!("✅ {} selected logs copied to clipboard!", count))
                                    .title("Copy Selection Success")
                                    .button("OK", |s| { s.pop_layer(); })
                            );
                        }
                        Err(e) => {
                            error!("Failed to copy selected range: {}", e);
                            s.add_layer(
                                Dialog::info(format!("❌ Copy failed: {}", e))
                                    .title("Copy Error")
                                    .button("OK", |s| { s.pop_layer(); })
                            );
                        }
                    }
                } else {
                    // Copy single selected log
                    match log_view.copy_selected_log() {
                        Ok(()) => {
                            s.add_layer(
                                Dialog::info("✅ Selected log copied to clipboard!")
                                    .title("Copy Success")
                                    .button("OK", |s| { s.pop_layer(); })
                            );
                        }
                        Err(e) => {
                            error!("Failed to copy selected log: {}", e);
                            s.add_layer(
                                Dialog::info(format!("❌ Copy failed: {}", e))
                                    .title("Copy Error")
                                    .button("OK", |s| { s.pop_layer(); })
                            );
                        }
                    }
                }
            }
        });

        // Copy all visible logs with Ctrl+C
        let log_view_ref = Arc::clone(&self.log_view);
        self.siv.add_global_callback(Event::CtrlChar('c'), move |s| {
            info!("Copy all visible logs requested via Ctrl+C");
            
            if let Ok(log_view) = log_view_ref.lock() {
                match log_view.copy_visible_logs() {
                    Ok(logs) => {
                        // Copy to clipboard
                        match arboard::Clipboard::new() {
                            Ok(mut clipboard) => {
                                match clipboard.set_text(logs.clone()) {
                                    Ok(()) => {
                                        let log_count = logs.lines().count();
                                        s.add_layer(
                                            Dialog::info(format!("✅ {} logs copied to clipboard!", log_count))
                                                .title("Copy All Success")
                                                .button("OK", |s| { s.pop_layer(); })
                                        );
                                        info!("Successfully copied {} logs to clipboard", log_count);
                                    }
                                    Err(e) => {
                                        error!("Failed to set clipboard: {}", e);
                                        s.add_layer(
                                            Dialog::info(format!("❌ Failed to copy to clipboard: {}", e))
                                                .title("Clipboard Error")
                                                .button("OK", |s| { s.pop_layer(); })
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to access clipboard: {}", e);
                                s.add_layer(
                                    Dialog::info(format!("❌ Could not access clipboard: {}", e))
                                        .title("Clipboard Error")
                                        .button("OK", |s| { s.pop_layer(); })
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get visible logs: {}", e);
                        s.add_layer(
                            Dialog::info(format!("❌ Failed to get logs: {}", e))
                                .title("Copy Error")
                                .button("OK", |s| { s.pop_layer(); })
                        );
                    }
                }
            }
        });

        // FIXED: Toggle follow mode with 'f' key - proper implementation
        let follow_mode_ref = Arc::clone(&self.follow_mode);
        let auto_scroll_ref = Arc::clone(&self.auto_scroll);
        self.siv.add_global_callback('f', move |s| {
            info!("Follow mode toggle requested via 'f' key");
            
            let new_follow_mode = {
                let mut follow_mode = follow_mode_ref.lock().unwrap();
                *follow_mode = !*follow_mode;
                *follow_mode
            };
            
            {
                let mut auto_scroll = auto_scroll_ref.lock().unwrap();
                *auto_scroll = new_follow_mode;
            }
            
            // Update status bar
            s.call_on_name("status_bar", |status: &mut StatusBar| {
                status.update_follow_mode(new_follow_mode);
                status.update_mode(if new_follow_mode { "FOLLOW" } else { "MANUAL" });
            });
            
            let mode_text = if new_follow_mode { "Follow mode ON" } else { "Follow mode OFF" };
            s.add_layer(Dialog::info(mode_text).button("OK", |s| { s.pop_layer(); }));
        });

        // ENHANCED: Pause/Resume with Space key
        let is_paused_ref = Arc::clone(&self.is_paused);
        let log_view_ref = Arc::clone(&self.log_view);
        self.siv.add_global_callback(' ', move |s| {
            let new_paused_state = {
                let mut is_paused = is_paused_ref.lock().unwrap();
                *is_paused = !*is_paused;
                *is_paused
            };
            
            // Update LogView pause state
            if let Ok(mut log_view) = log_view_ref.lock() {
                log_view.set_paused(new_paused_state);
            }
            
            // Update status bar with pause indicator
            s.call_on_name("status_bar", |status: &mut StatusBar| {
                status.update_mode(if new_paused_state { "PAUSED" } else { "NORMAL" });
            });
            
            // Update title to show pause state
            s.call_on_name("log_panel", |panel: &mut Panel<ScrollView<SelectView<String>>>| {
                let title = if new_paused_state {
                    "Logs [PAUSED] (Space:Resume, Mouse:Drag Select, c:Copy Selection, Ctrl+C:Copy All, q:Quit)"
                } else {
                    "Logs (Space:Pause, Mouse:Drag Select, c:Copy Selection, Ctrl+C:Copy All, q:Quit)"
                };
                panel.set_title(title);
            });
            
            let pause_text = if new_paused_state { 
                "⏸️  PAUSED - Press Space to resume" 
            } else { 
                "▶️  RESUMED - Press Space to pause" 
            };
            info!("{}", pause_text);
        });

        // Navigation keys
        let selected_index_ref = Arc::clone(&self.selected_log_index);
        self.siv.add_global_callback(Event::Key(Key::Up), move |s| {
            let mut selected_index = selected_index_ref.lock().unwrap();
            if *selected_index > 0 {
                *selected_index -= 1;
                
                // Update the SelectView selection
                s.call_on_name("log_list", |view: &mut SelectView<String>| {
                    if view.len() > *selected_index {
                        view.set_selection(*selected_index);
                    }
                });
            }
        });

        let selected_index_ref = Arc::clone(&self.selected_log_index);
        self.siv.add_global_callback(Event::Key(Key::Down), move |s| {
            let mut selected_index = selected_index_ref.lock().unwrap();
            
            // Get current log count to prevent going out of bounds
            s.call_on_name("log_list", |view: &mut SelectView<String>| {
                if *selected_index < view.len().saturating_sub(1) {
                    *selected_index += 1;
                    view.set_selection(*selected_index);
                }
            });
        });

        // ENHANCED: Mouse drag selection - Start drag
        let mouse_drag_start_ref = Arc::clone(&self.mouse_drag_start);
        let is_dragging_ref = Arc::clone(&self.is_dragging);
        let selection_start_ref = Arc::clone(&self.selection_start_index);
        self.siv.add_global_callback(Event::Mouse {
            offset: cursive::Vec2::new(0, 0),
            position: cursive::Vec2::new(0, 0),
            event: MouseEvent::Press(MouseButton::Left),
        }, move |s| {
            info!("Mouse drag selection started");
            
            // Start dragging
            {
                let mut is_dragging = is_dragging_ref.lock().unwrap();
                *is_dragging = true;
            }
            
            // Get current selected index as drag start
            let current_selection = s.call_on_name("log_list", |view: &mut SelectView<String>| {
                view.selected_id().unwrap_or(0)
            }).unwrap_or(0);
            
            {
                let mut drag_start = mouse_drag_start_ref.lock().unwrap();
                *drag_start = Some((current_selection, 0)); // Use row index, column not important for logs
            }
            
            {
                let mut selection_start = selection_start_ref.lock().unwrap();
                *selection_start = Some(current_selection);
            }
            
            info!("Drag selection started at index: {}", current_selection);
        });

        // ENHANCED: Mouse drag selection - Continue drag
        let mouse_drag_end_ref = Arc::clone(&self.mouse_drag_end);
        let is_dragging_ref = Arc::clone(&self.is_dragging);
        let selection_end_ref = Arc::clone(&self.selection_end_index);
        let log_view_ref = Arc::clone(&self.log_view);
        self.siv.add_global_callback(Event::Mouse {
            offset: cursive::Vec2::new(0, 0),
            position: cursive::Vec2::new(0, 0),
            event: MouseEvent::Hold(MouseButton::Left),
        }, move |s| {
            let is_dragging = *is_dragging_ref.lock().unwrap();
            if !is_dragging {
                return;
            }
            
            // Get current mouse position (simplified - using current selection)
            let current_selection = s.call_on_name("log_list", |view: &mut SelectView<String>| {
                view.selected_id().unwrap_or(0)
            }).unwrap_or(0);
            
            {
                let mut drag_end = mouse_drag_end_ref.lock().unwrap();
                *drag_end = Some((current_selection, 0));
            }
            
            {
                let mut selection_end = selection_end_ref.lock().unwrap();
                *selection_end = Some(current_selection);
            }
            
            // Update LogView selection
            if let Ok(mut log_view) = log_view_ref.lock() {
                let start_idx = selection_start_ref.lock().unwrap().unwrap_or(0);
                log_view.start_selection(start_idx);
                log_view.extend_selection(current_selection);
            }
        });

        // ENHANCED: Mouse drag selection - End drag
        let is_dragging_ref = Arc::clone(&self.is_dragging);
        let selection_start_ref = Arc::clone(&self.selection_start_index);
        let selection_end_ref = Arc::clone(&self.selection_end_index);
        self.siv.add_global_callback(Event::Mouse {
            offset: cursive::Vec2::new(0, 0),
            position: cursive::Vec2::new(0, 0),
            event: MouseEvent::Release(MouseButton::Left),
        }, move |s| {
            let was_dragging = {
                let mut is_dragging = is_dragging_ref.lock().unwrap();
                let result = *is_dragging;
                *is_dragging = false;
                result
            };
            
            if was_dragging {
                let start_idx = *selection_start_ref.lock().unwrap();
                let end_idx = *selection_end_ref.lock().unwrap();
                
                if let (Some(start), Some(end)) = (start_idx, end_idx) {
                    let count = (end as i32 - start as i32).abs() + 1;
                    info!("Mouse drag selection completed: {} to {} ({} logs)", start, end, count);
                    
                    // Show selection info in status
                    s.call_on_name("status_bar", |status: &mut StatusBar| {
                        status.update_mode(&format!("SELECTED {} logs", count));
                    });
                }
            }
        });

        // Clear selection with Escape
        let selection_start_ref = Arc::clone(&self.selection_start_index);
        let selection_end_ref = Arc::clone(&self.selection_end_index);
        let log_view_ref = Arc::clone(&self.log_view);
        self.siv.add_global_callback(Event::Key(Key::Esc), move |s| {
            info!("Escape pressed - clearing selection and filter");
            
            // Clear selection
            {
                let mut start = selection_start_ref.lock().unwrap();
                let mut end = selection_end_ref.lock().unwrap();
                *start = None;
                *end = None;
            }
            
            // Clear LogView selection
            if let Ok(mut log_view) = log_view_ref.lock() {
                log_view.clear_selection();
                log_view.set_filter(None);
            }
            
            // Update status
            s.call_on_name("status_bar", |status: &mut StatusBar| {
                status.update_filter_status(false);
                status.update_mode("NORMAL");
            });
            
            // Close any open dialogs
            s.pop_layer();
        });

        Ok(())
    }
    
    fn show_help(siv: &mut Cursive) {
        let help_text = "Wake - Kubernetes Log Viewer\n\n\
                        Keyboard Shortcuts:\n\
                        • q: Quit application\n\
                        • h/?: Show this help\n\
                        • i: Focus include filter input\n\
                        • Esc: Clear current filter\n\
                        • Space: Pause/Resume log streaming\n\n\
                        Log Navigation:\n\
                        • ↑/↓: Navigate log entries\n\
                        • Page Up/Down: Page through logs\n\
                        • Home/End: Jump to start/end\n\
                        • c: Copy selected log to clipboard\n\
                        • f: Toggle follow mode (auto-scroll)\n\n\
                        Mouse Support:\n\
                        • Click: Select log entry\n\
                        • Drag: Select multiple entries\n\
                        • Scroll: Navigate through logs\n\n\
                        Filtering:\n\
                        • Press 'i' to open filter dialog\n\
                        • Supports regex patterns\n\
                        • Enter: Apply filter\n\
                        • Esc: Clear filter\n\n\
                        Status Information:\n\
                        • Bottom bar shows current status\n\
                        • Follow mode indicator\n\
                        • Pause/Resume state\n\
                        • Log count and memory usage";
        
        siv.add_layer(
            Dialog::info(help_text)
                .title("Help - Wake Cursive UI")
                .button("Close", |s| { s.pop_layer(); })
                .max_width(70)
        );
    }
    
    fn update_status(&mut self, log_count: usize) {
        self.siv.call_on_name("status_bar", |status: &mut StatusBar| {
            status.update_log_count(log_count);
        });
    }
    
    fn handle_error(&mut self, error: &str) {
        error!("UI Error: {}", error);
        
        // Show error dialog for critical errors
        if error.contains("clipboard") || error.contains("kubernetes") {
            self.siv.add_layer(
                Dialog::new()
                    .title("Error")
                    .content(TextView::new(format!("An error occurred:\n\n{}", error)))
                    .button("OK", |s| { s.pop_layer(); })
                    .max_width(50)
            );
        }
    }
    
    pub async fn run_with_channel(
        &mut self,
        ui_rx: std::sync::mpsc::Receiver<LogEntry>
    ) -> Result<()> {
        info!("=== STARTING CURSIVE UI APP WITH CHANNEL ===");
        
        // Setup UI first
        if let Err(e) = self.setup_ui() {
            error!("Failed to setup UI: {}", e);
            return Err(e);
        }
        
        // Create background thread to handle incoming log entries
        let log_view_ref = Arc::clone(&self.log_view);
        let cb_sink = self.siv.cb_sink().clone();
        let is_paused_ref = Arc::clone(&self.is_paused);
        let auto_scroll_ref = Arc::clone(&self.auto_scroll);
        let follow_mode_ref = Arc::clone(&self.follow_mode);
        
        std::thread::spawn(move || {
            info!("Log handler thread: Starting to process incoming logs");
            let mut log_count = 0;
            
            while let Ok(log_entry) = ui_rx.recv() {
                // Check if paused - if so, skip processing but keep receiving
                if *is_paused_ref.lock().unwrap() {
                    continue;
                }
                
                log_count += 1;
                
                // Add log to view with error handling
                let display_logs = match log_view_ref.lock() {
                    Ok(mut log_view) => {
                        log_view.add_log(log_entry);
                        log_view.get_display_logs()
                    }
                    Err(e) => {
                        error!("Log handler thread: Failed to lock log view: {}", e);
                        continue;
                    }
                };
                
                // Update UI via callback sink
                let should_auto_scroll = *auto_scroll_ref.lock().unwrap();
                let is_follow_mode = *follow_mode_ref.lock().unwrap();
                
                if let Err(e) = cb_sink.send(Box::new(move |s: &mut Cursive| {
                    // Update status with log count
                    s.call_on_name("status_bar", |status: &mut StatusBar| {
                        status.update_log_count(log_count);
                    });
                    
                    // Update the log list with proper selection support
                    s.call_on_name("log_list", |view: &mut SelectView<String>| {
                        // Clear and repopulate (in a real implementation, this would be optimized)
                        view.clear();
                        
                        // Add logs with proper formatting
                        for log in display_logs.iter() {
                            let formatted = format!(
                                "[{}] {}/{}: {}",
                                log.timestamp,
                                log.pod,
                                log.container,
                                log.message.replace("\n", " ").replace("\r", " ")
                            );
                            view.add_item_str(&formatted);
                        }
                        
                        // Auto-scroll to bottom if in follow mode
                        if should_auto_scroll && is_follow_mode && !display_logs.is_empty() {
                            view.set_selection(display_logs.len() - 1);
                        }
                    });
                    
                    // Auto-scroll the ScrollView if needed
                    if should_auto_scroll && is_follow_mode {
                        s.call_on_name("log_scroll", |scroll: &mut ScrollView<SelectView<String>>| {
                            scroll.scroll_to_bottom();
                        });
                    }
                })) {
                    error!("Log handler thread: Failed to send UI update: {}", e);
                }
            }
        });
        
        // Start the main UI loop
        info!("Cursive UI: Starting main event loop");
        self.siv.run();
        
        info!("Cursive UI: Event loop finished");
        Ok(())
    }

    pub async fn run_with_stream(
        &mut self,
        log_stream: Pin<Box<dyn futures::Stream<Item = LogEntry> + Send>>,
    ) -> Result<()> {
        info!("Stream-based UI requested - converting to channel-based approach");
        
        // Create a channel to bridge async stream to sync UI
        let (tx, rx) = std::sync::mpsc::channel();
        
        // Spawn a task to forward stream items to the channel
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = log_stream;
            while let Some(entry) = stream.next().await {
                if tx.send(entry).is_err() {
                    break;
                }
            }
        });
        
        self.run_with_channel(rx).await
    }
}
