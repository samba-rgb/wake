use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::k8s::logs::LogEntry;
use crate::ui::input::{InputHandler, InputMode};
use std::collections::{VecDeque, HashMap};
use std::sync::OnceLock;
use regex::Regex;

/// Global regex instance for ANSI stripping - compiled once
static ANSI_REGEX: OnceLock<Regex> = OnceLock::new();

/// Color scheme that adapts to terminal background
#[derive(Debug, Clone, Copy)]
pub enum ColorScheme {
    Dark,   // For dark terminal backgrounds
    Light,  // For light terminal backgrounds
}

impl ColorScheme {
    /// Detect terminal background color scheme
    /// This is a heuristic since there's no reliable way to detect terminal background
    pub fn detect() -> Self {
        // Check environment variables that might indicate light theme
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            // Some terminals set this when using light themes
            if term_program.contains("light") {
                return ColorScheme::Light;
            }
        }
        
        // Check for VS Code integrated terminal (often light)
        if std::env::var("VSCODE_INJECTION").is_ok() {
            return ColorScheme::Light;
        }
        
        // Check COLORFGBG environment variable (some terminals set this)
        if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
            // Format is usually "foreground;background"
            if let Some(bg) = colorfgbg.split(';').nth(1) {
                if let Ok(bg_num) = bg.parse::<i32>() {
                    // Light colors (white-ish) have higher numbers
                    if bg_num >= 7 {
                        return ColorScheme::Light;
                    }
                }
            }
        }
        
        // Default to dark theme (most terminals)
        ColorScheme::Dark
    }
    
    /// Get text color that's visible on this background
    pub fn text_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::White,
            ColorScheme::Light => Color::Black,
        }
    }
    
    /// Get dim text color
    pub fn dim_text_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::DarkGray,
            ColorScheme::Light => Color::Gray,
        }
    }
    
    /// Get default message color for unknown log levels
    pub fn default_message_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::White,
            ColorScheme::Light => Color::Black,
        }
    }
    
    /// Get colors that work well on this background
    pub fn pod_colors(self) -> &'static [Color] {
        match self {
            ColorScheme::Dark => &[
                Color::Cyan, Color::Green, Color::Yellow, Color::Blue,
                Color::Magenta, Color::LightCyan, Color::LightGreen, Color::LightYellow,
                Color::LightBlue, Color::LightMagenta
            ],
            ColorScheme::Light => &[
                Color::Blue, Color::Red, Color::Green, Color::Magenta,
                Color::Cyan, Color::DarkGray, Color::LightRed, Color::LightGreen,
                Color::LightBlue, Color::LightMagenta
            ],
        }
    }
    
    /// Get container colors that work well on this background
    pub fn container_colors(self) -> &'static [Color] {
        match self {
            ColorScheme::Dark => &[
                Color::LightCyan, Color::LightGreen, Color::LightYellow,
                Color::LightBlue, Color::LightMagenta, Color::Cyan,
                Color::Green, Color::Yellow, Color::Blue, Color::Magenta
            ],
            ColorScheme::Light => &[
                Color::Blue, Color::Red, Color::Green, Color::Magenta,
                Color::Cyan, Color::DarkGray, Color::LightRed, Color::LightGreen,
                Color::LightBlue, Color::LightMagenta
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub start_line: usize,
    pub end_line: usize,
    pub is_active: bool,
    pub is_dragging: bool,
}

impl Selection {
    pub fn new(line: usize) -> Self {
        Self {
            start_line: line,
            end_line: line,
            is_active: true,
            is_dragging: false,
        }
    }

    pub fn extend_to(&mut self, line: usize) {
        if line < self.start_line {
            self.end_line = self.start_line;
            self.start_line = line;
        } else {
            self.end_line = line;
        }
    }

    #[allow(dead_code)]
    pub fn contains(&self, line: usize) -> bool {
        // Only include a line if selection is active and the line is actually within the selection range
        self.is_active && line >= self.start_line && line <= self.end_line
    }

    pub fn clear(&mut self) {
        self.is_active = false;
        self.is_dragging = false;
    }

    pub fn start_drag(&mut self) {
        self.is_dragging = true;
    }

    pub fn end_drag(&mut self) {
        self.is_dragging = false;
    }
}

pub struct DisplayManager {
    pub log_entries: VecDeque<LogEntry>,
    pub scroll_offset: usize,
    pub max_lines: usize,
    pub original_max_lines: usize, // Store original buffer size
    pub filtered_logs: usize,  // Only count logs in buffer (all are filtered)
    pub show_timestamps: bool,
    pub auto_scroll: bool,
    pub selection: Option<Selection>,
    pub selection_cursor: usize,
    pub dev_mode: bool, // Add dev mode flag
    // Performance optimizations
    cache_generation: usize,
    // Pre-computed colors for pods/containers
    pod_color_cache: HashMap<String, Color>,
    container_color_cache: HashMap<String, Color>,
    // Color scheme for adaptive colors
    color_scheme: ColorScheme,
}

impl DisplayManager {
    pub fn new(max_lines: usize, show_timestamps: bool, dev_mode: bool) -> anyhow::Result<Self> {
        let actual_max_lines = max_lines;
        let color_scheme = ColorScheme::detect();
        
        // Log detected color scheme
        tracing::info!("Detected terminal color scheme: {:?}", color_scheme);
        
        Ok(Self {
            log_entries: VecDeque::with_capacity(actual_max_lines),
            scroll_offset: 0,
            max_lines: actual_max_lines,
            original_max_lines: actual_max_lines, // Store original buffer size for selection mode
            filtered_logs: 0,
            show_timestamps,
            auto_scroll: true, // Default to auto-scroll enabled
            cache_generation: 0,
            pod_color_cache: HashMap::new(),
            container_color_cache: HashMap::new(),
            selection: None,
            selection_cursor: 0,
            dev_mode: dev_mode, // Set dev mode from parameter
            color_scheme,
        })
    }

    pub fn add_log_entry(&mut self, entry: &LogEntry) {
        // Check if we're in selection mode (either active selection OR selection mode cursor visible)
        let is_in_selection_mode = self.selection.is_some();
        
        // **SAFETY MECHANISM**: Force exit selection mode if buffer exceeds 2x original size
        if is_in_selection_mode && self.log_entries.len() >= (self.original_max_lines * 2) {
            self.add_system_log("🚨 Buffer reached 2x limit - Force exiting selection mode");
            self.selection = None;
            self.selection_cursor = 0;
            self.auto_scroll = true; // Enable auto-scroll for follow mode
            self.exit_selection_mode(); // This will trim buffer back to original size
            self.add_system_log("📉 Returned to normal mode with original buffer size and auto-follow enabled");
        }
        
        // Re-check selection mode status after potential forced exit
        let is_in_selection_mode = self.selection.is_some();
        
        // **CRITICAL FIX**: In selection mode, NEVER remove entries to prevent selection corruption
        // In normal mode, apply standard buffer rotation at configured max_lines
        if is_in_selection_mode {
            // Selection mode: no buffer rotation, just keep adding (until 2x safety limit)
            if self.log_entries.len() % 1000 == 0 && self.log_entries.len() > self.max_lines {
                self.add_system_log(&format!("🛡️ Selection mode: {} lines retained (no rotation)", 
                    self.log_entries.len()));
            }
        } else {
            // Normal mode: apply buffer rotation when max_lines reached
            if self.log_entries.len() >= self.original_max_lines {
                self.log_entries.pop_front();
                
                // When removing entries from front, adjust scroll offset to maintain position
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
        }
        
        self.log_entries.push_back(entry.clone());
        self.filtered_logs += 1;
        
        // If auto-scroll is enabled, keep scroll at bottom
        if self.auto_scroll {
            // Calculate viewport height conservatively (will be corrected during render)
            let estimated_viewport = 20; // Conservative estimate
            let max_scroll = self.log_entries.len().saturating_sub(estimated_viewport);
            self.scroll_offset = max_scroll;
        }

        // Invalidate cache on new log entry
        self.cache_generation += 1;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false; // Disable auto-scroll on manual scroll
        
        // Validate scroll offset bounds
        self.validate_scroll_bounds(50); // Use conservative viewport estimate
    }

    pub fn scroll_down(&mut self, lines: usize, viewport_height: usize) {
        // Enhanced scrolling logic - get total logs count as maximum scroll value
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        
        // Make sure we don't scroll beyond the maximum
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
        
        // Auto-enable auto-scroll when user manually scrolls to the bottom
        // This matches the behavior of most terminal applications
        if self.scroll_offset >= max_scroll {
            self.auto_scroll = true;
        } else {
            self.auto_scroll = false;
        }
        
        // Validate scroll offset bounds
        self.validate_scroll_bounds(viewport_height);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = false; // Disable auto-scroll on manual scroll
    }

    pub fn scroll_to_bottom(&mut self, viewport_height: usize) {
        // Fix: Calculate scroll offset based on actual log lines, not display lines
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        self.scroll_offset = max_scroll;
        // Auto-enable auto-scroll when going to bottom
        self.auto_scroll = true;
    }

    /// Validate and fix scroll bounds to prevent corruption
    fn validate_scroll_bounds(&mut self, viewport_height: usize) {
        let max_possible_scroll = if self.log_entries.len() > viewport_height {
            self.log_entries.len() - viewport_height
        } else {
            0
        };
        
        if self.scroll_offset > max_possible_scroll {
            self.scroll_offset = max_possible_scroll;
        }
    }

    /// Calculate the total number of display lines including wrapped lines
    #[allow(dead_code)]
    pub fn calculate_total_display_lines(&self, viewport_width: usize) -> usize {
        self.log_entries.iter().map(|entry| {
            let line = self.format_log_for_ui(entry);
            if line.starts_with("🔧") {
                1 // System messages are always single line
            } else {
                wrap_line(&line, viewport_width).len()
            }
        }).sum()
    }

    pub fn render(&mut self, f: &mut Frame, input_handler: &InputHandler) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Filter input area (3 lines for borders + content)
                Constraint::Min(3),    // Log display area (minimum 3 lines)
                Constraint::Length(1), // Status bar (1 line)
            ])
            .split(f.size());

        // Render filter input area
        self.render_filter_area(f, chunks[0], input_handler);

        // Render log display area
        self.render_log_area(f, chunks[1], input_handler);

        // Render status bar
        self.render_status_bar(f, chunks[2], input_handler);

        // Render help popup if in help mode
        if input_handler.mode == InputMode::Help {
            self.render_help_popup(f, input_handler);
        }
    }

    fn render_filter_area(&self, f: &mut Frame, area: Rect, input_handler: &InputHandler) {
        let filter_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Include filter
        let include_style = if input_handler.mode == InputMode::EditingInclude {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let include_block = Block::default()
            .title("Include Pattern (i to edit)")
            .borders(Borders::ALL)
            .border_style(include_style);

        let include_text = if input_handler.include_input.is_empty() {
            Text::from(Span::styled("(none)", Style::default().fg(Color::DarkGray)))
        } else {
            Text::from(input_handler.include_input.as_str())
        };

        let include_paragraph = Paragraph::new(include_text)
            .block(include_block)
            .wrap(Wrap { trim: true });

        f.render_widget(include_paragraph, filter_chunks[0]);

        // Show cursor for include input
        if input_handler.mode == InputMode::EditingInclude {
            let cursor_x = filter_chunks[0].x + 1 + input_handler.cursor_position as u16;
            let cursor_y = filter_chunks[0].y + 1;
            if cursor_x < filter_chunks[0].x + filter_chunks[0].width - 1 {
                f.set_cursor(cursor_x, cursor_y);
            }
        }

        // Exclude filter
        let exclude_style = if input_handler.mode == InputMode::EditingExclude {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };

        let exclude_block = Block::default()
            .title("Exclude Pattern (e to edit)")
            .borders(Borders::ALL)
            .border_style(exclude_style);

        let exclude_text = if input_handler.exclude_input.is_empty() {
            Text::from(Span::styled("(none)", Style::default().fg(Color::DarkGray)))
        } else {
            Text::from(input_handler.exclude_input.as_str())
        };

        let exclude_paragraph = Paragraph::new(exclude_text)
            .block(exclude_block)
            .wrap(Wrap { trim: true });

        f.render_widget(exclude_paragraph, filter_chunks[1]);

        // Show cursor for exclude input
        if input_handler.mode == InputMode::EditingExclude {
            let cursor_x = filter_chunks[1].x + 1 + input_handler.cursor_position as u16;
            let cursor_y = filter_chunks[1].y + 1;
            if cursor_x < filter_chunks[1].x + filter_chunks[1].width - 1 {
                f.set_cursor(cursor_x, cursor_y);
            }
        }
    }

    fn render_log_area(&mut self, f: &mut Frame, area: Rect, input_handler: &InputHandler) {
        let viewport_height = area.height.saturating_sub(2) as usize; // Account for borders
        
        // Validate and fix scroll offset before rendering to prevent out-of-bounds access
        if self.scroll_offset >= self.log_entries.len() && !self.log_entries.is_empty() {
            // If scroll offset is invalid, reset to bottom and update the actual field
            self.scroll_offset = if self.log_entries.len() > viewport_height {
                self.log_entries.len() - viewport_height
            } else {
                0
            };
        }
        
        // Additional safety check for edge cases
        let max_safe_offset = if self.log_entries.len() > viewport_height {
            self.log_entries.len() - viewport_height
        } else {
            0
        };
        
        if self.scroll_offset > max_safe_offset {
            self.scroll_offset = max_safe_offset;
        }

        // Get the visible log entries with proper bounds checking
        let visible_entries: Vec<LogEntry> = self.log_entries
            .iter()
            .skip(self.scroll_offset)
            .take(viewport_height)
            .cloned()
            .collect();
        
        // Add debugging for selection state
        if let Some(ref selection) = self.selection {
            if selection.is_active {
                tracing::debug!("Active selection: start_line={}, end_line={}, scroll_offset={}, selection_cursor={}", 
                    selection.start_line, selection.end_line, self.scroll_offset, self.selection_cursor);
            }
        }
        
        // Create colored lines from log entries with selection highlighting
        let colored_lines: Vec<Line> = if visible_entries.is_empty() {
            vec![Line::from(Span::styled(
                "No logs to display",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
            ))]
        } else {
            visible_entries.iter().enumerate().map(|(display_idx, entry)| {
                let log_idx = self.scroll_offset + display_idx;
                let mut line = self.create_colored_log_line(entry);
                
                // Determine if this line should be highlighted based on selection state
                let mut is_selected = false;
                let mut is_cursor_line = false;
                
                // Check if line is within active selection range
                if let Some(ref selection) = self.selection {
                    if selection.is_active && log_idx >= selection.start_line && log_idx <= selection.end_line {
                        is_selected = true;
                    }
                }
                
                // Check if this is the current cursor line in selection mode
                if input_handler.mode == InputMode::Selection {
                    // Calculate the absolute line index for the current cursor position
                    let cursor_absolute_line = self.scroll_offset + self.selection_cursor;
                    if log_idx == cursor_absolute_line {
                        is_cursor_line = true;
                    }
                }
                
                // Apply highlighting with proper priority:
                // 1. Cursor line gets reversed colors (highest priority)
                // 2. Selected lines get white background
                // 3. Normal lines get default styling
                
                if is_cursor_line {
                    // Cursor line: apply reversed styling for visibility
                    let cursor_spans: Vec<Span> = line.spans.into_iter().map(|span| {
                        Span::styled(
                            span.content,
                            span.style.add_modifier(Modifier::REVERSED)
                        )
                    }).collect();
                    line = Line::from(cursor_spans);
                } else if is_selected {
                    // Selected line: apply white background with black text
                    let highlighted_spans: Vec<Span> = line.spans.into_iter().map(|span| {
                        Span::styled(
                            span.content,
                            span.style.bg(Color::White).fg(Color::Black)
                        )
                    }).collect();
                    line = Line::from(highlighted_spans);
                }
                
                line
            }).collect()
        };

        let title = if input_handler.mode == InputMode::Selection {
            if let Some(ref selection) = self.selection {
                if selection.is_active {
                    let lines_selected = selection.end_line - selection.start_line + 1;
                    format!(" Logs ({}) - Selection: {} lines ", self.filtered_logs, lines_selected)
                } else {
                    format!(" Logs ({}) - Selection Mode ", self.filtered_logs)
                }
            } else {
                format!(" Logs ({}) - Selection Mode ", self.filtered_logs)
            }
        } else {
            format!(" Logs ({}) ", self.filtered_logs)
        };

        let border_color = if input_handler.mode == InputMode::Selection {
            Color::Yellow
        } else {
            Color::Blue
        };

        let logs_paragraph = Paragraph::new(colored_lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
            )
            .wrap(Wrap { trim: false });

        f.render_widget(logs_paragraph, area);
    }

    /// Get a color for a pod name based on its hash
    fn get_pod_color(&mut self, pod_name: &str) -> Color {
        // Check cache first
        if let Some(&color) = self.pod_color_cache.get(pod_name) {
            return color;
        }

        let colors = self.color_scheme.pod_colors();
        let hash = pod_name.chars().map(|c| c as usize).sum::<usize>();
        let color = colors[hash % colors.len()];

        // Cache the computed color
        self.pod_color_cache.insert(pod_name.to_string(), color);

        color
    }

    /// Get a color for a container name based on its hash
    fn get_container_color(&mut self, container_name: &str) -> Color {
        // Check cache first
        if let Some(&color) = self.container_color_cache.get(container_name) {
            return color;
        }

        let colors = self.color_scheme.container_colors();
        let hash = container_name.chars().map(|c| c as usize).sum::<usize>();
        let color = colors[hash % colors.len()];

        // Cache the computed color
        self.container_color_cache.insert(container_name.to_string(), color);

        color
    }

    /// Create a colored line from a log entry
    fn create_colored_log_line(&mut self, entry: &LogEntry) -> Line<'static> {
        let mut spans = Vec::new();

        // System messages (filter notifications) get special treatment
        if entry.message.starts_with("🔧") {
            return Line::from(Span::styled(
                entry.message.clone(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::ITALIC)
            ));
        }

        // Clean fields of ANSI codes
        let clean_message = Self::strip_ansi_codes(&entry.message);
        let clean_pod_name = Self::strip_ansi_codes(&entry.pod_name);
        let clean_container_name = Self::strip_ansi_codes(&entry.container_name);

        // Add timestamp if enabled
        if self.show_timestamps {
            if let Some(ts) = entry.timestamp {
                spans.push(Span::styled(
                    format!("{} ", ts.format("%H:%M:%S")),
                    Style::default().fg(self.color_scheme.dim_text_color())
                ));
            }
        }

        // Add pod name with color based on hash
        let pod_color = self.get_pod_color(&clean_pod_name);
        spans.push(Span::styled(
            clean_pod_name.clone(),
            Style::default().fg(pod_color).add_modifier(Modifier::BOLD)
        ));

        // Add separator
        spans.push(Span::styled("/".to_string(), Style::default().fg(self.color_scheme.dim_text_color())));

        // Add container name with different color
        let container_color = self.get_container_color(&clean_container_name);
        spans.push(Span::styled(
            clean_container_name.clone(),
            Style::default().fg(container_color)
        ));

        // Add separator
        spans.push(Span::styled(" ".to_string(), Style::default()));

        // Parse and color the log message based on log level
        let colored_message_spans = self.parse_log_message(&clean_message);
        spans.extend(colored_message_spans);

        Line::from(spans)
    }

    /// Parse log message and apply colors based on log level and content
    fn parse_log_message(&self, message: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        
        // Fast log level detection using string search instead of regex
        let level_start = message.find('[');
        let level_end = message.find(']');
        
        if let (Some(start), Some(end)) = (level_start, level_end) {
            if start < end && end < message.len() {
                let level = &message[start+1..end];
                let before = &message[..start];
                let after = &message[end+1..];
                
                // Only parse common log levels for performance
                if matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "WARNING" | "ERROR" | "FATAL") {
                    // Add text before log level
                    if !before.is_empty() {
                        spans.push(Span::raw(before.to_string()));
                    }
                    
                    // Add colored log level
                    let level_color = match level {
                        "TRACE" => self.color_scheme.dim_text_color(),
                        "DEBUG" => Color::Blue,
                        "INFO" => Color::Green,
                        "WARN" | "WARNING" => Color::Yellow,
                        "ERROR" => Color::Red,
                        "FATAL" => Color::LightRed,
                        _ => self.color_scheme.text_color(),
                    };
                    
                    spans.push(Span::styled(
                        format!("[{}]", level),
                        Style::default()
                            .fg(level_color)
                            .add_modifier(Modifier::BOLD)
                    ));
                    
                    // Add text after log level with appropriate coloring
                    if !after.is_empty() {
                        let after_spans = self.color_message_content_fast(after, level);
                        spans.extend(after_spans);
                    }
                    return spans;
                }
            }
        }
        
        // No log level detected, apply general coloring
        let content_spans = self.color_message_content_fast(message, "INFO");
        spans.extend(content_spans);
        spans
    }

    /// Fast message content coloring with reduced allocations
    fn color_message_content_fast(&self, content: &str, log_level: &str) -> Vec<Span<'static>> {
        // Base color depends on log level and adapts to background
        let base_color = match log_level {
            "ERROR" | "FATAL" => Color::LightRed,
            "WARN" | "WARNING" => Color::LightYellow,
            "DEBUG" => Color::LightBlue,
            "TRACE" => self.color_scheme.dim_text_color(),
            _ => self.color_scheme.default_message_color(),
        };
        
        // For performance, only do basic coloring on scrolling
        // Check for common error/success patterns quickly
        if content.contains("error") || content.contains("Error") || content.contains("fail") {
            vec![Span::styled(content.to_string(), Style::default().fg(Color::Red))]
        } else if content.contains("success") || content.contains("Success") || content.contains("ok") {
            vec![Span::styled(content.to_string(), Style::default().fg(Color::Green))]
        } else if content.contains("warn") || content.contains("Warn") {
            vec![Span::styled(content.to_string(), Style::default().fg(Color::Yellow))]
        } else {
            vec![Span::styled(content.to_string(), Style::default().fg(base_color))]
        }
    }

    /// Strip ANSI escape codes from a string to ensure clean UI display
    fn strip_ansi_codes(text: &str) -> String {
        // Use the global regex instance for ANSI stripping
        let ansi_regex = ANSI_REGEX.get_or_init(|| Regex::new(r"(\x1b\[[0-9;]*[a-zA-Z]|\x1b\[[0-9;]*m|\[[0-9;]*m)").unwrap());
        ansi_regex.replace_all(text, "").to_string()
    }

    pub fn add_system_message(&mut self, message: &str) {
        let system_entry = LogEntry {
            namespace: "system".to_string(),
            pod_name: "wake".to_string(),
            container_name: "filter".to_string(),
            message: format!("🔧 {}", message),
            timestamp: Some(chrono::Utc::now()),
        };
        
        // Remove oldest entries if we exceed max_lines
        if self.log_entries.len() >= self.max_lines {
            self.log_entries.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(system_entry);
        // Don't increment filtered_logs for system messages
    }

    /// Add a debug system message that only appears when dev mode is enabled
    pub fn add_system_log(&mut self, message: &str) {
        if self.dev_mode {
            self.add_system_message(message);
        }
    }

    fn render_status_bar(&self, f: &mut Frame, area: Rect, input_handler: &InputHandler) {
        let mode_text = match input_handler.mode {
            InputMode::Normal => "NORMAL",
            InputMode::EditingInclude => "EDIT INCLUDE",
            InputMode::EditingExclude => "EDIT EXCLUDE", 
            InputMode::Help => "HELP",
            InputMode::Selection => "SELECTION",
        };

        // Auto-scroll indicator
        let auto_scroll_indicator = if self.auto_scroll {
            Span::styled(
                " FOLLOW ",
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            )
        } else {
            Span::styled(
                " PAUSED ",
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            )
        };

        let mut status_spans = vec![
            Span::styled(
                format!(" {} ", mode_text),
                Style::default()
                    .bg(if input_handler.mode == InputMode::Selection { Color::Yellow } else { Color::Blue })
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            ),
            Span::raw(" "),
            auto_scroll_indicator,
            Span::raw(" "),
            Span::styled(
                format!("Lines: {}", self.filtered_logs),
                Style::default().fg(self.color_scheme.text_color())
            ),
            Span::raw(" | "),
            Span::styled(
                format!("Scroll: {}/{}", self.scroll_offset, self.log_entries.len()),
                Style::default().fg(self.color_scheme.text_color())
            ),
            Span::raw(" | "),
        ];

        // Add different help text based on mode
        let help_text = match input_handler.mode {
            InputMode::Selection => "s:Exit-Selection x:Toggle-Selection Ctrl+c:Copy-Selection ↑↓:Navigate",
            _ => "s:Selection f:Toggle-Follow h:Help q:Quit ↑↓:Scroll i:Include e:Exclude Ctrl+c:Copy",
        };

        status_spans.push(Span::styled(
            help_text,
            Style::default().fg(self.color_scheme.dim_text_color())
        ));

        let status_line = Line::from(status_spans);
        let status_paragraph = Paragraph::new(status_line)
            .style(Style::default().bg(Color::Black));

        f.render_widget(status_paragraph, area);
    }

    fn render_help_popup(&self, f: &mut Frame, input_handler: &InputHandler) {
        let area = f.size();
        let popup_area = centered_rect(80, 70, area);

        // Clear the area
        f.render_widget(Clear, popup_area);

        let help_text = input_handler.get_help_text();
        let help_lines: Vec<Line> = help_text
            .into_iter()
            .map(|line| Line::from(line))
            .collect();

        let help_paragraph = Paragraph::new(help_lines)
            .block(
                Block::default()
                    .title(" Help - Press h or Esc to close ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .style(Style::default().bg(Color::Black))
            )
            .wrap(Wrap { trim: true });

        f.render_widget(help_paragraph, popup_area);
    }

    /// Format a log entry specifically for UI display (used for wrapping calculations)
    fn format_log_for_ui(&self, entry: &LogEntry) -> String {
        let time_part = if self.show_timestamps {
            if let Some(ts) = entry.timestamp {
                format!("{} ", ts.format("%H:%M:%S"))
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let clean_message = Self::strip_ansi_codes(&entry.message);
        let clean_pod_name = Self::strip_ansi_codes(&entry.pod_name);
        let clean_container_name = Self::strip_ansi_codes(&entry.container_name);

        format!("{}{}/{} {}", 
            time_part,
            clean_pod_name,
            clean_container_name,
            clean_message
        )
    }

    /// Get the currently visible logs as plain text for clipboard copying
    pub fn get_visible_logs_as_text(&self, viewport_height: usize) -> String {
        let visible_entries: Vec<&LogEntry> = self.log_entries
            .iter()
            .skip(self.scroll_offset)
            .take(viewport_height)
            .collect();

        if visible_entries.is_empty() {
            return "No logs to copy".to_string();
        }

        let mut result = String::new();
        for entry in visible_entries {
            let formatted_line = self.format_log_for_ui(entry);
            result.push_str(&formatted_line);
            result.push('\n');
        }

        // Remove the last newline to avoid extra blank line
        if result.ends_with('\n') {
            result.pop();
        }

        result
    }

    /// Get all logs as plain text for clipboard copying
    #[allow(dead_code)]
    pub fn get_all_logs_as_text(&self) -> String {
        if self.log_entries.is_empty() {
            return "No logs to copy".to_string();
        }

        let mut result = String::new();
        for entry in &self.log_entries {
            let formatted_line = self.format_log_for_ui(entry);
            result.push_str(&formatted_line);
            result.push('\n');
        }

        // Remove the last newline to avoid extra blank line
        if result.ends_with('\n') {
            result.pop();
        }

        result
    }

    /// Start or toggle selection at the current cursor position
    pub fn toggle_selection(&mut self) {
        let current_line = self.scroll_offset + self.selection_cursor;
        
        if let Some(ref mut selection) = self.selection {
            if selection.is_active {
                // Clear existing selection and restore auto-scroll
                selection.clear();
                self.selection = None;
                self.auto_scroll = true; // Re-enable follow mode when selection is cleared
                tracing::debug!("Selection cleared and auto-scroll restored");
            } else {
                // Reactivate selection and pause auto-scroll
                *selection = Selection::new(current_line);
                self.auto_scroll = false; // Pause auto-scroll when selection becomes active
                tracing::debug!("Selection reactivated at line {} and auto-scroll paused", current_line);
            }
        } else {
            // Start new selection and pause auto-scroll
            self.selection = Some(Selection::new(current_line));
            self.auto_scroll = false; // Pause auto-scroll when starting new selection
            tracing::debug!("New selection started at line {} and auto-scroll paused", current_line);
        }
    }

    /// Move selection cursor up
    pub fn select_up(&mut self) {
        if self.selection_cursor > 0 {
            self.selection_cursor -= 1;
        }
        
        // Extend selection if active
        if let Some(ref mut selection) = self.selection {
            if selection.is_active {
                let current_line = self.scroll_offset + self.selection_cursor;
                selection.extend_to(current_line);
            }
        }
    }

    /// Move selection cursor down
    pub fn select_down(&mut self, viewport_height: usize) {
        let max_cursor = viewport_height.saturating_sub(1);
        if self.selection_cursor < max_cursor && self.selection_cursor < self.log_entries.len().saturating_sub(1) {
            self.selection_cursor += 1;
        }
        
        // Extend selection if active
        if let Some(ref mut selection) = self.selection {
            if selection.is_active {
                let current_line = self.scroll_offset + self.selection_cursor;
                selection.extend_to(current_line);
            }
        }
    }

    /// Clear current selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_cursor = 0;
        self.auto_scroll = true; // Re-enable follow mode when selection is cleared
    }

    /// Get selected logs as text for clipboard copying
    pub fn get_selected_logs_as_text(&self) -> String {
        if let Some(ref selection) = self.selection {
            if !selection.is_active {
                return "No selection active".to_string();
            }

            let mut result = String::new();
            let start_idx = selection.start_line.min(self.log_entries.len());
            let end_idx = (selection.end_line + 1).min(self.log_entries.len());
            
            for (idx, entry) in self.log_entries.iter().enumerate() {
                if idx >= start_idx && idx < end_idx {
                    let formatted_line = self.format_log_for_ui(entry);
                    result.push_str(&formatted_line);
                    result.push('\n');
                }
            }

            // Remove the last newline to avoid extra blank line
            if result.ends_with('\n') {
                result.pop();
            }

            if result.is_empty() {
                "No logs in selection range".to_string()
            } else {
                result
            }
        } else {
            "No selection active".to_string()
        }
    }

    /// Handle mouse click for selection
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Debug messages using the new add_system_log method
        self.add_system_log(&format!("🖱️ Click at ({}, {}) - log_area: x={}, y={}, w={}, h={}", 
            x, y, log_area.x, log_area.y, log_area.width, log_area.height));
        
        // Check if click is within the log area content (excluding borders and title)
        if x >= log_area.x + 1 && x < log_area.x + log_area.width - 1 &&
           y >= log_area.y + 2 && y < log_area.y + log_area.height - 1 {
            
            // Calculate which log line was clicked
            let relative_y = y.saturating_sub(log_area.y + 2) as usize;
            let clicked_line = self.scroll_offset.saturating_add(relative_y);
            
            self.add_system_log(&format!("📊 Calc: y={}, log_y={}, start={}, rel_y={}, scroll={}, line={}", 
                y, log_area.y, log_area.y + 2, relative_y, self.scroll_offset, clicked_line));
            
            // IMPORTANT: When logs are coming fast, pause auto-scroll immediately to stabilize the view
            let was_auto_scrolling = self.auto_scroll;
            self.auto_scroll = false;
            
            // Ensure we don't go beyond available logs and relative_y is in visible range
            if !self.log_entries.is_empty() && 
               relative_y < (log_area.height as usize).saturating_sub(3) && 
               clicked_line < self.log_entries.len() {
                
                self.add_system_log(&format!("✅ Valid click - selecting line {} (auto_scroll was {})", 
                    clicked_line, was_auto_scrolling));
                
                // Clear any existing selection first to prevent conflicts
                self.selection = None;
                self.selection_cursor = 0;
                
                // Create new selection at the clicked line
                self.selection = Some(Selection::new(clicked_line));
                
                // Set selection cursor to the relative viewport position
                self.selection_cursor = relative_y;
                
                self.add_system_log(&format!("🎯 Selection cursor set to relative_y={} for clicked_line={}", 
                    relative_y, clicked_line));
                
                if let Some(ref mut selection) = self.selection {
                    selection.start_drag();
                    self.add_system_log(&format!("🖱️ Mouse drag started for line {}", clicked_line));
                }
                return true;
            } else {
                self.add_system_log(&format!("❌ Invalid: entries={}, rel_y={}, max_rel_y={}, clicked_line={}", 
                    self.log_entries.len(), relative_y, log_area.height.saturating_sub(3), clicked_line));
                // Restore auto-scroll if click was invalid
                self.auto_scroll = was_auto_scrolling;
            }
        } else {
            self.add_system_log("❌ Click outside log area bounds");
        }
        false
    }
    
    /// Handle mouse drag for selection extension
    pub fn handle_mouse_drag(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Check if drag is within the log area and we have an active selection
        if let Some(ref mut selection) = self.selection {
            if selection.is_dragging && 
               x >= log_area.x + 1 && x < log_area.x + log_area.width - 1 &&
               y >= log_area.y + 2 && y < log_area.y + log_area.height - 1 {
                
                // Calculate which log line is being dragged to
                let content_start_y = log_area.y + 2;
                let relative_y = (y - content_start_y) as usize;
                let drag_line = self.scroll_offset + relative_y;
                
                // Ensure we don't go beyond available logs and viewport bounds
                let viewport_height = (log_area.height as usize).saturating_sub(3);
                if drag_line < self.log_entries.len() && relative_y < viewport_height {
                    let old_start = selection.start_line;
                    let old_end = selection.end_line;
                    selection.extend_to(drag_line);
                    
                    // Store values for logging before the borrow ends
                    let new_start = selection.start_line;
                    let new_end = selection.end_line;
                    
                    // Log selection changes during dragging
                    tracing::info!("Selection extended: {}..{} → {}..{} (drag_line={}, scroll_offset={})",
                        old_start, old_end, new_start, new_end, 
                        drag_line, self.scroll_offset);
                    
                    // Add debug logging after the selection borrow ends
                    let _ = selection; // Explicitly drop the mutable borrow
                    self.add_system_log(&format!("🔄 Drag: y={}, rel_y={}, drag_line={}, selection={}..{}", 
                        y, relative_y, drag_line, new_start, new_end));
                    
                    return true;
                }
            }
        }
        false
    }

    /// Handle mouse release to end selection
    pub fn handle_mouse_release(&mut self) -> bool {
        if let Some(ref mut selection) = self.selection {
            if selection.is_dragging {
                selection.end_drag();
                return true;
            }
        }
        false
    }

    /// Enter selection mode - double the buffer size to prevent rotation
    pub fn enter_selection_mode(&mut self) {
        if self.max_lines == self.original_max_lines {
            self.max_lines = self.original_max_lines * 2; // 100 -> 200
            // Expand the VecDeque capacity to match
            self.log_entries.reserve(self.original_max_lines);
            self.add_system_log(&format!("📈 Buffer expanded from {} to {} lines for selection mode", 
                self.original_max_lines, self.max_lines));
        }
    }

    /// Exit selection mode - restore original buffer size and trim if necessary
    pub fn exit_selection_mode(&mut self) {
        if self.max_lines > self.original_max_lines {
            self.max_lines = self.original_max_lines; // Back to original size
            
            // Before trimming, check if we have excessive entries and warn user
            let entries_to_remove = self.log_entries.len().saturating_sub(self.max_lines);
            if entries_to_remove > 0 {
                self.add_system_log(&format!("📉 Trimming {} excess entries when exiting selection mode", 
                    entries_to_remove));
            }
            
            // Trim excess entries if we have more than the original buffer size
            while self.log_entries.len() > self.max_lines {
                self.log_entries.pop_front();
                
                // Adjust scroll offset when removing entries from front
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
            
            // Shrink the VecDeque capacity back to original size
            self.log_entries.shrink_to_fit();
            
            self.add_system_log(&format!("📉 Buffer restored to {} lines, selection mode ended", 
                self.max_lines));
        }
        
        // Always restore auto-scroll when exiting selection mode
        self.auto_scroll = true;
    }

    /// Clear all buffers for performance optimization during shutdown
    pub fn clear_all_buffers(&mut self) {
        let buffer_size_before = self.log_entries.len();
        let cache_entries_before = self.pod_color_cache.len() + self.container_color_cache.len();
        
        // Clear main log buffer
        self.log_entries.clear();
        self.log_entries.shrink_to_fit();
        
        // Clear color caches
        self.pod_color_cache.clear();
        self.container_color_cache.clear();
        
        // Clear selection state
        self.selection = None;
        self.selection_cursor = 0;
        
        // Reset counters
        self.filtered_logs = 0;
        self.scroll_offset = 0;
        self.cache_generation = 0;
        
        // Log cleanup for performance monitoring
        tracing::info!("Buffer cleanup completed: {} log entries cleared, {} cache entries cleared", 
                      buffer_size_before, cache_entries_before);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Wraps a log line into multiple lines if it exceeds the terminal width.
/// Continuation lines are prefixed with spaces for indentation.
#[allow(dead_code)]
fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
    let mut wrapped_lines = Vec::new();
    let mut current_line = String::new();

    for word in line.split(' ') {
        // Check if adding this word would exceed the max width
        if current_line.len() + word.len() + 1 > max_width {
            // Push the current line to wrapped lines and start a new line
            wrapped_lines.push(current_line);
            current_line = String::new();
        } else if !current_line.is_empty() {
            // Add a space before the next word if the line is not empty
            current_line.push(' ');
        }

        // Add the word to the current line
        current_line.push_str(word);
    }

    // Don't forget to add the last line
    if !current_line.is_empty() {
        wrapped_lines.push(current_line);
    }

    wrapped_lines
}