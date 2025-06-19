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
    pub dev_mode: bool, // Add dev mode flag
    // Enhanced buffer management
    pub enhanced_buffer_active: bool, // Whether 5x buffer expansion is active
    pub memory_warning_shown: bool,   // Track if we've shown the 90% warning
    pub memory_warning_active: bool,  // Track if warning popup is currently displayed
    pub file_output_mode: bool,       // When true, skip display buffer and write only to file
    // Performance optimizations
    cache_generation: usize,
    // Pre-computed colors for pods/containers
    pod_color_cache: HashMap<String, Color>,
    container_color_cache: HashMap<String, Color>,
    // Color scheme for adaptive colors
    color_scheme: ColorScheme,
    pub selection: Option<Selection>, // Add back selection for pause mode
    pub selection_cursor: usize,     // Cursor position for keyboard selection
    // Display window tracking
    pub display_start_index: usize,  // Start index of visible display window
    pub display_end_index: usize,    // End index of visible display window
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
            original_max_lines: actual_max_lines, // Store original buffer size
            filtered_logs: 0,
            show_timestamps,
            auto_scroll: true, // Default to auto-scroll enabled
            cache_generation: 0,
            pod_color_cache: HashMap::new(),
            container_color_cache: HashMap::new(),
            dev_mode: dev_mode, // Set dev mode from parameter
            color_scheme,
            enhanced_buffer_active: false, // Default to 5x buffer expansion inactive
            memory_warning_shown: false,   // Memory warning not shown initially
            memory_warning_active: false,  // Warning popup not active initially
            file_output_mode: false,       // Normal display mode
            selection: None,               // No active selection initially
            selection_cursor: 0,           // Cursor position for selection
            display_start_index: 0,        // Initialize display start index
            display_end_index: 0,          // Initialize display end index
        })
    }

    /// Enable file output mode - logs always get written to file
    pub fn set_file_output_mode(&mut self, enabled: bool) {
        self.file_output_mode = enabled;
        if enabled {
            self.add_system_log("📁 File output mode enabled - all logs will be saved to file");
        }
    }

    /// Activate enhanced buffer mode (5x expansion) for selection/follow modes
    pub fn activate_enhanced_buffer(&mut self) {
        if !self.enhanced_buffer_active {
            let new_size = self.original_max_lines * 5; // 5x expansion
            self.max_lines = new_size;
            self.enhanced_buffer_active = true;
            self.log_entries.reserve(self.original_max_lines * 4); // Reserve additional space
            self.add_system_log(&format!("🚀 Enhanced buffer activated: {} → {} lines (5x expansion)", 
                self.original_max_lines, new_size));
        }
    }

    /// Deactivate enhanced buffer mode and return to normal size
    pub fn deactivate_enhanced_buffer(&mut self) {
        if self.enhanced_buffer_active {
            self.enhanced_buffer_active = false;
            self.max_lines = self.original_max_lines;
            
            // Trim excess entries if buffer is over normal size
            let entries_to_remove = self.log_entries.len().saturating_sub(self.max_lines);
            if entries_to_remove > 0 {
                self.add_system_log(&format!("📉 Trimming {} excess entries from enhanced buffer", 
                    entries_to_remove));
                
                // Remove from front to keep recent logs
                for _ in 0..entries_to_remove {
                    self.log_entries.pop_front();
                    // **FIX BUFFER DEACTIVATION SCROLL**: Only adjust scroll when not in selection mode
                    if self.selection.is_none() && self.scroll_offset > 0 {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    }
                }
            }
            
            self.log_entries.shrink_to_fit();
            self.add_system_log(&format!("📉 Enhanced buffer deactivated: restored to {} lines", 
                self.max_lines));
        }
    }

    /// Check if buffer is approaching capacity and show warning at 80%
    pub fn check_memory_warning(&mut self) -> bool {
        // In follow mode, we never show memory warnings as logs will cycle out automatically
        if self.auto_scroll {
            // Reset any active warning state if we switch to follow mode
            if self.memory_warning_shown || self.memory_warning_active {
                self.memory_warning_shown = false;
                self.memory_warning_active = false;
            }
            return false;
        }
        
        // In selection mode (paused), calculate usage percentage against the 
        // current max_lines (which would be 5x original in enhanced mode)
        let usage_percent = (self.log_entries.len() as f64 / self.max_lines as f64) * 100.0;
        
        // Show warning if we hit 80.0% of the (potentially expanded) buffer size and haven't shown it yet
        if usage_percent >= 80.0 && !self.memory_warning_shown {
            self.memory_warning_shown = true;
            self.memory_warning_active = true; // Activate the persistent popup
            self.add_system_log(&format!("⚠️ Buffer usage at {:.1}% of capacity ({} entries)", 
                                     usage_percent, self.log_entries.len()));
            return true; // Signal to show warning popup
        }
        
        // Reset warning if usage drops below 70%
        if usage_percent < 70.0 && self.memory_warning_shown {
            self.memory_warning_shown = false;
            self.memory_warning_active = false; // Deactivate popup
        }
        
        // Return true if popup should be displayed (persistent until dismissed)
        self.memory_warning_active
    }

    /// Get memory usage percentage for status bar indicator
    pub fn get_memory_usage_percent(&self) -> f64 {
        // Use the actual 5x limit when in pause mode with enhanced buffer
        let effective_max_buffer = if self.enhanced_buffer_active {
            // In enhanced mode, use the full 5x limit for accurate percentage
            self.original_max_lines * 5
        } else {
            self.max_lines
        };
        
        if effective_max_buffer == 0 {
            return 0.0;
        }
        (self.log_entries.len() as f64 / effective_max_buffer as f64) * 100.0
    }

    /// Check if memory is critically high (80%+) - for status bar indicator
    pub fn is_memory_critical(&self) -> bool {
        // Only show memory indicator in pause mode when we have selection capability
        if self.auto_scroll {
            return false;
        }
        
        // Show critical when we're at or near 80% of the 5x limit
        self.get_memory_usage_percent() >= 80.0
    }

    /// Dismiss the memory warning popup
    pub fn dismiss_memory_warning(&mut self) {
        self.memory_warning_active = false;
        self.add_system_log("📊 Memory warning dismissed");
    }

    /// Check if we should insert into display buffer
    /// Returns true if we should insert, false if we should skip (but continue polling)
    pub fn should_insert_to_buffer(&self) -> bool {
        // **ENHANCED LOGIC**: Always insert if file output mode is enabled AND buffer not at 80% limit
        if self.file_output_mode {
            // In pause mode with enhanced buffer, stop inserting once we reach 90% of 5x limit
            if !self.auto_scroll && self.enhanced_buffer_active {
                let five_x_limit = self.original_max_lines * 5;
                let ninety_percent_limit = (five_x_limit as f64 * 0.9) as usize;
                if self.log_entries.len() >= ninety_percent_limit {
                    // Buffer is at 90% of 5x - stop inserting, only write to file
                    return false;
                }
            }
            return true;
        }
        
        // **CRITICAL FIX**: In pause mode with enhanced buffer active, stop at 90% of 5x capacity
        if !self.auto_scroll && self.enhanced_buffer_active {
            let five_x_limit = self.original_max_lines * 5;
            let ninety_percent_limit = (five_x_limit as f64 * 0.9) as usize;
            if self.log_entries.len() >= ninety_percent_limit {
                // Buffer reached 90% of 5x limit - stop all insertions to preserve selection stability
                return false;
            }
            // Still room in enhanced buffer (under 90%) - allow insertion
            return true;
        }
        
        // **NORMAL MODE**: Insert if buffer is not full 
        if self.log_entries.len() < self.max_lines {
            return true;
        }
        
        // **FOLLOW MODE**: Insert with rotation when following logs
        if self.auto_scroll {
            return true;
        }
        
        // **PAUSE MODE**: Skip insertion if buffer is full and user is not following
        // This prevents memory bloat when user is browsing old logs
        false
    }

    /// Toggle follow mode - activates/deactivates enhanced buffer (5x expansion)
    pub fn toggle_follow_mode(&mut self) {
        if self.auto_scroll {
            // Currently following -> switch to paused mode
            self.auto_scroll = false;
            
            // Save the current position before activating enhanced buffer
            let current_position = self.scroll_offset;
            
            // Expand buffer in pause mode for better browsing
            self.activate_enhanced_buffer(); 
            
            // Make sure we preserve the scroll position after buffer expansion
            self.scroll_offset = current_position;
            
            self.add_system_log("⏸️ Follow mode disabled - Buffer expanded to 5x size for browsing");
        } else {
            // Currently paused -> switch to follow mode
            
            // Clear any active selection before switching to follow mode
            self.selection = None;
            self.selection_cursor = 0;
            
            // Clear any memory warnings since we're returning to normal mode
            self.memory_warning_shown = false;
            self.memory_warning_active = false;
            
            self.auto_scroll = true;
            self.deactivate_enhanced_buffer(); // This will trim buffer back to normal size
            
            // Scroll to bottom when enabling follow mode
            let estimated_viewport = 20;
            let max_scroll = self.log_entries.len().saturating_sub(estimated_viewport);
            self.scroll_offset = max_scroll;
            self.add_system_log("▶️ Follow mode enabled - Buffer restored to original size, memory warnings cleared");
        }
    }

    pub fn add_log_entry(&mut self, entry: &LogEntry) -> bool {
        // Check if we should insert this entry into the display buffer
        if !self.should_insert_to_buffer() {
            // Skip insertion but return false to indicate we're dropping display logs
            return false;
        }

        // **ENHANCED BUFFER MANAGEMENT**: Use 5x expansion for pause modes
        let current_max_buffer = if self.enhanced_buffer_active {
            self.max_lines // Already expanded to 5x in enhanced mode
        } else {
            self.original_max_lines // Normal 1x buffer
        };

        // **CRITICAL FIX**: Calculate 90% threshold to ensure NO ROTATION after memory full
        let ninety_percent_threshold = if !self.auto_scroll && self.enhanced_buffer_active {
            let five_x_limit = self.original_max_lines * 5;
            (five_x_limit as f64 * 0.9) as usize
        } else {
            current_max_buffer // For normal/follow mode, use standard threshold
        };

        // **ABSOLUTE PROTECTION**: NEVER rotate if we're at or above 90% threshold
        let should_rotate = if self.log_entries.len() >= ninety_percent_threshold {
            // At or above 90% - NO ROTATION ALLOWED
            false
        } else if !self.auto_scroll && self.enhanced_buffer_active {
            // Enhanced mode: only rotate if we're below 90% AND at max buffer
            self.log_entries.len() >= current_max_buffer
        } else {
            // Normal rotation behavior for follow mode and normal buffer
            self.log_entries.len() >= current_max_buffer
        };

        // **SMART BUFFER ROTATION**: Only rotate when necessary AND allowed
        if should_rotate {
            let has_active_selection = self.selection.is_some();
            
            if has_active_selection {
                // If selection is active, clear it before rotation to prevent index corruption
                self.add_system_log("⚠️ Buffer rotation with active selection - clearing selection to prevent corruption");
                self.clear_selection();
            }
            
            // Apply buffer rotation
            self.log_entries.pop_front();
            
            // **FIX SCROLL COUNTER**: Only adjust scroll when not in selection mode to prevent conflicts
            if self.selection.is_none() && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(entry.clone());
        
        // **FIX FILTERED LOGS COUNTER**: Only increment when actually inserted to buffer
        self.filtered_logs += 1;
        
        // If auto-scroll is enabled, keep scroll at bottom
        if self.auto_scroll {
            // Calculate viewport height conservatively (will be corrected during render)
            let estimated_viewport = 20; // Conservative estimate
            let max_scroll = self.log_entries.len().saturating_sub(estimated_viewport);
            self.scroll_offset = max_scroll;
            self.update_display_window(estimated_viewport);
        }

        // Invalidate cache on new log entry
        self.cache_generation += 1;
        
        // Return true to indicate successful insertion
        true
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false; // Disable auto-scroll on manual scroll
        
        // Validate scroll offset bounds
        self.validate_scroll_bounds(50); // Use conservative viewport estimate
        
        // Update display window after manual scroll
        self.update_display_window(50);
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
        
        // Update display window after manual scroll
        self.update_display_window(viewport_height);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = false; // Disable auto-scroll on manual scroll
        
        // Update display window to reflect scroll position
        self.update_display_window(20);
    }

    pub fn scroll_to_bottom(&mut self, viewport_height: usize) {
        // Fix: Calculate scroll offset based on actual log lines, not display lines
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        self.scroll_offset = max_scroll;
        // Auto-enable auto-scroll when going to bottom
        self.auto_scroll = true;
        
        // Update display window to reflect scroll position
        self.update_display_window(viewport_height);
    }

    /// Validate and fix scroll bounds to prevent corruption
    fn validate_scroll_bounds(&mut self, viewport_height: usize) {
        let max_possible_scroll = if self.log_entries.len() > viewport_height {
            self.log_entries.len() - viewport_height
        } else {
            0
        };
        
        // **FIX HIGH SCROLL VALIDATION**: Only adjust if scroll is SIGNIFICANTLY beyond bounds
        // This prevents unwanted corrections at valid high scroll positions like 10,000
        if self.scroll_offset > max_possible_scroll + 100 {  // Allow some buffer for high positions
            let old_scroll = self.scroll_offset;
            self.scroll_offset = max_possible_scroll;
            if self.dev_mode {
                self.add_system_log(&format!("🔧 BOUNDS CORRECTION: {} → {} (was way beyond max {})", 
                    old_scroll, self.scroll_offset, max_possible_scroll));
            }
        }
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
            // **FIX SYSTEM MESSAGE SCROLL**: Only adjust scroll when not in selection mode
            if self.selection.is_none() && self.scroll_offset > 0 {
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
                    .bg(Color::Blue)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            ),
            Span::raw(" "),
            auto_scroll_indicator,
        ];

        // Add memory warning indicator when in pause mode and memory is critical
        if self.is_memory_critical() {
            status_spans.push(Span::raw(" "));
            status_spans.push(Span::styled(
                " 🔴 DISPLAY BUFFER FULL ",
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
            ));
        }

        status_spans.extend(vec![
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
        ]);

        // Updated help text without selection mode references
        let help_text = "f:Toggle-Follow h:Help q:Quit ↑↓:Scroll i:Include e:Exclude Ctrl+c:Copy";

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

    /// Get selected logs as text for clipboard copying
    pub fn get_selected_logs_as_text(&self) -> String {
        if let Some(ref selection) = self.selection {
            if selection.is_active {
                let start = selection.start_line;
                let end = selection.end_line.min(self.log_entries.len().saturating_sub(1));
                
                let mut result = String::new();
                for i in start..=end {
                    if let Some(entry) = self.log_entries.get(i) {
                        let formatted_line = self.format_log_for_ui(entry);
                        result.push_str(&formatted_line);
                        result.push('\n');
                    }
                }
                
                // Remove the last newline to avoid extra blank line
                if result.ends_with('\n') {
                    result.pop();
                }
                
                return result;
            }
        }
        
        "No text selected".to_string()
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

        // Only show memory warning popup if no selection is active AND memory warning is active
        // This prevents the intrusive popup when user is actively selecting text
        if self.memory_warning_active && self.selection.is_none() {
            self.render_memory_warning_popup(f);
        }
    }

    /// Render memory warning popup when buffer reaches 90% capacity
    fn render_memory_warning_popup(&self, f: &mut Frame) {
        let area = f.size();
        let popup_area = centered_rect(60, 30, area);

        // Clear the area
        f.render_widget(Clear, popup_area);

        let usage_percent = (self.log_entries.len() as f64 / self.max_lines as f64) * 100.0;
        let usage_mb = (self.log_entries.len() * 400) / 1024 / 1024; // Estimate ~400 bytes per entry

        let warning_lines = vec![
            Line::from(vec![
                Span::styled("⚠️ BUFFER WARNING ⚠️", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Buffer usage: "),
                Span::styled(format!("{:.1}%", usage_percent), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(format!(" (~{}MB)", usage_mb))
            ]),
            Line::from(vec![
                Span::raw("Buffer size: "),
                Span::styled(format!("{} entries", self.log_entries.len()), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" / {} max", self.max_lines))
            ]),
            Line::from(""),
            Line::from("Options:"),
            Line::from("• Press 'f' to resume follow mode"),
            if !self.file_output_mode {
                Line::from("• Consider using -w flag for file output")
            } else {
                Line::from("• File output mode: logs are safely stored")
            },
            Line::from(""),
            Line::from(vec![
                Span::styled("Press any key to dismiss", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            ])
        ];

        let warning_paragraph = Paragraph::new(warning_lines)
            .block(
                Block::default()
                    .title(" Display Buffer Warning ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .style(Style::default().bg(Color::Black))
            )
            .wrap(Wrap { trim: true })
            .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(warning_paragraph, popup_area);
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
        
        // **FIX RENDER SCROLL JUMP**: Only validate scroll if there's an actual rendering problem
        // Don't automatically "fix" scroll positions that might be valid (like high scroll positions)
        
        // First, try to render with current scroll position and see if it works
        let entries_available = self.log_entries.len();
        let can_render_safely = if entries_available == 0 {
            // **FIX SCROLL RESET**: Only reset scroll to 0 if we're certain there are truly no entries
            // AND we're not in the middle of buffer operations that might temporarily show 0 entries
            if self.scroll_offset != 0 && !self.enhanced_buffer_active {
                // Only reset in normal mode when buffer is genuinely empty
                self.add_system_log(&format!("🔧 Buffer genuinely empty, resetting scroll from {} to 0", self.scroll_offset));
                self.scroll_offset = 0;
            } else if self.scroll_offset != 0 && self.enhanced_buffer_active {
                // **CRITICAL FIX**: In enhanced buffer mode, NEVER reset scroll during temporary empty states
                // This was causing the jump from 2000 → 0 during buffer operations
                self.add_system_log(&format!("⚠️ Enhanced buffer shows 0 entries but preserving scroll at {} (likely temporary state)", self.scroll_offset));
                // DON'T reset scroll_offset here - preserve user's position
            }
            true
        } else if self.scroll_offset < entries_available {
            // Scroll position is within available entries - safe to render
            true
        } else {
            // Scroll position is beyond available entries - need to adjust
            let max_safe_offset = if entries_available > viewport_height {
                entries_available - viewport_height
            } else {
                0
            };
            
            // **FIX AGGRESSIVE SCROLL CORRECTION**: Only adjust if scroll is WAY beyond reasonable bounds
            if self.scroll_offset > max_safe_offset + 500 {  // Much more tolerance for high scroll positions
                self.add_system_log(&format!("🔧 RENDER FIX: Scroll {} WAY exceeds available entries {}, adjusting to {}", 
                    self.scroll_offset, entries_available, max_safe_offset));
                self.scroll_offset = max_safe_offset;
            } else {
                // Scroll is high but potentially valid - preserve it and log the situation
                self.add_system_log(&format!("📊 High scroll position {} with {} entries - preserving (may be valid)", 
                    self.scroll_offset, entries_available));
            }
            true
        };

        // Get the visible log entries with bounds checking
        let visible_entries: Vec<LogEntry> = if can_render_safely && !self.log_entries.is_empty() {
            self.log_entries
                .iter()
                .skip(self.scroll_offset)
                .take(viewport_height)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        
        // Create colored lines from log entries with selection highlighting
        let colored_lines: Vec<Line> = if visible_entries.is_empty() {
            // **ENHANCED EMPTY STATE MESSAGING**: Provide better context about why display is empty
            let empty_message = if self.log_entries.is_empty() {
                "No logs to display"
            } else if self.scroll_offset >= self.log_entries.len() {
                "Scroll position beyond available logs - scroll up to see content"
            } else {
                "Loading logs..."
            };
            
            vec![Line::from(Span::styled(
                empty_message,
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
            ))]
        } else {
            visible_entries.iter().enumerate().map(|(display_index, entry)| {
                let absolute_line_index = self.scroll_offset + display_index;
                let is_selected = self.is_line_selected(absolute_line_index);
                
                let mut line = self.create_colored_log_line(entry);
                
                // Apply selection highlighting
                if is_selected {
                    line = self.apply_selection_highlight(line);
                }
                
                line
            }).collect()
        };

        let title = format!(" Logs ({}) ", self.filtered_logs);

        let logs_paragraph = Paragraph::new(colored_lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
            )
            .wrap(Wrap { trim: false });

        f.render_widget(logs_paragraph, area);
    }

    /// Check if a line is within the current selection
    fn is_line_selected(&self, line_index: usize) -> bool {
        if let Some(ref selection) = self.selection {
            if selection.is_active {
                let start = selection.start_line.min(selection.end_line);
                let end = selection.start_line.max(selection.end_line);
                return line_index >= start && line_index <= end;
            }
        }
        false
    }

    /// Apply selection highlighting to a line
    fn apply_selection_highlight(&self, line: Line<'static>) -> Line<'static> {
        let highlighted_spans: Vec<Span> = line.spans.into_iter().map(|span| {
            Span::styled(
                span.content,
                span.style.bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            )
        }).collect();
        
        Line::from(highlighted_spans)
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
        
        // Reset counters
        self.filtered_logs = 0;
        self.scroll_offset = 0;
        self.cache_generation = 0;
        
        // Log cleanup for performance monitoring
        tracing::info!("Buffer cleanup completed: {} log entries cleared, {} cache entries cleared", 
                      buffer_size_before, cache_entries_before);
    }

    /// Update display window indices based on current scroll position and viewport
    fn update_display_window(&mut self, viewport_height: usize) {
        self.display_start_index = self.scroll_offset;
        self.display_end_index = (self.scroll_offset + viewport_height - 1).min(self.log_entries.len().saturating_sub(1));
        
        if self.dev_mode {
            self.add_system_log(&format!("📊 Display window: start={}, end={}", 
                self.display_start_index, self.display_end_index));
        }
    }

    /// Get absolute line index from relative viewport position - memory safe
    fn get_absolute_line_safe(&self, relative_y: usize) -> Option<usize> {
        let absolute_line = self.scroll_offset.saturating_add(relative_y);
        
        // Ensure we don't go beyond available logs
        if absolute_line < self.log_entries.len() {
            Some(absolute_line)
        } else {
            None
        }
    }

    /// Check if selection boundary requires display window adjustment
    fn check_selection_display_adjustment(&mut self, _viewport_height: usize) -> bool {
        // **FIX SELECTION SCROLL JUMP**: Disable dynamic scroll adjustments during selection
        // This was causing unwanted scroll jumps at high positions like 10,000
        // When user clicks to select text, we should preserve their current scroll position
        
        if let Some(ref selection) = self.selection {
            let selection_start = selection.start_line;
            let selection_end = selection.end_line;
            
            // Debug current state but don't modify scroll
            self.add_system_log(&format!("🔍 Selection: start={}, end={} (scroll preserved at {})", 
                selection_start, selection_end, self.scroll_offset));
            self.add_system_log(&format!("🔍 Display: start={}, end={}", 
                self.display_start_index, self.display_end_index));
            
            // **PRESERVE SCROLL POSITION**: Don't auto-scroll during selection operations
            // Let the user manually scroll if they want to see different parts of their selection
            // This prevents the jarring jump from high scroll positions
            
            self.add_system_log("🔧 Dynamic scroll adjustment disabled - preserving user's scroll position");
            return false; // Always return false to prevent scroll modifications
        }
        
        false
    }

    /// Handle mouse click for text selection in pause mode
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Only allow selection in pause mode (not in follow mode)
        if self.auto_scroll {
            return false;
        }
        
        // Check if click is within the log area
        if x >= log_area.x + 1 && x < log_area.x + log_area.width - 1 &&
           y >= log_area.y + 2 && y < log_area.y + log_area.height - 1 {
            
            let content_start_y = log_area.y + 2;
            let relative_y = (y - content_start_y) as usize;
            let viewport_height = (log_area.height as usize).saturating_sub(3);
            
            // **FIX CLICK SCROLL JUMP**: Don't validate scroll during click - preserve user's scroll position
            // The automatic scroll validation was causing unwanted jumps at high scroll positions like 10,000
            
            // MEMORY-SAFE: Use safe line calculation with current scroll position (no validation)
            if let Some(clicked_line) = self.get_absolute_line_safe(relative_y) {
                // SMART CLICK BEHAVIOR: Check if we're clicking on existing selection
                if let Some(ref existing_selection) = self.selection {
                    // Check if click is within the existing selection range
                    let selection_start = existing_selection.start_line.min(existing_selection.end_line);
                    let selection_end = existing_selection.start_line.max(existing_selection.end_line);
                    
                    if clicked_line >= selection_start && clicked_line <= selection_end {
                        // Clicking inside existing selection - clear it (user wants to deselect)
                        self.clear_selection();
                        self.add_system_log(&format!("🖱️ Clicked inside selection - cleared selection"));
                        return true;
                    } else {
                        // Clicking outside existing selection - clear old and prepare for new selection
                        self.add_system_log(&format!("🔄 Clearing previous selection: {}..{}", 
                            existing_selection.start_line, existing_selection.end_line));
                        self.selection = None; // Clear old selection
                    }
                }
                
                // Start new selection but DON'T set dragging state yet
                // We'll only start dragging if the user actually drags the mouse
                let new_selection = Selection::new(clicked_line);
                self.selection = Some(new_selection);
                self.selection_cursor = relative_y.min(viewport_height.saturating_sub(1));
                
                self.add_system_log(&format!("🖱️ Selection anchor set at line {} (scroll: {}) - drag to extend", 
                    clicked_line, self.scroll_offset));
                return true;
            } else {
                self.add_system_log(&format!("❌ Invalid click position: relative_y={}, scroll={}, buffer_size={}", 
                    relative_y, self.scroll_offset, self.log_entries.len()));
                return false;
            }
        }
        false
    }
    
    /// Handle mouse drag for extending selection with dynamic display adjustment
    pub fn handle_mouse_drag(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Debug every mouse drag event
        self.add_system_log(&format!("🖱️ Mouse drag: x={}, y={}, log_area={}x{}", x, y, log_area.width, log_area.height));
        
        // Check if we have a selection anchor (from mouse click)
        if self.selection.is_none() {
            self.add_system_log("❌ No selection anchor - click first to start selection");
            return false;
        }
        
        // Start dragging if not already dragging (first drag event after click)
        if let Some(ref mut selection) = self.selection {
            if !selection.is_dragging {
                selection.start_drag();
                self.add_system_log("🖱️ Started dragging - selection now active");
            }
        }
        
        // Now check if we have an active dragging selection
        let has_dragging_selection = self.selection.as_ref()
            .map(|s| s.is_dragging)
            .unwrap_or(false);
            
        self.add_system_log(&format!("🔍 Has dragging selection: {}", has_dragging_selection));
        
        // Debug current selection state
        if let Some(ref selection) = self.selection {
            self.add_system_log(&format!("🔍 Current selection state: active={}, dragging={}, range={}..{}", 
                selection.is_active, selection.is_dragging, selection.start_line, selection.end_line));
        }
            
        if !has_dragging_selection {
            self.add_system_log("❌ No active dragging selection - this shouldn't happen!");
            return false;
        }
        
        if x < log_area.x + 1 || x >= log_area.x + log_area.width - 1 {
            self.add_system_log(&format!("❌ Mouse outside horizontal bounds: x={}, bounds={}..{}", 
                x, log_area.x + 1, log_area.x + log_area.width - 1));
            return false;
        }
        
        let content_start_y = log_area.y + 2;
        let content_end_y = log_area.y + log_area.height - 1;
        let viewport_height = (log_area.height as usize).saturating_sub(3);
        
        self.add_system_log(&format!("🔍 Viewport: content_y={}..{}, height={}", 
            content_start_y, content_end_y, viewport_height));
        
        // **FIX DRAG SCROLL JUMP**: Don't validate scroll during drag - preserve user's scroll position
        // The automatic scroll validation was causing unwanted jumps at high scroll positions like 10,000
        
        // Update display window based on current scroll position (no validation)
        self.update_display_window(viewport_height);
        
        // Calculate drag line position (handle edge cases)
        let relative_y = if y < content_start_y {
            0 // Dragging above viewport
        } else if y >= content_end_y {
            viewport_height.saturating_sub(1) // Dragging below viewport
        } else {
            (y - content_start_y) as usize // Normal position
        };
        
        self.add_system_log(&format!("🔍 Drag position: relative_y={}, scroll_offset={}", 
            relative_y, self.scroll_offset));
        
        // MEMORY-SAFE: Use safe line calculation for drag position (no scroll validation)
        if let Some(drag_line) = self.get_absolute_line_safe(relative_y) {
            let mut debug_messages = Vec::new();
            
            if let Some(ref mut selection) = self.selection {
                // Store the original selection for debugging
                let original_start = selection.start_line;
                let original_end = selection.end_line;
                
                // Extend selection to new drag position
                selection.extend_to(drag_line);
                
                // Critical fix: Ensure we preserve the original start point
                if selection.start_line != original_start {
                    debug_messages.push(format!("⚠️ Selection start moved from {} to {} - fixing", 
                        original_start, selection.start_line));
                        
                    // Restore proper start/end based on drag direction
                    if drag_line < original_start {
                        selection.start_line = drag_line;
                        selection.end_line = original_start;
                    } else {
                        selection.start_line = original_start;
                        selection.end_line = drag_line;
                    }
                }
                
                let selection_changed = selection.start_line != original_start || selection.end_line != original_end;
                
                if selection_changed {
                    debug_messages.push(format!("📝 Selection updated: {}..{} (was {}..{})", 
                        selection.start_line, selection.end_line, original_start, original_end));
                }
                
                // Update selection cursor to follow drag
                self.selection_cursor = relative_y.min(viewport_height.saturating_sub(1));
            } else {
                self.add_system_log("❌ Selection disappeared during drag - this shouldn't happen!");
                return false;
            }
            
            // Log debug messages after we're done with selection borrow
            for msg in debug_messages {
                self.add_system_log(&msg);
            }
            
            // MEMORY-SAFE: Only adjust display if not at memory limit
            let scrolled = if !self.is_memory_critical() {
                self.check_selection_display_adjustment(viewport_height)
            } else {
                // When memory is full, avoid dynamic scrolling to maintain stability
                false
            };
            
            self.add_system_log(&format!("🔄 Result: scrolled={}, memory_full={}", 
                scrolled, self.is_memory_critical()));
            
            // Return true to force UI update
            true
        } else {
            self.add_system_log(&format!("❌ Invalid drag position: relative_y={}, scroll={}", 
                relative_y, self.scroll_offset));
            false
        }
    }
    
    /// Handle mouse release to end dragging
    pub fn handle_mouse_release(&mut self) -> bool {
        if let Some(ref mut selection) = self.selection {
            if selection.is_dragging {
                selection.end_drag();
                let lines_selected = selection.end_line - selection.start_line + 1;
                let start_line = selection.start_line;
                let end_line = selection.end_line;
                
                // Store debug message to log after borrow ends
                let debug_message = format!("📝 SELECTION COMPLETE: {} lines selected ({}..{}) - Ctrl+C to copy", 
                    lines_selected, start_line, end_line);
                
                // Log debug message after we're done with selection borrow
                self.add_system_log(&debug_message);
                self.add_system_log("🔧 Selection state finalized, ready for next selection");
                return true;
            }
        }
        false
    }
    
    /// Extend selection up with keyboard
    pub fn select_up(&mut self) {
        if let Some(ref mut selection) = self.selection {
            if selection.start_line > 0 {
                selection.start_line -= 1;
                // Move cursor up as well
                if self.selection_cursor > 0 {
                    self.selection_cursor -= 1;
                } else if self.scroll_offset > 0 {
                    // **FIX KEYBOARD SELECTION SCROLL**: Only scroll when buffer is not at critical capacity
                    // Check memory usage before scroll adjustment to avoid conflicts
                    let usage_percent = (self.log_entries.len() as f64 / self.max_lines as f64) * 100.0;
                    if usage_percent < 80.0 {
                        self.scroll_offset -= 1;
                    }
                }
                
                let lines_selected = selection.end_line - selection.start_line + 1;
                self.add_system_log(&format!("📝 Selection extended: {} lines", lines_selected));
            }
        }
    }
    
    /// Extend selection down with keyboard
    pub fn select_down(&mut self, viewport_height: usize) {
        if let Some(ref mut selection) = self.selection {
            if selection.end_line + 1 < self.log_entries.len() {
                selection.end_line += 1;
                // Move cursor down as well
                let max_cursor = viewport_height.saturating_sub(3);
                if self.selection_cursor < max_cursor {
                    self.selection_cursor += 1;
                } else if self.scroll_offset + max_cursor < self.log_entries.len() {
                    // **FIX KEYBOARD SELECTION SCROLL**: Only scroll when buffer is not at critical capacity
                    // Check memory usage before scroll adjustment to avoid conflicts
                    let usage_percent = (self.log_entries.len() as f64 / self.max_lines as f64) * 100.0;
                    if usage_percent < 80.0 {
                        self.scroll_offset += 1;
                    }
                }
                
                let lines_selected = selection.end_line - selection.start_line + 1;
                self.add_system_log(&format!("📝 Selection extended: {} lines", lines_selected));
            }
        }
    }
    
    /// Clear current selection and completely reset all selection state
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_cursor = 0;
        
        // Reset any selection-related display state that might persist
        // This ensures a completely fresh start for the next selection
        self.add_system_log("📝 Selection cleared - all state reset");
    }
    
    /// Toggle selection mode and select all logs if no selection exists
    pub fn toggle_selection_all(&mut self) {
        if self.selection.is_some() {
            // If selection exists, clear it
            self.clear_selection();
        } else if !self.log_entries.is_empty() {
            // Create a selection spanning all logs
            self.selection = Some(Selection {
                start_line: 0,
                end_line: self.log_entries.len() - 1,
                is_active: true,
                is_dragging: false,
            });
            let count = self.log_entries.len();
            self.add_system_log(&format!("📝 Selected all {} logs (Ctrl+C to copy)", count));
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
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