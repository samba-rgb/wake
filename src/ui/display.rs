use crate::k8s::logs::LogEntry;
use crate::ui::input::{InputMode, InputHandler};
use regex::Regex;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

/// Global regex instance for ANSI stripping - compiled once
static ANSI_REGEX: OnceLock<Regex> = OnceLock::new();

/// Hash-based line mapping for perfect selection accuracy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LineHash {
    content_hash: u64,
    log_entry_index: usize,
    line_number_in_entry: usize,
    terminal_width: usize,
}

impl LineHash {
    fn new(content: &str, log_entry_index: usize, line_number: usize, terminal_width: usize) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        log_entry_index.hash(&mut hasher);
        line_number.hash(&mut hasher);
        terminal_width.hash(&mut hasher);

        Self {
            content_hash: hasher.finish(),
            log_entry_index,
            line_number_in_entry: line_number,
            terminal_width,
        }
    }
}

/// Visual display line with hash-based tracking
#[derive(Debug, Clone)]
pub struct VisualDisplayLine {
    pub content: String,
    pub hash: LineHash,
    pub scroll_position: usize, // The scroll offset when this line was created
    pub is_wrapped: bool,       // True if this is a continuation of a wrapped line
}

/// Hash-based selection system that maps visual lines to log entries perfectly
#[derive(Debug, Clone)]
pub struct HashBasedSelection {
    pub start_hash: Option<LineHash>,
    pub end_hash: Option<LineHash>,
    pub visual_start_line: usize, // Visual line number in current display
    pub visual_end_line: usize,   // Visual line number in current display
    pub is_active: bool,
    pub is_dragging: bool,
    pub selection_direction: SelectionDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectionDirection {
    Forward,   // From top to bottom
    Backward,  // From bottom to top
    None,      // Single line or no direction yet
}

impl HashBasedSelection {
    pub fn new(line_hash: LineHash, visual_line: usize) -> Self {
        Self {
            start_hash: Some(line_hash.clone()),
            end_hash: Some(line_hash),
            visual_start_line: visual_line,
            visual_end_line: visual_line,
            is_active: true,
            is_dragging: false,
            selection_direction: SelectionDirection::None,
        }
    }

    pub fn extend_to(&mut self, line_hash: LineHash, visual_line: usize) {
        self.end_hash = Some(line_hash);

        // Determine selection direction
        if visual_line < self.visual_start_line {
            self.selection_direction = SelectionDirection::Backward;
            self.visual_end_line = self.visual_start_line;
            self.visual_start_line = visual_line;
        } else if visual_line > self.visual_start_line {
            self.selection_direction = SelectionDirection::Forward;
            self.visual_end_line = visual_line;
        } else {
            self.selection_direction = SelectionDirection::None;
            self.visual_end_line = visual_line;
        }
    }

    pub fn start_drag(&mut self) {
        self.is_dragging = true;
    }

    pub fn end_drag(&mut self) {
        self.is_dragging = false;
    }
}

/// Hash-based line cache for efficient lookups and perfect accuracy
pub struct HashLineCache {
    /// Maps visual line indices to their hashes
    visual_to_hash: HashMap<usize, LineHash>,
    /// Maps hashes back to log entry information
    hash_to_log_info: HashMap<LineHash, (usize, String)>, // (log_entry_index, content)
    /// Current terminal width used for the cache
    terminal_width: usize,
    /// Current scroll offset when cache was built
    scroll_offset: usize,
    /// Generation counter for cache invalidation
    generation: usize,
}

impl HashLineCache {
    pub fn new() -> Self {
        Self {
            visual_to_hash: HashMap::new(),
            hash_to_log_info: HashMap::new(),
            terminal_width: 80,
            scroll_offset: 0,
            generation: 0,
        }
    }

    /// Rebuild the entire cache based on current display state
    pub fn rebuild(&mut self,
                   log_entries: &VecDeque<LogEntry>,
                   scroll_offset: usize,
                   terminal_width: usize,
                   viewport_height: usize,
                   show_timestamps: bool) -> Vec<VisualDisplayLine> {

        self.visual_to_hash.clear();
        self.hash_to_log_info.clear();
        self.terminal_width = terminal_width;
        self.scroll_offset = scroll_offset;
        self.generation += 1;

        let mut visual_lines = Vec::new();
        let mut visual_index = 0;

        // Get the visible entries based on scroll offset
        let visible_entries: Vec<(usize, &LogEntry)> = log_entries
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(viewport_height + 10) // Buffer for wrapped lines
            .collect();

        for (log_entry_index, entry) in visible_entries {
            let formatted_line = Self::format_log_for_display(entry, show_timestamps);

            // Calculate effective content width (account for borders)
            let content_width = if terminal_width > 4 {
                terminal_width - 4
            } else {
                80
            };

            // Generate wrapped lines with perfect hash tracking
            let wrapped_lines = Self::wrap_line_with_hash(&formatted_line, content_width);

            for (line_in_entry, line_content) in wrapped_lines.into_iter().enumerate() {
                let line_hash = LineHash::new(&line_content, log_entry_index, line_in_entry, terminal_width);

                let visual_line = VisualDisplayLine {
                    content: line_content.clone(),
                    hash: line_hash.clone(),
                    scroll_position: scroll_offset,
                    is_wrapped: line_in_entry > 0,
                };

                // Store in caches
                self.visual_to_hash.insert(visual_index, line_hash.clone());
                self.hash_to_log_info.insert(line_hash, (log_entry_index, line_content));

                visual_lines.push(visual_line);
                visual_index += 1;

                // Stop if we have enough lines for the viewport
                if visual_index >= viewport_height + 5 {
                    break;
                }
            }

            if visual_index >= viewport_height + 5 {
                break;
            }
        }

        visual_lines
    }

    /// Get the hash for a visual line index
    pub fn get_hash_for_visual_line(&self, visual_index: usize) -> Option<&LineHash> {
        self.visual_to_hash.get(&visual_index)
    }

    /// Get log entry information from a hash
    pub fn get_log_info_from_hash(&self, hash: &LineHash) -> Option<&(usize, String)> {
        self.hash_to_log_info.get(hash)
    }

    /// Check if cache is valid for current state
    pub fn is_valid(&self, scroll_offset: usize, terminal_width: usize, generation: usize) -> bool {
        self.scroll_offset == scroll_offset &&
        self.terminal_width == terminal_width &&
        self.generation == generation
    }

