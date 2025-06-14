use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::k8s::logs::LogEntry;
use crate::ui::input::{InputHandler, InputMode};
use std::collections::VecDeque;

pub struct DisplayManager {
    pub log_entries: VecDeque<LogEntry>,
    pub scroll_offset: usize,
    pub max_lines: usize,
    pub total_logs: usize,
    pub filtered_logs: usize,
    pub show_timestamps: bool,
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
        })
    }

    pub fn add_log_entry(&mut self, entry: &LogEntry) {
        // Remove oldest entries if we exceed max_lines
        if self.log_entries.len() >= self.max_lines {
            self.log_entries.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(entry.clone());
        self.filtered_logs += 1;
        self.total_logs += 1;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn scroll_down(&mut self, lines: usize, viewport_height: usize) {
        // Calculate the total number of display lines (accounting for wrapped lines)
        let total_display_lines = self.calculate_total_display_lines(viewport_height);
        let max_scroll = total_display_lines.saturating_sub(viewport_height);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn scroll_to_bottom(&mut self, viewport_height: usize) {
        // Fix: Calculate scroll offset based on actual log lines, not display lines
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        self.scroll_offset = max_scroll;
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

    pub fn render(&self, f: &mut Frame, input_handler: &InputHandler) {
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

    fn render_log_area(&self, f: &mut Frame, area: Rect, _input_handler: &InputHandler) {
        let viewport_height = area.height.saturating_sub(2) as usize; // Account for borders
        
        // Get the visible log entries with proper bounds checking
        let visible_entries: Vec<&LogEntry> = self.log_entries
            .iter()
            .skip(self.scroll_offset)
            .take(viewport_height)
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

    /// Create a colored line from a log entry
    fn create_colored_log_line(&self, entry: &LogEntry) -> Line<'static> {
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

    /// Get a color for a pod name based on its hash
    fn get_pod_color(&self, pod_name: &str) -> Color {
        let colors = [
            Color::Cyan, Color::Green, Color::Yellow, Color::Blue,
            Color::Magenta, Color::LightCyan, Color::LightGreen, Color::LightYellow,
            Color::LightBlue, Color::LightMagenta
        ];
        
        let hash = pod_name.chars().map(|c| c as usize).sum::<usize>();
        colors[hash % colors.len()]
    }

    /// Get a color for a container name based on its hash
    fn get_container_color(&self, container_name: &str) -> Color {
        let colors = [
            Color::LightCyan, Color::LightGreen, Color::LightYellow,
            Color::LightBlue, Color::LightMagenta, Color::Cyan,
            Color::Green, Color::Yellow, Color::Blue, Color::Magenta
        ];
        
        let hash = container_name.chars().map(|c| c as usize).sum::<usize>();
        colors[hash % colors.len()]
    }

    /// Parse log message and apply colors based on log level and content
    fn parse_log_message(&self, message: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        
        // Detect log level patterns
        let log_level_regex = regex::Regex::new(r"\[(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL)\]").unwrap();
        
        if let Some(captures) = log_level_regex.find(message) {
            let level = &message[captures.start()+1..captures.end()-1]; // Remove brackets
            let before = &message[..captures.start()];
            let after = &message[captures.end()..];
            
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
                let after_spans = self.color_message_content(after, level);
                spans.extend(after_spans);
            }
        } else {
            // No log level detected, apply general coloring
            let content_spans = self.color_message_content(message, "INFO");
            spans.extend(content_spans);
        }
        
        spans
    }

    /// Apply colors to message content based on patterns
    fn color_message_content(&self, content: &str, log_level: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        
        // Base color depends on log level
        let base_color = match log_level {
            "ERROR" | "FATAL" => Color::LightRed,
            "WARN" | "WARNING" => Color::LightYellow,
            "DEBUG" => Color::LightBlue,
            "TRACE" => Color::Gray,
            _ => Color::White,
        };
        
        // Split by spaces and color different parts
        let words: Vec<&str> = content.split_whitespace().collect();
        
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" ".to_string()));
            }
            
            // Color specific patterns
            let colored_span = if word.contains("error") || word.contains("Error") || word.contains("ERROR") {
                Span::styled(word.to_string(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            } else if word.contains("warn") || word.contains("Warn") || word.contains("WARN") {
                Span::styled(word.to_string(), Style::default().fg(Color::Yellow))
            } else if word.contains("success") || word.contains("Success") || word.contains("SUCCESS") {
                Span::styled(word.to_string(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else if word.contains("fail") || word.contains("Fail") || word.contains("FAIL") {
                Span::styled(word.to_string(), Style::default().fg(Color::Red))
            } else if word.starts_with("http") || word.starts_with("https") {
                Span::styled(word.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED))
            } else if word.contains(':') && (word.contains("user") || word.contains("key") || word.contains("id")) {
                Span::styled(word.to_string(), Style::default().fg(Color::Magenta))
            } else if word.parse::<f64>().is_ok() || word.ends_with('%') {
                Span::styled(word.to_string(), Style::default().fg(Color::LightCyan))
            } else {
                Span::styled(word.to_string(), Style::default().fg(base_color))
            };
            
            spans.push(colored_span);
        }
        
        spans
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

    /// Strip ANSI escape codes from a string to ensure clean UI display
    fn strip_ansi_codes(text: &str) -> String {
        let ansi_regex = regex::Regex::new(r"(\x1b\[[0-9;]*[a-zA-Z]|\x1b\[[0-9;]*m|\[[0-9;]*m)").unwrap();
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

        let status_spans = vec![
            Span::styled(
                format!(" {} ", mode_text),
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            ),
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
                "h:Help q:Quit ↑↓:Scroll i:Include e:Exclude",
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