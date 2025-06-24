use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum InputEvent {
    Quit,
    ToggleAutoScroll,
    Refresh,
    ToggleHelp,
    UpdateIncludeFilter(String),
    UpdateExcludeFilter(String),
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    CopyLogs,
    CopySelection,         // Copy selected text
    ToggleMouseCapture,
    MouseClick(u16, u16),
    MouseDrag(u16, u16),
    MouseRelease(u16, u16),
    SelectUp,              // Extend selection up with arrow keys
    SelectDown,            // Extend selection down with arrow keys
    ToggleSelection,       // Toggle selection at current position
    SelectAll,             // Select all logs
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EditingInclude,
    EditingExclude,
    Help,
}

pub struct InputHandler {
    pub mode: InputMode,
    pub include_input: String,
    pub exclude_input: String,
    pub cursor_position: usize,
    pub input_history: VecDeque<String>,
    pub history_index: Option<usize>,
}

impl InputHandler {
    pub fn new(initial_include: Option<String>, initial_exclude: Option<String>) -> Self {
        Self {
            mode: InputMode::Normal,
            include_input: initial_include.unwrap_or_default(),
            exclude_input: initial_exclude.unwrap_or_default(),
            cursor_position: 0,
            input_history: VecDeque::with_capacity(50),
            history_index: None,
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<InputEvent> {
        match self.mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::EditingInclude => self.handle_editing_mode(key, true),
            InputMode::EditingExclude => self.handle_editing_mode(key, false),
            InputMode::Help => self.handle_help_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) -> Option<InputEvent> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(InputEvent::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Smart copy: if there's a selection, copy it; otherwise copy visible logs
                Some(InputEvent::CopySelection)
            }
            KeyCode::Char('i') => {
                self.mode = InputMode::EditingInclude;
                self.cursor_position = self.include_input.len();
                None
            }
            KeyCode::Char('e') => {
                self.mode = InputMode::EditingExclude;
                self.cursor_position = self.exclude_input.len();
                None
            }
            KeyCode::Char('h') => Some(InputEvent::ToggleHelp),
            KeyCode::Char('r') => Some(InputEvent::Refresh),
            // Arrow keys for selection extension when Shift is held, otherwise normal scroll
            KeyCode::Up | KeyCode::Char('k') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    Some(InputEvent::SelectUp) // Selection handled in display manager based on auto_scroll
                } else {
                    Some(InputEvent::ScrollUp)
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    Some(InputEvent::SelectDown) // Selection handled in display manager based on auto_scroll
                } else {
                    Some(InputEvent::ScrollDown)
                }
            }
            KeyCode::Home | KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::ScrollToTop)
            }
            KeyCode::End | KeyCode::Char('G') => Some(InputEvent::ScrollToBottom),
            KeyCode::PageUp => Some(InputEvent::ScrollPageUp),
            KeyCode::PageDown => Some(InputEvent::ScrollPageDown),
            KeyCode::Char('f') => Some(InputEvent::ToggleAutoScroll), // Add 'f' key to toggle follow/auto-scroll mode
            KeyCode::Char('m') => {
                // Toggle mouse capture mode
                Some(InputEvent::ToggleMouseCapture)
            }
            // Add space key to toggle selection
            KeyCode::Char(' ') => Some(InputEvent::ToggleSelection), // Will be handled in display manager based on auto_scroll
            // Selection shortcuts - only work in pause mode
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::SelectAll) // Ctrl+A to select all logs
            }
            _ => None,
        }
    }

    fn handle_editing_mode(&mut self, key: KeyEvent, is_include: bool) -> Option<InputEvent> {
        match key.code {
            KeyCode::Enter => {
                self.mode = InputMode::Normal;
                let input_value = if is_include {
                    self.include_input.clone()
                } else {
                    self.exclude_input.clone()
                };
                self.add_to_history(input_value.clone());
                if is_include {
                    Some(InputEvent::UpdateIncludeFilter(input_value))
                } else {
                    Some(InputEvent::UpdateExcludeFilter(input_value))
                }
            }
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                None
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    let current_input = if is_include {
                        &mut self.include_input
                    } else {
                        &mut self.exclude_input
                    };
                    current_input.remove(self.cursor_position - 1);
                    self.cursor_position -= 1;
                }
                None
            }
            KeyCode::Delete => {
                let current_input = if is_include {
                    &mut self.include_input
                } else {
                    &mut self.exclude_input
                };
                if self.cursor_position < current_input.len() {
                    current_input.remove(self.cursor_position);
                }
                None
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
                None
            }
            KeyCode::Right => {
                let current_len = if is_include {
                    self.include_input.len()
                } else {
                    self.exclude_input.len()
                };
                if self.cursor_position < current_len {
                    self.cursor_position += 1;
                }
                None
            }
            KeyCode::Home => {
                self.cursor_position = 0;
                None
            }
            KeyCode::End => {
                self.cursor_position = if is_include {
                    self.include_input.len()
                } else {
                    self.exclude_input.len()
                };
                None
            }
            KeyCode::Up => {
                self.navigate_history(true, is_include);
                None
            }
            KeyCode::Down => {
                self.navigate_history(false, is_include);
                None
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = InputMode::Normal;
                Some(InputEvent::Quit)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let current_input = if is_include {
                    &mut self.include_input
                } else {
                    &mut self.exclude_input
                };
                current_input.clear();
                self.cursor_position = 0;
                None
            }
            KeyCode::Char(c) => {
                let current_input = if is_include {
                    &mut self.include_input
                } else {
                    &mut self.exclude_input
                };
                current_input.insert(self.cursor_position, c);
                self.cursor_position += 1;
                None
            }
            _ => None,
        }
    }

    fn handle_help_mode(&mut self, key: KeyEvent) -> Option<InputEvent> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                self.mode = InputMode::Normal;
                None
            }
            _ => None,
        }
    }

    fn add_to_history(&mut self, input: String) {
        if !input.is_empty() && self.input_history.front() != Some(&input) {
            self.input_history.push_front(input);
            if self.input_history.len() > 50 {
                self.input_history.pop_back();
            }
        }
        self.history_index = None;
    }

    fn navigate_history(&mut self, up: bool, is_include: bool) {
        if self.input_history.is_empty() {
            return;
        }

        let current_input = if is_include {
            &mut self.include_input
        } else {
            &mut self.exclude_input
        };

        match self.history_index {
            None => {
                if up {
                    self.history_index = Some(0);
                    if let Some(item) = self.input_history.get(0) {
                        *current_input = item.clone();
                        self.cursor_position = current_input.len();
                    }
                }
            }
            Some(index) => {
                if up && index + 1 < self.input_history.len() {
                    self.history_index = Some(index + 1);
                    if let Some(item) = self.input_history.get(index + 1) {
                        *current_input = item.clone();
                        self.cursor_position = current_input.len();
                    }
                } else if !up && index > 0 {
                    self.history_index = Some(index - 1);
                    if let Some(item) = self.input_history.get(index - 1) {
                        *current_input = item.clone();
                        self.cursor_position = current_input.len();
                    }
                } else if !up && index == 0 {
                    self.history_index = None;
                    current_input.clear();
                    self.cursor_position = 0;
                }
            }
        }
    }

    pub fn get_help_text(&self) -> Vec<&'static str> {
        vec![
            "=== WAKE - Kubernetes Log Viewer Help ===",
            "",
            "📋 Filter System:",
            "  i: Edit include filter (show only matching logs)",
            "  e: Edit exclude filter (hide matching logs)",
            "  Filter Logic: INCLUDE AND NOT EXCLUDE",
            "  • Include empty = show all logs",
            "  • Exclude empty = hide nothing", 
            "  • Both active = show logs that match include but don't match exclude",
            "",
            "📖 Navigation:",
            "  ↑/↓ or j/k: Scroll up/down one line",
            "  Page Up/Down: Scroll by page",
            "  Home/Ctrl+g: Go to top",
            "  End/G: Go to bottom",
            "  f: Toggle follow mode (auto-scroll)",
            "",
            "📝 Selection (Pause mode only):",
            "  Mouse: Click and drag to select lines",
            "  Shift+↑/↓: Extend selection up/down",
            "  Ctrl+A: Select all visible logs",
            "  Ctrl+C: Copy selected logs to clipboard",
            "  Esc: Clear selection",
            "",
            "🎛️ Display Controls:",
            "  t: Toggle timestamps",
            "  r: Refresh display",
            "  Ctrl+L: Clear screen",
            "",
            "❓ General:",
            "  h: Toggle this help screen",
            "  q/Esc: Quit application",
            "",
            "💡 Mode Indicators:",
            "  ▶️ FOLLOW: Auto-scrolls, shows new logs as they arrive",
            "  ⏸️ PAUSE: Manual control, enables selection and browsing",
            "",
            "🔍 Advanced Filter Examples:",
            "  Basic text: 'error'                     - Show logs containing 'error'",
            "  Regex: 'ERROR|WARN'                     - Show ERROR or WARN logs",
            "  Complex: '\"user.*login\" && \"success\"'  - User login success logs",
            "  Exclude: 'debug'                        - Hide all debug messages",
            "",
            "📁 File Output (when -w flag used):",
            "  • All logs are saved to file regardless of filters",
            "  • Filters only affect what you see in the UI",
            "  • File contains complete, unfiltered log stream",
            "",
            "🚀 Performance Tips:",
            "  • Use follow mode for real-time monitoring",
            "  • Switch to pause mode for detailed analysis",
            "  • Buffer auto-expands in pause mode for better history",
            "  • Complex regex patterns may impact performance",
            "",
            "💾 Memory Management:",
            "  • Follow mode: Automatic buffer rotation (efficient)",
            "  • Pause mode: 5x buffer expansion for selection",
            "  • Warning shown at 80% buffer capacity",
            "  • Switch to follow mode to free memory",
            "",
            "🎨 Visual Design:",
            "  • Color-coded pods and containers for easy identification",
            "  • Smart highlighting for log levels (ERROR, WARN, INFO, etc.)",
            "  • Visual filter status indicators",
            "  • Modern terminal UI with intuitive navigation",
            "",
            "Example Workflow:",
            "  1. Start with 'wake --ui --namespace my-app'",
            "  2. Press 'i' and type 'ERROR' to see only errors",
            "  3. Press 'f' to pause and enable selection",
            "  4. Select interesting logs with mouse or keyboard",
            "  5. Press Ctrl+C to copy logs for sharing",
            "",
            "Press 'h' again or 'q' to close this help screen.",
        ]
    }
}