    /// Format a log entry for display with consistent formatting
    fn format_log_for_display(entry: &LogEntry, show_timestamps: bool) -> String {
        let time_part = if show_timestamps {
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

    /// Wrap a line with perfect hash tracking
    fn wrap_line_with_hash(line: &str, max_width: usize) -> Vec<String> {
        if line.len() <= max_width {
            return vec![line.to_string()];
        }

        let mut wrapped_lines = Vec::new();
        let mut remaining = line;

        while !remaining.is_empty() {
            if remaining.len() <= max_width {
                wrapped_lines.push(remaining.to_string());
                break;
            }

            // Find the best break point
            let mut break_point = max_width;

            // Try to break at a space for better readability
            if let Some(space_pos) = remaining[..max_width].rfind(' ') {
                if space_pos > max_width / 2 { // Don't break too early
                    break_point = space_pos;
                }
            }

            let (current_chunk, rest) = remaining.split_at(break_point);
            wrapped_lines.push(current_chunk.to_string());
            remaining = rest.trim_start(); // Remove leading whitespace from continuation
        }

        wrapped_lines
    }

    /// Strip ANSI codes for consistent formatting
    fn strip_ansi_codes(text: &str) -> String {
        let ansi_regex = ANSI_REGEX.get_or_init(|| Regex::new(r"(\x1b\[[0-9;]*[a-zA-Z]|\x1b\[[0-9;]*m|\[[0-9;]*m)").unwrap());
        ansi_regex.replace_all(text, "").to_string()
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
    // Hash-based selection system for perfect accuracy
    pub hash_selection: Option<HashBasedSelection>,
    pub hash_line_cache: HashLineCache,
    pub terminal_width: usize,
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
            hash_selection: None,          // No active selection initially
            hash_line_cache: HashLineCache::new(), // Initialize hash-based cache
            terminal_width: 80,            // Default terminal width
            selection_cursor: 0,           // Cursor position for selection
            display_start_index: 0,        // Initialize display start index
            display_end_index: 0,          // Initialize display end index
        })
    }
}

/// Modern color scheme with enhanced visual hierarchy
#[derive(Debug, Clone, Copy)]
pub enum ColorScheme {
    Dark,   // For dark terminal backgrounds - Modern dark theme
    Light,  // For light terminal backgrounds - Clean light theme
    Auto,   // Auto-detect based on terminal
}

impl ColorScheme {
    /// Enhanced terminal detection with better heuristics
    pub fn detect() -> Self {
        // Check for common terminal environment indicators
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("256color") || term.contains("truecolor") {
                // Modern terminals with good color support
                return Self::Auto;
            }
        }
        
        // Check for VS Code, which often uses light themes
        if std::env::var("VSCODE_INJECTION").is_ok() || 
           std::env::var("TERM_PROGRAM").map_or(false, |v| v.contains("vscode")) {
            return ColorScheme::Light;
        }
        
        // Check background hints
        if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
            if let Some(bg) = colorfgbg.split(';').nth(1) {
                if let Ok(bg_num) = bg.parse::<i32>() {
                    return if bg_num >= 7 { ColorScheme::Light } else { ColorScheme::Dark };
                }
            }
        }
        
        // Default to dark theme for maximum compatibility
        ColorScheme::Dark
    }
    
    /// Modern primary text color with better contrast
    pub fn primary_text(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(240, 240, 240),  // Soft white
            ColorScheme::Light => Color::Rgb(30, 30, 30),    // Deep black
            ColorScheme::Auto => Color::White,
        }
    }
    
    /// Secondary text color for less important information
    pub fn secondary_text(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(180, 180, 180),  // Light gray
            ColorScheme::Light => Color::Rgb(100, 100, 100), // Medium gray
            ColorScheme::Auto => Color::DarkGray,
        }
    }
    
    /// Accent color for highlights and selections
    pub fn accent_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(100, 200, 255),  // Modern blue
            ColorScheme::Light => Color::Rgb(0, 120, 215),   // Professional blue
            ColorScheme::Auto => Color::Blue,
        }
    }
    
    /// Success/positive color
    pub fn success_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(100, 255, 100),  // Bright green
            ColorScheme::Light => Color::Rgb(0, 150, 0),     // Forest green
            ColorScheme::Auto => Color::Green,
        }
    }
    
    /// Warning color
    pub fn warning_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(255, 200, 0),    // Golden yellow
            ColorScheme::Light => Color::Rgb(200, 140, 0),   // Amber
            ColorScheme::Auto => Color::Yellow,
        }
    }
    
    /// Error/danger color
    pub fn error_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(255, 100, 100),  // Bright red
            ColorScheme::Light => Color::Rgb(200, 50, 50),   // Deep red
            ColorScheme::Auto => Color::Red,
        }
    }
    
    /// Background color for containers and panels
    pub fn panel_bg(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(20, 20, 20),     // Almost black
            ColorScheme::Light => Color::Rgb(250, 250, 250), // Almost white
            ColorScheme::Auto => Color::Black,
        }
    }
    
    /// Border color for modern look
    pub fn border_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(60, 60, 60),     // Dark gray
            ColorScheme::Light => Color::Rgb(200, 200, 200), // Light gray
            ColorScheme::Auto => Color::Gray,
        }
    }
    
    /// Selection background color
    pub fn selection_bg(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(50, 100, 200),   // Deep blue
            ColorScheme::Light => Color::Rgb(180, 210, 255), // Light blue
            ColorScheme::Auto => Color::Blue,
        }
    }
    
    /// Get dim text color for the color scheme
    pub fn dim_text_color(self) -> Color {
        match self {
            ColorScheme::Dark => Color::Rgb(120, 120, 120),   // Dim gray
            ColorScheme::Light => Color::Rgb(140, 140, 140),  // Medium gray
            ColorScheme::Auto => Color::DarkGray,
        }
    }
    
    /// Get default text color
    pub fn text_color(self) -> Color {
        self.primary_text()
    }
    
    /// Get default message color for unknown log levels
    pub fn default_message_color(self) -> Color {
        self.primary_text()
    }
    
    /// Get container colors that work well on this background
    pub fn container_colors(self) -> &'static [Color] {
        // Use similar colors to pod colors but with slight variation
        match self {
            ColorScheme::Dark => &[
                Color::Rgb(120, 180, 255), // Lighter blue
                Color::Rgb(120, 255, 170), // Lighter mint
                Color::Rgb(255, 170, 120), // Lighter coral  
                Color::Rgb(255, 120, 220), // Lighter pink
                Color::Rgb(220, 170, 255), // Lighter lavender
                Color::Rgb(255, 220, 120), // Lighter gold
                Color::Rgb(170, 255, 220), // Lighter aqua
                Color::Rgb(255, 200, 200), // Lighter rose
                Color::Rgb(200, 255, 200), // Lighter green
                Color::Rgb(200, 200, 255), // Lighter blue
            ],
            ColorScheme::Light => &[
                Color::Rgb(20, 80, 180),   // Darker blue
                Color::Rgb(20, 130, 70),   // Darker green
                Color::Rgb(180, 60, 20),   // Darker orange
                Color::Rgb(130, 20, 80),   // Darker purple
                Color::Rgb(80, 30, 130),   // Darker indigo
                Color::Rgb(130, 80, 20),   // Darker brown
                Color::Rgb(20, 100, 100),  // Darker teal
                Color::Rgb(100, 40, 40),   // Darker maroon
                Color::Rgb(40, 100, 40),   // Darker olive
                Color::Rgb(40, 40, 100),   // Darker navy
            ],
            ColorScheme::Auto => &[
                Color::LightCyan, Color::LightGreen, Color::LightYellow,
                Color::LightBlue, Color::LightMagenta, Color::Cyan,
                Color::Green, Color::Yellow, Color::Blue, Color::Magenta
            ],
        }
    }
    
    /// Modern pod colors with better contrast and visual appeal
    pub fn pod_colors(self) -> &'static [Color] {
        match self {
            ColorScheme::Dark => &[
                Color::Rgb(100, 200, 255), // Modern blue
                Color::Rgb(100, 255, 150), // Mint green
                Color::Rgb(255, 150, 100), // Coral
                Color::Rgb(255, 100, 200), // Pink
                Color::Rgb(200, 150, 255), // Lavender
                Color::Rgb(255, 200, 100), // Gold
                Color::Rgb(150, 255, 200), // Aqua
                Color::Rgb(255, 180, 180), // Rose
                Color::Rgb(180, 255, 180), // Light green
                Color::Rgb(180, 180, 255), // Light blue
            ],
            ColorScheme::Light => &[
                Color::Rgb(0, 100, 200),   // Deep blue
                Color::Rgb(0, 150, 50),    // Forest green
                Color::Rgb(200, 80, 0),    // Orange
                Color::Rgb(150, 0, 100),   // Purple
                Color::Rgb(100, 50, 150),  // Indigo
                Color::Rgb(150, 100, 0),   // Brown
                Color::Rgb(0, 120, 120),   // Teal
                Color::Rgb(120, 60, 60),   // Maroon
                Color::Rgb(60, 120, 60),   // Olive green
                Color::Rgb(60, 60, 120),   // Navy
            ],
            ColorScheme::Auto => &[
                Color::Cyan, Color::Green, Color::Yellow, Color::Blue,
                Color::Magenta, Color::LightCyan, Color::LightGreen, 
                Color::LightYellow, Color::LightBlue, Color::LightMagenta
            ],
        }
    }
}

