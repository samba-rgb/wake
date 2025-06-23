use arboard::Clipboard;
use anyhow::Result;
use cursive::event::{Event, EventResult, Key};
use cursive::views::{ScrollView, TextView};
use cursive::View;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};

use crate::k8s::logs::LogEntry;

#[derive(Debug, Clone)]
pub struct LogDisplayEntry {
    pub timestamp: String,
    pub pod: String,
    pub container: String,
    pub message: String,
    #[allow(dead_code)]
    pub raw_entry: LogEntry,
}

impl LogDisplayEntry {
    pub fn format_for_display(&self) -> cursive::utils::markup::StyledString {
        let mut styled = cursive::utils::markup::StyledString::new();

        // Add timestamp with dim effect
        styled.append_styled(&format!("[{}] ", self.timestamp), cursive::theme::Effect::Dim);

        // Add pod name in bold
        styled.append_styled(&format!("{}/", self.pod), cursive::theme::Effect::Bold);

        // Add container name
        styled.append_plain(&format!("{}: ", self.container));

        // Add message
        styled.append_plain(&self.message);

        styled
    }

    pub fn format_for_copy(&self) -> String {
        format!(
            "[{}] {}/{}: {}",
            self.timestamp, self.pod, self.container, self.message
        )
    }
}

#[derive(Clone)]
pub struct LogView {
    logs: Arc<Mutex<VecDeque<LogEntry>>>,
    filtered_logs: Arc<Mutex<VecDeque<LogEntry>>>,
    selected_index: usize,
    max_logs: usize,
    auto_scroll: bool,
    current_filter: Option<String>,
    follow_mode: bool,
    scroll_offset: usize,
    is_paused: bool,          // NEW: Track pause state locally
    selection_start: Option<usize>, // NEW: For multi-selection support
    selection_end: Option<usize>,   // NEW: For multi-selection support
}

