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

pub struct DisplayManager {
    pub log_entries: VecDeque<LogEntry>,
    pub scroll_offset: usize,
    pub max_lines: usize,
    pub total_logs: usize,
    pub filtered_logs: usize,
    pub show_timestamps: bool,
    pub auto_scroll: bool,
    // Performance optimizations
    rendered_lines_cache: HashMap<usize, Line<'static>>,
    cache_generation: usize,
    // Pre-computed colors for pods/containers
    pod_color_cache: HashMap<String, Color>,
    container_color_cache: HashMap<String, Color>,
}

impl DisplayManager {
    pub fn new(max_lines: usize, show_timestamps: bool) -> anyhow::Result<Self> {
        Ok(Self {
            log_entries: VecDeque::with_capacity(max_lines),
            scroll_offset: 0,
            max_lines,
            total_logs: 0,
            filtered_logs: 0,
            show_timestamps,
            auto_scroll: true, // Default to auto-scroll enabled
            rendered_lines_cache: HashMap::new(),
            cache_generation: 0,
            pod_color_cache: HashMap::new(),
            container_color_cache: HashMap::new(),
        })
    }

    pub fn add_log_entry(&mut self, entry: &LogEntry) {
        // Remove oldest entries if we exceed max_lines
        if self.log_entries.len() >= self.max_lines {
            self.log_entries.pop_front();
            // When removing entries from front, we need to adjust scroll offset more carefully
            // to maintain the user's relative position in the log history
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(entry.clone());
        self.filtered_logs += 1;
        self.total_logs += 1;
        
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
                Constraint::Length(3), // Filter input area
                Constraint::Min(0),    // Log display area
                Constraint::Length(1), // Status bar
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

    fn render_log_area(&mut self, f: &mut Frame, area: Rect, _input_handler: &InputHandler) {
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
        
        // Create colored lines from log entries
        let colored_lines: Vec<Line> = if visible_entries.is_empty() {
            vec![Line::from(Span::styled(
                "No logs to display",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
            ))]
        } else {
            visible_entries.iter().map(|entry| {
                self.create_colored_log_line(entry)
            }).collect()
        };

        let logs_paragraph = Paragraph::new(colored_lines)
            .block(
                Block::default()
                    .title(format!(" Logs ({}/{}) ", self.filtered_logs, self.total_logs))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
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

        let colors = [
            Color::Cyan, Color::Green, Color::Yellow, Color::Blue,
            Color::Magenta, Color::LightCyan, Color::LightGreen, Color::LightYellow,
            Color::LightBlue, Color::LightMagenta
        ];
        
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

        let colors = [
            Color::LightCyan, Color::LightGreen, Color::LightYellow,
            Color::LightBlue, Color::LightMagenta, Color::Cyan,
            Color::Green, Color::Yellow, Color::Blue, Color::Magenta
        ];
        
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
                    Style::default().fg(Color::DarkGray)
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
        spans.push(Span::styled("/".to_string(), Style::default().fg(Color::DarkGray)));

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
                        "TRACE" => Color::DarkGray,
                        "DEBUG" => Color::Blue,
                        "INFO" => Color::Green,
                        "WARN" | "WARNING" => Color::Yellow,
                        "ERROR" => Color::Red,
                        "FATAL" => Color::LightRed,
                        _ => Color::White,
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
        // Base color depends on log level
        let base_color = match log_level {
            "ERROR" | "FATAL" => Color::LightRed,
            "WARN" | "WARNING" => Color::LightYellow,
            "DEBUG" => Color::LightBlue,
            "TRACE" => Color::Gray,
            _ => Color::White,
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

    fn render_status_bar(&self, f: &mut Frame, area: Rect, input_handler: &InputHandler) {
        let mode_text = match input_handler.mode {
            InputMode::Normal => "NORMAL",
            InputMode::EditingInclude => "EDIT INCLUDE",
            InputMode::EditingExclude => "EDIT EXCLUDE", 
            InputMode::Help => "HELP",
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

        let status_spans = vec![
            Span::styled(
                format!(" {} ", mode_text),
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            ),
            Span::raw(" "),
            auto_scroll_indicator,
            Span::raw(" "),
            Span::styled(
                format!("Lines: {}/{}", self.filtered_logs, self.total_logs),
                Style::default().fg(Color::White)
            ),
            Span::raw(" | "),
            Span::styled(
                format!("Scroll: {}/{}", self.scroll_offset, self.log_entries.len()),
                Style::default().fg(Color::White)
            ),
            Span::raw(" | "),
            Span::styled(
                "f:Toggle-Follow h:Help q:Quit ↑↓:Scroll i:Include e:Exclude",
                Style::default().fg(Color::DarkGray)
            ),
        ];

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