impl DisplayManager {
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
            
            // Clear any active hash-based selection before switching to follow mode
            self.hash_selection = None;
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
    
    /// Update terminal width and invalidate hash cache when terminal is resized
    pub fn update_terminal_width(&mut self, new_width: usize) {
        if self.terminal_width != new_width {
            self.terminal_width = new_width;
            // Invalidate cache to force rebuild with new width
            self.cache_generation += 1;
            self.add_system_log(&format!("📐 Terminal width updated: {} -> {}", self.terminal_width, new_width));
        }
    }
    
    /// Hash-based mouse click handler for perfect selection accuracy
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Only allow selection in pause mode (not in follow mode)
        if self.auto_scroll {
            return false;
        }
        
        // Check if click is within the log area content
        if x < log_area.x + 1 || x >= log_area.x + log_area.width - 1 ||
           y < log_area.y + 2 || y >= log_area.y + log_area.height - 1 {
            return false;
        }
        
        let content_start_y = log_area.y + 2;
        let relative_y = (y - content_start_y) as usize;
        let viewport_height = (log_area.height as usize).saturating_sub(3);
        
        // Rebuild cache if needed
        if !self.hash_line_cache.is_valid(self.scroll_offset, self.terminal_width, self.cache_generation) {
            self.hash_line_cache.rebuild(
                &self.log_entries,
                self.scroll_offset,
                self.terminal_width,
                viewport_height,
                self.show_timestamps
            );
        }
        
        // Get the hash for the clicked visual line
        if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(relative_y).cloned() {
            // Check if we're clicking on existing selection
            if let Some(ref existing_selection) = self.hash_selection {
                let start_visual = existing_selection.visual_start_line;
                let end_visual = existing_selection.visual_end_line;
                
                if relative_y >= start_visual && relative_y <= end_visual {
                    // Clicking inside existing selection - clear it
                    self.clear_selection();
                    self.add_system_log("🖱️ Clicked inside selection - cleared selection");
                    return true;
                }
            }
            
            // Start new hash-based selection
            self.hash_selection = Some(HashBasedSelection::new(line_hash.clone(), relative_y));
            self.selection_cursor = relative_y;
            
            self.add_system_log(&format!("🖱️ Hash selection started at visual line {} (log entry {})", 
                relative_y, line_hash.log_entry_index));
            return true;
        }
        
