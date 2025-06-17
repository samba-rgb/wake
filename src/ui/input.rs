use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum InputEvent {
    UpdateIncludeFilter(String),
    UpdateExcludeFilter(String),
    ToggleHelp,
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    ScrollPageUp,
    ScrollPageDown,
    ToggleAutoScroll,
    Quit,
    Refresh,
    CopyLogs,
    CopySelection,
    ToggleSelection,
    SelectUp,
    SelectDown,
    EnterSelectionMode,
    ExitSelectionMode,
    ToggleMouseCapture,
    #[allow(dead_code)]
    MouseClick(u16, u16),
    #[allow(dead_code)]
    MouseDrag(u16, u16),
    #[allow(dead_code)]
    MouseRelease(u16, u16),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EditingInclude,
    EditingExclude,
    Help,
    Selection,
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
            InputMode::Selection => self.handle_selection_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) -> Option<InputEvent> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(InputEvent::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::CopyLogs)
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
            KeyCode::Up | KeyCode::Char('k') => Some(InputEvent::ScrollUp),
            KeyCode::Down | KeyCode::Char('j') => Some(InputEvent::ScrollDown),
            KeyCode::Home | KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::ScrollToTop)
            }
            KeyCode::End | KeyCode::Char('G') => Some(InputEvent::ScrollToBottom),
            KeyCode::PageUp => Some(InputEvent::ScrollPageUp),
            KeyCode::PageDown => Some(InputEvent::ScrollPageDown),
            KeyCode::Char('f') => Some(InputEvent::ToggleAutoScroll), // Add 'f' key to toggle follow/auto-scroll mode
            KeyCode::Char('s') => {
                self.mode = InputMode::Selection;
                // Return event to trigger buffer expansion when entering selection mode
                Some(InputEvent::EnterSelectionMode)
            }
            KeyCode::Char('m') => {
                // Toggle mouse capture mode
                Some(InputEvent::ToggleMouseCapture)
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

    fn handle_selection_mode(&mut self, key: KeyEvent) -> Option<InputEvent> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => {
                self.mode = InputMode::Normal;
                Some(InputEvent::ExitSelectionMode)
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(InputEvent::CopySelection)
            }
            KeyCode::Up | KeyCode::Char('k') => Some(InputEvent::SelectUp),
            KeyCode::Down | KeyCode::Char('j') => Some(InputEvent::SelectDown),
            KeyCode::Char('x') => Some(InputEvent::ToggleSelection),
            KeyCode::Char('r') => Some(InputEvent::Refresh),
            KeyCode::Char('f') => Some(InputEvent::ToggleAutoScroll),
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
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            "                                  WAKE - Help                                    ",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            "",
            "  Navigation:",
            "    ↑/k         Scroll up                    ↓/j         Scroll down",
            "    Page Up     Scroll up (page)             Page Down   Scroll down (page)",
            "    Home/Ctrl+g Go to top                    End/G       Go to bottom",
            "    f           Toggle auto-scroll (follow mode)",
            "",
            "  Filtering:",
            "    i           Edit include pattern         e           Edit exclude pattern",
            "    r           Refresh with current filters",
            "",
            "  Selection & Copying:",
            "    s           Enter selection mode         x           Toggle selection at cursor",
            "    Ctrl+c      Copy visible logs            Ctrl+c      Copy selection (in selection mode)",
            "",
            "  In Selection Mode:",
            "    ↑/k         Move cursor up & extend      ↓/j         Move cursor down & extend",
            "    x           Toggle selection start/end   Esc/s       Exit selection mode",
            "    Ctrl+c      Copy selected lines",
            "",
            "  While editing filters:",
            "    Enter       Apply filter                 Esc         Cancel editing",
            "    ↑/↓         Navigate filter history      Ctrl+u      Clear input",
            "    ←/→         Move cursor                   Home/End    Start/End of line",
            "",
            "  General:",
            "    h           Toggle this help             q/Esc       Quit application",
            "    m           Toggle mouse capture mode",
            "",
            "  Mouse Modes:",
            "    • Mouse capture OFF (default): Terminal text selection works normally",
            "    • Mouse capture ON: Application mouse selection and scrolling enabled",
            "    💡 Press 'm' to switch between terminal and application mouse modes",
            "",
            "  File Output:",
            "    💡 TIP: Use '--output-file file.log' with UI mode for best experience!",
            "       UI mode allows real-time viewing while simultaneously writing to file.",
            "       This gives you both interactive filtering AND permanent log storage.",
            "",
            "  Buffer Configuration:",
            "    • Default buffer: 10,000 lines (configurable with --buffer-size)",
            "    • In selection mode: buffer expands to 2x size to preserve history",
            "    • Higher buffer sizes (20k, 30k) allow longer selection history",
            "",
            "  Filter Examples:",
            "    Basic regex: 'ERROR|WARN'               - Show only error and warning logs",
            "    Text search: 'user.*login'              - Show logs matching user login pattern",
            "",
            "  Advanced Pattern Syntax:",
            "    Logical AND: '\"info\" && \"32\"'          - Logs containing both 'info' and '32'",
            "    Logical OR:  '\"debug\" || \"error\"'       - Logs containing either 'debug' or 'error'",
            "    Grouping:    '(info || debug) && \"32\"'  - Complex logic with parentheses",
            "    Negation:    '!\"debug\"'                 - Logs NOT containing 'debug'",
            "    Mixed:       'ERROR && !\"timeout\"'      - Error logs excluding timeouts",
            "",
            "  Pattern Examples:",
            "    '(info || debug) && \"32\"'              - Logs with 'info' or 'debug' AND '32'",
            "    '\"INFO\" || \"DEBUG\" && \"32\"'         - Exact text matches with AND logic",
            "",
            "  Selection Highlighting:",
            "    • Selected lines are highlighted with white background",
            "    • Current cursor position is shown with reversed colors",
            "    • Yellow border indicates selection mode is active",
            "",
            "  Press any key to close help...",
            "",
        ]
    }
}