impl LogView {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: Arc::new(Mutex::new(VecDeque::new())),
            filtered_logs: Arc::new(Mutex::new(VecDeque::new())),
            selected_index: 0,
            max_logs,
            auto_scroll: true,
            current_filter: None,
            follow_mode: true,
            scroll_offset: 0,
            is_paused: false,      // NEW: Start unpaused
            selection_start: None, // NEW: No selection initially
            selection_end: None,   // NEW: No selection initially
        }
    }

    // NEW: Pause/Resume functionality
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
        if paused {
            self.follow_mode = false; // Disable follow mode when paused
            self.auto_scroll = false;
        }
        info!("LogView paused state changed to: {}", paused);
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        // Don't add logs if paused
        if self.is_paused {
            return;
        }

        {
            let mut logs = self.logs.lock().unwrap();
            logs.push_back(entry.clone());

            // Keep only the last max_logs entries with rotation
            if logs.len() > self.max_logs {
                logs.pop_front();
                // Adjust scroll offset and selection when rotating
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                if self.selected_index > 0 {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
        }

        // Apply current filter
        self.apply_filter();
        
        // Auto-scroll to bottom in follow mode
        if self.follow_mode && self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn toggle_follow_mode(&mut self) {
        self.follow_mode = !self.follow_mode;
        self.auto_scroll = self.follow_mode;
        
        if self.follow_mode {
            // When enabling follow mode, scroll to bottom
            self.scroll_to_bottom();
        }
        
        info!("Follow mode toggled to: {}", self.follow_mode);
    }

    pub fn is_follow_mode(&self) -> bool {
        self.follow_mode
    }

    // IMPROVED: Better scroll handling
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.follow_mode = false; // Disable follow when manually scrolling
        self.auto_scroll = false;
        
        // Move selection up if needed
        if self.selected_index > 0 {
            self.selected_index = self.selected_index.saturating_sub(lines.min(self.selected_index));
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        let max_scroll = filtered_logs.len().saturating_sub(20); // Keep some buffer
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
        
        // Move selection down if needed
        let max_selection = filtered_logs.len().saturating_sub(1);
        self.selected_index = (self.selected_index + lines).min(max_selection);
        
        // Check if we're at bottom - if so, re-enable follow mode
        if self.scroll_offset >= max_scroll {
            self.follow_mode = true;
            self.auto_scroll = true;
        } else {
            self.follow_mode = false;
            self.auto_scroll = false;
        }
    }

    // NEW: Selection management
    pub fn set_selected_index(&mut self, index: usize) {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        if index < filtered_logs.len() {
            self.selected_index = index;
            
            // Disable follow mode when manually selecting
            if self.follow_mode && index < filtered_logs.len().saturating_sub(5) {
                self.follow_mode = false;
                self.auto_scroll = false;
            }
        }
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected_index
    }

    // NEW: Multi-selection support for drag operations
    pub fn start_selection(&mut self, index: usize) {
        self.selection_start = Some(index);
        self.selection_end = Some(index);
        self.selected_index = index;
    }

    pub fn extend_selection(&mut self, index: usize) {
        if self.selection_start.is_some() {
            self.selection_end = Some(index);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    pub fn get_selection_range(&self) -> Option<(usize, usize)> {
        match (self.selection_start, self.selection_end) {
            (Some(start), Some(end)) => {
                let min_idx = start.min(end);
                let max_idx = start.max(end);
                Some((min_idx, max_idx))
            }
            _ => None,
        }
    }

    pub fn set_filter(&mut self, filter: Option<String>) {
        debug!("Setting filter: {:?}", filter);
        self.current_filter = filter;
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let logs = self.logs.lock().unwrap();
        let mut filtered = self.filtered_logs.lock().unwrap();

        filtered.clear();

        if let Some(ref filter_text) = self.current_filter {
            let filter_lower = filter_text.to_lowercase();
            for log in logs.iter() {
                if log.message.to_lowercase().contains(&filter_lower)
                    || log.pod_name.to_lowercase().contains(&filter_lower)
                    || log.container_name.to_lowercase().contains(&filter_lower)
                {
                    filtered.push_back(log.clone());
                }
            }
        } else {
            // No filter - show all logs
            for log in logs.iter() {
                filtered.push_back(log.clone());
            }
        }

        // Adjust selected index if needed
        if self.selected_index >= filtered.len() && !filtered.is_empty() {
            self.selected_index = filtered.len() - 1;
        }
    }

    pub fn copy_selected_log(&self) -> Result<(), String> {
        let filtered_logs = self.filtered_logs.lock().unwrap();

        if filtered_logs.is_empty() {
            return Err("No logs available to copy".to_string());
        }

        if self.selected_index >= filtered_logs.len() {
            return Err("Selected log index out of range".to_string());
        }

        let log_entry = &filtered_logs[self.selected_index];
        let formatted_log = self.format_log_for_copy(log_entry);

        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(formatted_log) {
                Ok(()) => {
                    info!("Successfully copied log to clipboard");
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to set clipboard text: {}", e);
                    Err(format!("Failed to copy to clipboard: {}", e))
                }
            },
            Err(e) => {
                error!("Failed to create clipboard: {}", e);
                Err(format!("Failed to access clipboard: {}", e))
            }
        }
    }

    // NEW: Copy selected range of logs - ENHANCED for mouse drag
    pub fn copy_selected_range(&self) -> Result<String, String> {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        
        if filtered_logs.is_empty() {
            return Err("No logs available to copy".to_string());
        }

        let (start_idx, end_idx) = match self.get_selection_range() {
            Some(range) => range,
            None => (self.selected_index, self.selected_index), // Single selection fallback
        };

        let mut result = String::new();
        let actual_start = start_idx.min(end_idx);
        let actual_end = start_idx.max(end_idx);
        
        for i in actual_start..=actual_end.min(filtered_logs.len().saturating_sub(1)) {
            if let Some(log) = filtered_logs.get(i) {
                let formatted_log = self.format_log_for_copy(log);
                result.push_str(&formatted_log);
                result.push('\n');
            }
        }

        // Remove trailing newline
        if result.ends_with('\n') {
            result.pop();
        }

        // Copy to clipboard automatically
        match Clipboard::new() {
            Ok(mut clipboard) => {
                clipboard.set_text(result.clone()).map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
                let count = actual_end - actual_start + 1;
                info!("Successfully copied {} logs to clipboard", count);
            }
            Err(e) => {
                error!("Failed to create clipboard: {}", e);
                return Err(format!("Failed to access clipboard: {}", e));
            }
        }

        Ok(result)
    }

    pub fn copy_visible_logs(&self) -> Result<String, String> {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        
        if filtered_logs.is_empty() {
            return Err("No logs available to copy".to_string());
        }

        let mut result = String::new();
        for log in filtered_logs.iter() {
            let formatted_log = self.format_log_for_copy(log);
            result.push_str(&formatted_log);
            result.push('\n');
        }

        // Remove trailing newline
        if result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    // Helper method for consistent log formatting
    fn format_log_for_copy(&self, log: &LogEntry) -> String {
        let timestamp = log.timestamp
            .map(|ts| ts.format("%H:%M:%S%.3f").to_string())
            .unwrap_or_else(|| "??:??:??".to_string());
        
        format!(
            "[{}] {}/{}: {}",
            timestamp, log.pod_name, log.container_name, log.message
        )
    }

    pub fn get_display_logs(&self) -> Vec<LogDisplayEntry> {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        filtered_logs
            .iter()
            .map(|log| LogDisplayEntry {
                timestamp: log.timestamp
                    .map(|ts| ts.format("%H:%M:%S%.3f").to_string())
                    .unwrap_or_else(|| "??:??:??".to_string()),
                pod: log.pod_name.clone(),
                container: log.container_name.clone(),
                message: log.message.clone(),
                raw_entry: log.clone(),
            })
            .collect()
    }

    fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Disable follow mode when manually navigating
            if self.follow_mode {
                self.follow_mode = false;
                self.auto_scroll = false;
            }
        }
    }

    fn move_selection_down(&mut self) {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        if self.selected_index < filtered_logs.len().saturating_sub(1) {
            self.selected_index += 1;
        } else if self.selected_index == filtered_logs.len().saturating_sub(1) {
            // At the bottom - re-enable follow mode if not paused
            if !self.is_paused {
                self.follow_mode = true;
                self.auto_scroll = true;
            }
        }
    }

    fn scroll_to_bottom(&mut self) {
        let filtered_logs = self.filtered_logs.lock().unwrap();
        if !filtered_logs.is_empty() {
            self.selected_index = filtered_logs.len() - 1;
            self.scroll_offset = filtered_logs.len().saturating_sub(20); // Show last 20 lines
        }
    }

    // NEW: Jump to top
    pub fn scroll_to_top(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.follow_mode = false;
        self.auto_scroll = false;
    }
}

impl View for LogView {
    fn draw(&self, printer: &cursive::Printer) {
        let display_logs = self.get_display_logs();

        if display_logs.is_empty() {
            let empty_text = TextView::new("No logs available");
            empty_text.draw(printer);
            return;
        }

        // Create content for the scroll view
        let mut content = String::new();
        for (i, log) in display_logs.iter().enumerate() {
            if i == self.selected_index {
                content.push_str(&format!("> {}\n", log.format_for_copy()));
            } else {
                content.push_str(&format!("  {}\n", log.format_for_copy()));
            }
        }

        let text_view = TextView::new(content);
        let scroll_view = ScrollView::new(text_view);
        scroll_view.draw(printer);
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Key(Key::Up) => {
                self.move_selection_up();
                EventResult::Consumed(None)
            }
            Event::Key(Key::Down) => {
                self.move_selection_down();
                EventResult::Consumed(None)
            }
            Event::Key(Key::End) => {
                self.scroll_to_bottom();
                EventResult::Consumed(None)
            }
            Event::Char('c') => {
                match self.copy_selected_log() {
                    Ok(()) => {
                        // Could show a status message here
                    }
                    Err(e) => {
                        error!("Copy failed: {}", e);
                    }
                }
                EventResult::Consumed(None)
            }
            Event::Char('C') => {
                // Copy all visible logs
                match self.copy_visible_logs() {
                    Ok(logs) => {
                        // Optionally, copy to clipboard directly
                        let _ = Clipboard::new().and_then(|mut clipboard| clipboard.set_text(logs));
                        info!("Copied all visible logs to clipboard");
                    }
                    Err(e) => {
                        error!("Failed to copy logs: {}", e);
                    }
                }
                EventResult::Consumed(None)
            }
            Event::Char('g') => {
                // Go to top
                self.selected_index = 0;
                EventResult::Consumed(None)
            }
            Event::Char('G') => {
                // Go to bottom
                self.scroll_to_bottom();
                EventResult::Consumed(None)
            }
            Event::Char('f') => {
                // Toggle follow mode
                self.toggle_follow_mode();
                EventResult::Consumed(None)
            }
            Event::Char('u') => {
                // Scroll up
                self.scroll_up(1);
                EventResult::Consumed(None)
            }
            Event::Char('d') => {
                // Scroll down
                self.scroll_down(1);
                EventResult::Consumed(None)
            }
            Event::Char('p') => {
                // Toggle pause
                self.set_paused(!self.is_paused());
                EventResult::Consumed(None)
            }
            Event::Char('[') => {
                // Move selection to the left (for multi-selection)
                if let Some(start) = self.selection_start {
                    let new_end = if start > 0 { start - 1 } else { 0 };
                    let filtered_logs = self.filtered_logs.lock().unwrap();
                    drop(filtered_logs); // Release the lock before calling extend_selection
                    self.extend_selection(new_end);
                } else {
                    self.start_selection(self.selected_index);
                }
                EventResult::Consumed(None)
            }
            Event::Char(']') => {
                // Move selection to the right (for multi-selection)
                let max_len = {
                    let filtered_logs = self.filtered_logs.lock().unwrap();
                    filtered_logs.len()
                };
                
                if let Some(end) = self.selection_end {
                    let new_end = if end < max_len.saturating_sub(1) { end + 1 } else { end };
                    self.extend_selection(new_end);
                } else {
                    self.start_selection(self.selected_index);
                }
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored,
        }
    }

    fn required_size(&mut self, constraint: cursive::Vec2) -> cursive::Vec2 {
        constraint
    }
}