        self.add_system_log(&format!("❌ No hash found for visual line {}", relative_y));
        false
    }
    
    /// Hash-based mouse drag handler for perfect selection extension
    pub fn handle_mouse_drag(&mut self, x: u16, y: u16, log_area: Rect) -> bool {
        // Check if we have an active selection to extend
        let has_selection = self.hash_selection.is_some();
        if !has_selection {
            return false;
        }
        
        // Check bounds
        if x < log_area.x + 1 || x >= log_area.x + log_area.width - 1 {
            return false;
        }
        
        let content_start_y = log_area.y + 2;
        let content_end_y = log_area.y + log_area.height - 1;
        let viewport_height = (log_area.height as usize).saturating_sub(3);
        
        // Calculate drag position with edge handling
        let relative_y = if y < content_start_y {
            0 // Dragging above viewport
        } else if y >= content_end_y {
            viewport_height.saturating_sub(1) // Dragging below viewport
        } else {
            (y - content_start_y) as usize // Normal position
        };
        
        // Get hash for drag position
        if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(relative_y).cloned() {
            if let Some(ref mut selection) = self.hash_selection {
                // Start dragging if not already
                let was_dragging = selection.is_dragging;
                if !was_dragging {
                    selection.start_drag();
                }
                
                // Extend selection to new position
                let old_start = selection.visual_start_line;
                let old_end = selection.visual_end_line;
                selection.extend_to(line_hash, relative_y);
                
                // Log system messages after selection is updated
                let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
                
                // Drop the mutable reference before calling add_system_log
                drop(selection);
                
                if !was_dragging {
                    self.add_system_log("🖱️ Started hash-based dragging");
                }
                
                self.add_system_log(&format!("🖱️ Hash selection extended: {} lines ({}..{})", 
                    lines_selected, old_start.min(relative_y), old_end.max(relative_y)));
                return true;
            }
        }
        
        false
    }
    
    /// Handle mouse release to finalize hash-based selection
    pub fn handle_mouse_release(&mut self) -> bool {
        if let Some(ref mut selection) = self.hash_selection {
            if selection.is_dragging {
                selection.end_drag();
                let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
                self.add_system_log(&format!("📝 Hash selection complete: {} lines selected - Ctrl+C to copy", lines_selected));
                return true;
            }
        }
        false
    }
    
    /// Keyboard selection up with hash-based accuracy
    pub fn select_up(&mut self) {
        if let Some(ref mut selection) = self.hash_selection {
            if selection.visual_start_line > 0 {
                let new_visual_line = selection.visual_start_line - 1;
                
                // Get hash for new position
                if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(new_visual_line).cloned() {
                    selection.visual_start_line = new_visual_line;
                    selection.start_hash = Some(line_hash);
                    
                    // Move cursor up as well
                    if self.selection_cursor > 0 {
                        self.selection_cursor -= 1;
                    }
                    
                    let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
                    self.add_system_log(&format!("📝 Hash selection extended up: {} lines", lines_selected));
                }
            }
        }
    }
    
    /// Keyboard selection down with hash-based accuracy
    pub fn select_down(&mut self, viewport_height: usize) {
        if let Some(ref mut selection) = self.hash_selection {
            let max_visual_line = viewport_height.saturating_sub(1);
            if selection.visual_end_line < max_visual_line {
                let new_visual_line = selection.visual_end_line + 1;
                
                // Get hash for new position
                if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(new_visual_line).cloned() {
                    selection.visual_end_line = new_visual_line;
                    selection.end_hash = Some(line_hash);
                    
                    // Move cursor down as well
                    if self.selection_cursor < max_visual_line {
                        self.selection_cursor += 1;
                    }
                    
                    let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
                    self.add_system_log(&format!("📝 Hash selection extended down: {} lines", lines_selected));
                }
            }
        }
    }
    
    /// Toggle select all with hash-based accuracy
    pub fn toggle_selection_all(&mut self) {
        if self.hash_selection.is_some() {
            // Clear existing selection
            self.clear_selection();
        } else if !self.log_entries.is_empty() {
            // Create selection for all visible lines
            let viewport_height = 50; // Conservative estimate, will be corrected in render
            
            // Ensure cache is built
            if !self.hash_line_cache.is_valid(self.scroll_offset, self.terminal_width, self.cache_generation) {
                self.hash_line_cache.rebuild(
                    &self.log_entries,
                    self.scroll_offset,
                    self.terminal_width,
                    viewport_height,
                    self.show_timestamps
                );
            }
            
            // Get first and last line hashes
            if let (Some(start_hash), Some(end_hash)) = (
                self.hash_line_cache.get_hash_for_visual_line(0).cloned(),
                self.hash_line_cache.get_hash_for_visual_line(viewport_height.saturating_sub(1)).cloned()
            ) {
                self.hash_selection = Some(HashBasedSelection {
                    start_hash: Some(start_hash),
                    end_hash: Some(end_hash),
                    visual_start_line: 0,
                    visual_end_line: viewport_height.saturating_sub(1),
                    is_active: true,
                    is_dragging: false,
                    selection_direction: SelectionDirection::Forward,
                });
                
                self.add_system_log("📝 Selected all visible logs with hash accuracy - Ctrl+C to copy");
            }
        }
    }
    
    /// Clear hash-based selection
    pub fn clear_selection(&mut self) {
        self.hash_selection = None;
        self.selection_cursor = 0;
        self.add_system_log("📝 Hash selection cleared");
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
                    // Only adjust scroll when not in selection mode
                    if self.hash_selection.is_none() && self.scroll_offset > 0 {
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
        // Always insert if file output mode is enabled AND buffer not at 80% limit
        if self.file_output_mode {
            // In pause mode with enhanced buffer, stop inserting once we reach 90% of 5x limit
            if !self.auto_scroll && self.enhanced_buffer_active {
                let five_x_limit = self.original_max_lines * 5;
                let ninety_percent_limit = (five_x_limit as f64 * 0.9) as usize;
                if self.log_entries.len() >= ninety_percent_limit {
                    return false;
                }
            }
            return true;
        }
        
        // In pause mode with enhanced buffer active, stop at 90% of 5x capacity
        if !self.auto_scroll && self.enhanced_buffer_active {
            let five_x_limit = self.original_max_lines * 5;
            let ninety_percent_limit = (five_x_limit as f64 * 0.9) as usize;
            if self.log_entries.len() >= ninety_percent_limit {
                return false;
            }
            return true;
        }
        
        // Normal mode: Insert if buffer is not full 
        if self.log_entries.len() < self.max_lines {
            return true;
        }
        
        // Follow mode: Insert with rotation when following logs
        if self.auto_scroll {
            return true;
        }
        
        // Pause mode: Skip insertion if buffer is full
        false
    }

    pub fn add_log_entry(&mut self, entry: &LogEntry) -> bool {
        // Check if we should insert this entry into the display buffer
        if !self.should_insert_to_buffer() {
            return false;
        }

        // Enhanced buffer management: Use 5x expansion for pause modes
        let current_max_buffer = if self.enhanced_buffer_active {
            self.max_lines // Already expanded to 5x in enhanced mode
        } else {
            self.original_max_lines // Normal 1x buffer
        };

        // Calculate 90% threshold to ensure NO ROTATION after memory full
        let ninety_percent_threshold = if !self.auto_scroll && self.enhanced_buffer_active {
            let five_x_limit = self.original_max_lines * 5;
            (five_x_limit as f64 * 0.9) as usize
        } else {
            current_max_buffer
        };

        // Never rotate if we're at or above 90% threshold
        let should_rotate = if self.log_entries.len() >= ninety_percent_threshold {
            false
        } else if !self.auto_scroll && self.enhanced_buffer_active {
            self.log_entries.len() >= current_max_buffer
        } else {
            self.log_entries.len() >= current_max_buffer
        };

        // Smart buffer rotation: Only rotate when necessary AND allowed
        if should_rotate {
            let has_active_selection = self.hash_selection.is_some();
            
            if has_active_selection {
                self.add_system_log("⚠️ Buffer rotation with active selection - clearing selection to prevent corruption");
                self.clear_selection();
            }
            
            self.log_entries.pop_front();
            
            if self.hash_selection.is_none() && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(entry.clone());
        
        // Invalidate hash cache when new entry is added
        self.cache_generation += 1;
        
        self.filtered_logs += 1;
        
        // If auto-scroll is enabled, keep scroll at bottom
        if self.auto_scroll {
            let estimated_viewport = 20;
            let max_scroll = self.log_entries.len().saturating_sub(estimated_viewport);
            self.scroll_offset = max_scroll;
            self.update_display_window(estimated_viewport);
        }

        true
    }

    /// Get selected logs as text for clipboard copying using hash-based selection
    pub fn get_selected_logs_as_text(&self) -> String {
        if let Some(ref selection) = self.hash_selection {
            if selection.is_active {
                let mut result = String::new();
                
                // Use the hash-based cache to get exact selected lines
                for visual_line in selection.visual_start_line..=selection.visual_end_line {
                    if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(visual_line) {
                        if let Some((_log_entry_index, content)) = self.hash_line_cache.get_log_info_from_hash(line_hash) {
                            // For hash-based selection, we get the exact visual content
                            result.push_str(content);
                            result.push('\n');
                        }
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

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false; // Disable auto-scroll on manual scroll
        
        // Invalidate cache when scrolling
        self.cache_generation += 1;
        
        self.validate_scroll_bounds(50);
        self.update_display_window(50);
        
        // Update selection after scrolling
        self.update_selection_after_scroll(50);
    }

    pub fn scroll_down(&mut self, lines: usize, viewport_height: usize) {
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
        
        // Auto-enable auto-scroll when user manually scrolls to the bottom
        if self.scroll_offset >= max_scroll {
            self.auto_scroll = true;
        } else {
            self.auto_scroll = false;
        }
        
        // Invalidate cache when scrolling
        self.cache_generation += 1;
        
        self.validate_scroll_bounds(viewport_height);
        self.update_display_window(viewport_height);
        
        // Update selection after scrolling
        self.update_selection_after_scroll(viewport_height);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = false;
        
        // Invalidate cache when scrolling
        self.cache_generation += 1;
        
        self.update_display_window(20);
        
        // Update selection after scrolling
        self.update_selection_after_scroll(20);
    }

    pub fn scroll_to_bottom(&mut self, viewport_height: usize) {
        let max_scroll = self.log_entries.len().saturating_sub(viewport_height);
        self.scroll_offset = max_scroll;
        self.auto_scroll = true;
        
        // Invalidate cache when scrolling
        self.cache_generation += 1;
        
        self.update_display_window(viewport_height);
        
        // Update selection after scrolling
        self.update_selection_after_scroll(viewport_height);
    }

    /// Validate and fix scroll bounds to prevent corruption
    fn validate_scroll_bounds(&mut self, viewport_height: usize) {
        let max_possible_scroll = if self.log_entries.len() > viewport_height {
            self.log_entries.len() - viewport_height
        } else {
            0
        };

        if self.scroll_offset > max_possible_scroll + 100 {
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

    /// Create a colored line from a log entry with hash-based selection highlighting
    fn create_colored_log_line(&mut self, entry: &LogEntry, visual_line_index: usize) -> Line<'static> {
        let mut spans = Vec::new();

        // System messages (filter notifications) get special treatment
        if entry.message.starts_with("🔧") {
            let line = Line::from(Span::styled(
                entry.message.clone(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::ITALIC)
            ));
            
            // Apply selection highlighting if this line is selected
            return if self.is_line_selected_hash(visual_line_index) {
                self.apply_selection_highlight(line)
            } else {
                line
            };
        }

        // Clean fields of ANSI codes
        let clean_message = HashLineCache::strip_ansi_codes(&entry.message);
        let clean_pod_name = HashLineCache::strip_ansi_codes(&entry.pod_name);
        let clean_container_name = HashLineCache::strip_ansi_codes(&entry.container_name);

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

        let line = Line::from(spans);
        
        // Apply selection highlighting if this line is selected
        if self.is_line_selected_hash(visual_line_index) {
            self.apply_selection_highlight(line)
        } else {
            line
        }
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

    /// Check if a visual line is selected using hash-based selection
    fn is_line_selected_hash(&self, visual_line_index: usize) -> bool {
        if let Some(ref selection) = self.hash_selection {
            if selection.is_active {
                return visual_line_index >= selection.visual_start_line && 
                       visual_line_index <= selection.visual_end_line;
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
            if self.hash_selection.is_none() && self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        
        self.log_entries.push_back(system_entry);
        
        // Invalidate cache when system message is added
        self.cache_generation += 1;
    }

    /// Add a debug system message that only appears when dev mode is enabled
    pub fn add_system_log(&mut self, message: &str) {
        if self.dev_mode {
            self.add_system_message(message);
        }
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

        let clean_message = HashLineCache::strip_ansi_codes(&entry.message);
        let clean_pod_name = HashLineCache::strip_ansi_codes(&entry.pod_name);
        let clean_container_name = HashLineCache::strip_ansi_codes(&entry.container_name);

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
        
        // Clear hash-based caches
        self.hash_line_cache = HashLineCache::new();
        self.hash_selection = None;
        
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

    /// Update selection visual line indices after scrolling
    fn update_selection_after_scroll(&mut self, viewport_height: usize) {
        if let Some(ref mut selection) = self.hash_selection {
            // Rebuild cache if needed for current scroll position
            if !self.hash_line_cache.is_valid(self.scroll_offset, self.terminal_width, self.cache_generation) {
                self.hash_line_cache.rebuild(
                    &self.log_entries,
                    self.scroll_offset,
                    self.terminal_width,
                    viewport_height,
                    self.show_timestamps
                );
            }

            // Find new visual positions for the selected hashes
            let mut new_start_line = None;
            let mut new_end_line = None;

            // Search through the current viewport for our hashes
            for visual_index in 0..viewport_height {
                if let Some(line_hash) = self.hash_line_cache.get_hash_for_visual_line(visual_index) {
                    // Check if this hash matches our selection start
                    if let Some(ref start_hash) = selection.start_hash {
                        if line_hash == start_hash {
                            new_start_line = Some(visual_index);
                        }
                    }
                    
                    // Check if this hash matches our selection end
                    if let Some(ref end_hash) = selection.end_hash {
                        if line_hash == end_hash {
                            new_end_line = Some(visual_index);
                        }
                    }
                }
            }

            match (new_start_line, new_end_line) {
                (Some(start), Some(end)) => {
                    // Both hashes found in current viewport - update visual indices
                    selection.visual_start_line = start.min(end);
                    selection.visual_end_line = start.max(end);
                    
                    let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
                    self.add_system_log(&format!("📝 Selection updated after scroll: {} lines visible", lines_selected));
                }
                (Some(visible), None) | (None, Some(visible)) => {
                    // Only one hash visible in current viewport - adjust selection
                    selection.visual_start_line = visible;
                    selection.visual_end_line = visible;
                    self.add_system_log("📝 Selection partially visible after scroll - adjusted to visible portion");
                }
                (None, None) => {
                    // Neither hash visible in current viewport - clear selection
                    self.add_system_log("📝 Selection scrolled out of view - clearing selection");
                    self.hash_selection = None;
                    self.selection_cursor = 0;
                }
            }
        }
    }

    /// Render the display manager UI
    pub fn render(&mut self, f: &mut Frame, input_handler: &InputHandler) {
        use ratatui::{
            layout::{Constraint, Direction, Layout, Rect},
            style::{Color, Modifier, Style},
            text::{Line, Span, Text},
            widgets::{Block, Borders, Clear, Paragraph, Wrap},
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Filter input area (now 4 lines: header + filters)
                Constraint::Min(0),    // Log display area
                Constraint::Length(1), // Status bar
            ])
            .split(f.size());

        // Update terminal width for hash cache
        let new_width = f.size().width as usize;
        self.update_terminal_width(new_width);

        // Filter input area
        self.render_filter_input(f, input_handler, chunks[0]);

        // Log display area
        self.render_logs(f, chunks[1]);

        // Status bar
        self.render_status_bar(f, chunks[2]);

        // Memory warning popup (if active)
        if self.memory_warning_active {
            self.render_memory_warning(f);
        }

        // Help overlay (if active)
        if input_handler.mode == InputMode::Help {
            self.render_help_overlay(f, input_handler);
        }
    }

    /// Render the filter input area with modern styling and improved design
    fn render_filter_input(&self, f: &mut Frame, input_handler: &InputHandler, area: Rect) {
        // Create a more sophisticated layout with better proportions
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header line
                Constraint::Length(3), // Filter inputs
            ])
            .split(area);

        // Render header with visual connection indicator
        self.render_filter_header(f, input_handler, main_layout[0]);

        // Split filter area with better proportions and visual separation
        let filter_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(48), // Include filter
                Constraint::Length(4),      // Visual separator
                Constraint::Percentage(48), // Exclude filter
            ])
            .split(main_layout[1]);

        // Render include filter with enhanced design
        self.render_include_filter(f, input_handler, filter_chunks[0]);

        // Render visual connector between filters
        self.render_filter_connector(f, input_handler, filter_chunks[1]);

        // Render exclude filter with enhanced design
        self.render_exclude_filter(f, input_handler, filter_chunks[2]);
    }

    /// Render the filter header showing the logical relationship
    fn render_filter_header(&self, f: &mut Frame, input_handler: &InputHandler, area: Rect) {
        let active_filters = (!input_handler.include_input.is_empty()) as u8 + 
                           (!input_handler.exclude_input.is_empty()) as u8;
        
        let header_text = match active_filters {
            0 => "📋 Log Filters: All logs shown (no filters active)",
            1 => "📋 Log Filters: One filter active",
            2 => "📋 Log Filters: Include AND NOT Exclude logic active",
            _ => "📋 Log Filters",
        };

        let header_style = Style::default()
            .fg(match active_filters {
                0 => self.color_scheme.secondary_text(),
                1 => self.color_scheme.accent_color(),
                2 => self.color_scheme.success_color(),
                _ => self.color_scheme.primary_text(),
            })
            .add_modifier(if active_filters > 0 { Modifier::BOLD } else { Modifier::empty() });

        let header = Paragraph::new(header_text)
            .style(header_style)
            .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(header, area);
    }

    /// Render the include filter with enhanced visual design
    fn render_include_filter(&self, f: &mut Frame, input_handler: &InputHandler, area: Rect) {
        let is_active = input_handler.mode == InputMode::EditingInclude;
        let has_content = !input_handler.include_input.is_empty();

        // Dynamic title with status indicators
        let title = match (is_active, has_content) {
            (true, _) => "🔍 INCLUDE (editing) ✏️",
            (false, true) => "🔍 INCLUDE ✅",
            (false, false) => "🔍 INCLUDE",
        };

        // Enhanced styling based on state
        let border_style = if is_active {
            Style::default()
                .fg(self.color_scheme.accent_color())
                .add_modifier(Modifier::BOLD)
        } else if has_content {
            Style::default()
                .fg(self.color_scheme.success_color())
        } else {
            Style::default().fg(self.color_scheme.border_color())
        };

        let text_style = if is_active {
            Style::default()
                .fg(self.color_scheme.accent_color())
                .add_modifier(Modifier::BOLD)
        } else if has_content {
            Style::default()
                .fg(self.color_scheme.primary_text())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.color_scheme.secondary_text())
        };

        // Create block with enhanced visual hierarchy
        let include_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title,
                Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD)
            ));

        // Show placeholder text when empty
        let display_text = if input_handler.include_input.is_empty() {
            if is_active {
                "Type to show only matching logs..."
            } else {
                "Press 'i' to add include filter"
            }
        } else {
            &input_handler.include_input
        };

        let include_input = Paragraph::new(display_text)
            .block(include_block)
            .style(if input_handler.include_input.is_empty() && !is_active {
                Style::default().fg(self.color_scheme.secondary_text()).add_modifier(Modifier::ITALIC)
            } else {
                text_style
            });

        f.render_widget(include_input, area);

        // Render cursor for active editing
        if is_active {
            let cursor_x = area.x + 1 + input_handler.cursor_position as u16;
            let cursor_y = area.y + 1;
            f.set_cursor(cursor_x, cursor_y);
        }
    }

    /// Render visual connector showing the logical relationship with Wake branding
    fn render_filter_connector(&self, f: &mut Frame, input_handler: &InputHandler, area: Rect) {
        let has_include = !input_handler.include_input.is_empty();
        let has_exclude = !input_handler.exclude_input.is_empty();

        let connector_text = match (has_include, has_exclude) {
            (true, true) => vec![
                Line::from(Span::styled("WAKE", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(" AND", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(" NOT", Style::default()
                    .fg(self.color_scheme.warning_color())
                    .add_modifier(Modifier::BOLD))),
            ],
            (true, false) => vec![
                Line::from(Span::styled("WAKE", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(" →  ", Style::default()
                    .fg(self.color_scheme.secondary_text()))),
                Line::from(Span::styled("    ", Style::default())),
            ],
            (false, true) => vec![
                Line::from(Span::styled("WAKE", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(" NOT", Style::default()
                    .fg(self.color_scheme.warning_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled("    ", Style::default())),
            ],
            (false, false) => vec![
                Line::from(Span::styled("WAKE", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
                Line::from(Span::styled("════", Style::default()
                    .fg(self.color_scheme.secondary_text()))),
                Line::from(Span::styled("LOGS", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD))),
            ],
        };

        let connector = Paragraph::new(connector_text)
            .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(connector, area);
    }

    /// Render the exclude filter with enhanced visual design
    fn render_exclude_filter(&self, f: &mut Frame, input_handler: &InputHandler, area: Rect) {
        let is_active = input_handler.mode == InputMode::EditingExclude;
        let has_content = !input_handler.exclude_input.is_empty();

        // Dynamic title with status indicators
        let title = match (is_active, has_content) {
            (true, _) => "🚫 EXCLUDE (editing) ✏️",
            (false, true) => "🚫 EXCLUDE ✅",
            (false, false) => "🚫 EXCLUDE",
        };

        // Enhanced styling based on state
        let border_style = if is_active {
            Style::default()
                .fg(self.color_scheme.warning_color())
                .add_modifier(Modifier::BOLD)
        } else if has_content {
            Style::default()
                .fg(self.color_scheme.error_color())
        } else {
            Style::default().fg(self.color_scheme.border_color())
        };

        let text_style = if is_active {
            Style::default()
                .fg(self.color_scheme.warning_color())
                .add_modifier(Modifier::BOLD)
        } else if has_content {
            Style::default()
                .fg(self.color_scheme.primary_text())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.color_scheme.secondary_text())
        };

        // Create block with enhanced visual hierarchy
        let exclude_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                title,
                Style::default()
                    .fg(self.color_scheme.warning_color())
                    .add_modifier(Modifier::BOLD)
            ));

        // Show placeholder text when empty
        let display_text = if input_handler.exclude_input.is_empty() {
            if is_active {
                "Type to hide matching logs..."
            } else {
                "Press 'e' to add exclude filter"
            }
        } else {
            &input_handler.exclude_input
        };

        let exclude_input = Paragraph::new(display_text)
            .block(exclude_block)
            .style(if input_handler.exclude_input.is_empty() && !is_active {
                Style::default().fg(self.color_scheme.secondary_text()).add_modifier(Modifier::ITALIC)
            } else {
                text_style
            });

        f.render_widget(exclude_input, area);

        // Render cursor for active editing
        if is_active {
            let cursor_x = area.x + 1 + input_handler.cursor_position as u16;
            let cursor_y = area.y + 1;
            f.set_cursor(cursor_x, cursor_y);
        }
    }

    /// Render the main log display area with modern styling
    fn render_logs(&mut self, f: &mut Frame, area: Rect) {
        let viewport_height = (area.height as usize).saturating_sub(3);
        
        // Rebuild hash cache if needed
        if !self.hash_line_cache.is_valid(self.scroll_offset, self.terminal_width, self.cache_generation) {
            self.hash_line_cache.rebuild(
                &self.log_entries,
                self.scroll_offset,
                self.terminal_width,
                viewport_height,
                self.show_timestamps
            );
        }

        // Create log lines with hash-based selection highlighting
        let mut log_lines = Vec::new();
        let visible_entries: Vec<(usize, LogEntry)> = self.log_entries
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(viewport_height)
            .map(|(i, entry)| (i, entry.clone()))
            .collect();

        for (visual_line_index, (_, entry)) in visible_entries.iter().enumerate() {
            let colored_line = self.create_colored_log_line(entry, visual_line_index);
            log_lines.push(colored_line);
        }

        // Enhanced title with modern icons and status indicators
        let mode_indicator = if self.auto_scroll { 
            "▶️ FOLLOW" 
        } else { 
            "⏸️ PAUSE" 
        };
        
        let selection_info = if let Some(ref selection) = self.hash_selection {
            let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
            format!(" │ 📝 {} selected", lines_selected)
        } else {
            String::new()
        };

        let memory_indicator = if !self.auto_scroll && self.is_memory_critical() {
            " │ ⚠️ Memory High"
        } else {
            ""
        };

        let title = format!("📋 Kubernetes Logs [{}{}{}] ({}/{})", 
            mode_indicator, 
            selection_info,
            memory_indicator,
            self.filtered_logs, 
            self.log_entries.len()
        );

        // Enhanced block styling with modern borders
        let logs_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.color_scheme.border_color()))
            .title(Span::styled(
                title, 
                Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD)
            ));

        let logs_paragraph = Paragraph::new(log_lines)
            .block(logs_block)
            .style(Style::default().fg(self.color_scheme.primary_text()))
            .wrap(Wrap { trim: false });

        f.render_widget(logs_paragraph, area);
    }

    /// Render the status bar with modern design
    fn render_status_bar(&self, f: &mut Frame, area: Rect) {
        // Modern gradient-like status bar styling
        let status_style = Style::default()
            .bg(self.color_scheme.panel_bg())
            .fg(self.color_scheme.primary_text());
        
        // Build status message with modern separators
        let mut status_spans = Vec::new();
        
        // Mode indicator with icon
        let mode_icon = if self.auto_scroll { "▶️" } else { "⏸️" };
        let mode_color = if self.auto_scroll { 
            self.color_scheme.success_color() 
        } else { 
            self.color_scheme.warning_color() 
        };
        
        status_spans.push(Span::styled(
            format!("{} {}", mode_icon, if self.auto_scroll { "FOLLOW" } else { "PAUSE" }),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD)
        ));
        
        // Modern separator
        status_spans.push(Span::styled(" │ ", Style::default().fg(self.color_scheme.secondary_text())));
        
        // Memory usage indicator (only in pause mode)
        if !self.auto_scroll {
            let memory_percent = self.get_memory_usage_percent();
            let memory_color = if self.is_memory_critical() {
                self.color_scheme.error_color()
            } else if memory_percent > 60.0 {
                self.color_scheme.warning_color()
            } else {
                self.color_scheme.success_color()
            };
            
            let memory_icon = if self.is_memory_critical() { "⚠️" } else { "💾" };
            status_spans.push(Span::styled(
                format!("{} {:.0}%", memory_icon, memory_percent),
                Style::default().fg(memory_color).add_modifier(Modifier::BOLD)
            ));
            
            status_spans.push(Span::styled(" │ ", Style::default().fg(self.color_scheme.secondary_text())));
        }
        
        // Selection indicator with icon
        if let Some(ref selection) = self.hash_selection {
            let lines_selected = (selection.visual_end_line - selection.visual_start_line) + 1;
            status_spans.push(Span::styled(
                format!("📝 {} lines", lines_selected),
                Style::default().fg(self.color_scheme.accent_color()).add_modifier(Modifier::BOLD)
            ));
            
            status_spans.push(Span::styled(" │ ", Style::default().fg(self.color_scheme.secondary_text())));
        }
        
        // Buffer size with icon
        let buffer_percent = (self.log_entries.len() as f64 / self.max_lines as f64) * 100.0;
        let buffer_color = if buffer_percent > 80.0 {
            self.color_scheme.warning_color()
        } else {
            self.color_scheme.secondary_text()
        };
        
        status_spans.push(Span::styled(
            format!("📊 {}/{}", self.log_entries.len(), self.max_lines),
            Style::default().fg(buffer_color)
        ));
        
        status_spans.push(Span::styled(" │ ", Style::default().fg(self.color_scheme.secondary_text())));
        
        // Scroll position with dynamic icon
        if !self.log_entries.is_empty() {
            let scroll_percent = if self.auto_scroll {
                100.0
            } else {
                (self.scroll_offset as f64 / self.log_entries.len().max(1) as f64) * 100.0
            };
            
            let scroll_icon = if scroll_percent >= 100.0 {
                "⬇️"
            } else if scroll_percent <= 0.0 {
                "⬆️"
            } else {
                "📜"
            };
            
            status_spans.push(Span::styled(
                format!("{} {:.0}%", scroll_icon, scroll_percent),
                Style::default().fg(self.color_scheme.secondary_text())
            ));
            
            status_spans.push(Span::styled(" │ ", Style::default().fg(self.color_scheme.secondary_text())));
        }
        
        // Help hint with icon
        status_spans.push(Span::styled(
            "❓ Press 'h' for help",
            Style::default().fg(self.color_scheme.secondary_text()).add_modifier(Modifier::ITALIC)
        ));
        
        let status_line = Line::from(status_spans);
        let status_paragraph = Paragraph::new(vec![status_line])
            .style(status_style)
            .alignment(ratatui::layout::Alignment::Left);
        
        f.render_widget(status_paragraph, area);
    }

    /// Render modern memory warning popup with enhanced styling
    fn render_memory_warning(&self, f: &mut Frame) {
        let area = f.size();
        
        // Calculate popup size based on terminal size
        let popup_width = (area.width as f32 * 0.6).max(50.0) as u16;
        let popup_height = 12;
        
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        // Clear the background with shadow effect
        f.render_widget(Clear, popup_area);

        let memory_percent = self.get_memory_usage_percent();
        
        // Create warning content with modern formatting
        let warning_text = vec![
            Line::from(vec![
                Span::styled("⚠️  ", Style::default().fg(self.color_scheme.warning_color())),
                Span::styled("MEMORY WARNING", Style::default()
                    .fg(self.color_scheme.warning_color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
                Span::styled("  ⚠️", Style::default().fg(self.color_scheme.warning_color())),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("📊 Buffer usage: ", Style::default().fg(self.color_scheme.secondary_text())),
                Span::styled(format!("{:.1}%", memory_percent), Style::default()
                    .fg(self.color_scheme.error_color())
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("📈 Current entries: ", Style::default().fg(self.color_scheme.secondary_text())),
                Span::styled(format!("{}", self.log_entries.len()), Style::default()
                    .fg(self.color_scheme.primary_text())
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("🔧 Available Actions:", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ▶️  Press ", Style::default().fg(self.color_scheme.secondary_text())),
                Span::styled("'f'", Style::default()
                    .fg(self.color_scheme.success_color())
                    .add_modifier(Modifier::BOLD)),
                Span::styled(" to enable follow mode", Style::default().fg(self.color_scheme.secondary_text())),
            ]),
            Line::from(vec![
                Span::styled("  🔄 Press ", Style::default().fg(self.color_scheme.secondary_text())),
                Span::styled("any key", Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD)),
                Span::styled(" to dismiss this warning", Style::default().fg(self.color_scheme.secondary_text())),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("💡 Tip: Follow mode automatically manages memory", Style::default()
                    .fg(self.color_scheme.secondary_text())
                    .add_modifier(Modifier::ITALIC)),
            ]),
        ];

        // Enhanced warning block with modern styling
        let warning_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default()
                .fg(self.color_scheme.warning_color())
                .add_modifier(Modifier::BOLD))
            .title(Span::styled(
                " ⚠️ System Alert ⚠️ ", 
                Style::default()
                    .fg(self.color_scheme.warning_color())
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            ));

        let warning_paragraph = Paragraph::new(warning_text)
            .block(warning_block)
            .style(Style::default()
                .bg(self.color_scheme.panel_bg())
                .fg(self.color_scheme.primary_text()))
            .wrap(Wrap { trim: false })
            .alignment(ratatui::layout::Alignment::Left);

        f.render_widget(warning_paragraph, popup_area);
    }

    /// Render modern help overlay with enhanced navigation
    fn render_help_overlay(&self, f: &mut Frame, input_handler: &InputHandler) {
        let area = f.size();
        
        // Calculate help area size
        let help_width = (area.width as f32 * 0.9).max(80.0) as u16;
        let help_height = area.height.saturating_sub(6);
        
        let help_area = Rect {
            x: (area.width.saturating_sub(help_width)) / 2,
            y: 3,
            width: help_width,
            height: help_height,
        };

        // Clear the background
        f.render_widget(Clear, help_area);

        // Create enhanced help content with modern formatting
        let help_lines: Vec<Line> = input_handler.get_help_text()
            .iter()
            .map(|&line| {
                if line.starts_with("=") {
                    // Section headers
                    Line::from(Span::styled(
                        line.replace("=", ""),
                        Style::default()
                            .fg(self.color_scheme.accent_color())
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                    ))
                } else if line.contains(":") && !line.starts_with(" ") {
                    // Key bindings
                    let parts: Vec<&str> = line.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Line::from(vec![
                            Span::styled(format!("  {} ", parts[0]), Style::default()
                                .fg(self.color_scheme.success_color())
                                .add_modifier(Modifier::BOLD)),
                            Span::styled(parts[1], Style::default()
                                .fg(self.color_scheme.secondary_text())),
                        ])
                    } else {
                        Line::from(Span::styled(line, Style::default().fg(self.color_scheme.primary_text())))
                    }
                } else if line.trim().is_empty() {
                    Line::from("")
                } else {
                    // Regular text
                    Line::from(Span::styled(
                        line,
                        Style::default().fg(self.color_scheme.secondary_text())
                    ))
                }
            })
            .collect();

        // Enhanced help block with modern styling
        let help_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default()
                .fg(self.color_scheme.accent_color())
                .add_modifier(Modifier::BOLD))
            .title(Span::styled(
                " 📖 Wake - Kubernetes Log Viewer Help ", 
                Style::default()
                    .fg(self.color_scheme.accent_color())
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            ));

        let help_paragraph = Paragraph::new(help_lines)
            .block(help_block)
            .style(Style::default().fg(self.color_scheme.primary_text()))
            .wrap(Wrap { trim: false })
            .alignment(ratatui::layout::Alignment::Left);

        f.render_widget(help_paragraph, help_area);
    }
}