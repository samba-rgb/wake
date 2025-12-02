//! Script Editor UI - TUI for creating and editing scripts

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

use super::manager::{Script, ScriptArg, ScriptManager};

/// Default script template with placeholders
const DEFAULT_SCRIPT_TEMPLATE: &str = r#"#!/bin/sh
# ================================================
# Wake Script Template
# ================================================
# Description: <DESCRIBE_YOUR_SCRIPT_HERE>
# Author: <YOUR_NAME>
# Created: <DATE>
# ================================================

# Use arguments with ${arg_name} or $arg_name syntax
# Example: echo "Searching for: ${pattern}"

# --- Your script starts here ---

echo "Starting script execution..."

# Example: Search for a pattern in logs
# grep -r "${pattern}" /var/log/

# Example: Check disk usage
# df -h

# Example: List processes
# ps aux | head -20

# --- Add your commands below ---

echo "Script completed."
"#;

/// Validation errors for the editor
#[derive(Debug, Clone)]
struct ValidationErrors {
    name_error: Option<String>,
    content_error: Option<String>,
}

impl ValidationErrors {
    fn new() -> Self {
        Self {
            name_error: None,
            content_error: None,
        }
    }

    fn has_errors(&self) -> bool {
        self.name_error.is_some() || self.content_error.is_some()
    }

    fn validate_name(name: &str) -> Option<String> {
        if name.is_empty() {
            return Some("Script name is required".to_string());
        }
        if name.len() > 50 {
            return Some("Name too long (max 50 chars)".to_string());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Some("Only letters, numbers, _, - allowed".to_string());
        }
        if name.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
            return Some("Name cannot start with a number".to_string());
        }
        None
    }

    fn validate_content(content: &str) -> Option<String> {
        if content.trim().is_empty() {
            return Some("Script content cannot be empty".to_string());
        }
        if content.contains("<DESCRIBE_YOUR_SCRIPT_HERE>") {
            return Some("Please replace <DESCRIBE_YOUR_SCRIPT_HERE> placeholder".to_string());
        }
        if content.contains("<YOUR_NAME>") {
            return Some("Please replace <YOUR_NAME> placeholder".to_string());
        }
        None
    }
}

/// Editor focus state
#[derive(Debug, Clone, Copy, PartialEq)]
enum EditorFocus {
    NameInput,
    ScriptContent,
    Arguments,
    ArgumentDialog,
}

/// Argument dialog state
#[derive(Debug, Clone)]
struct ArgDialogState {
    name: String,
    description: String,
    default_value: String,
    required: bool,
    focus_field: usize, // 0=name, 1=desc, 2=default, 3=required
    // Cursor positions for each field
    name_cursor: usize,
    desc_cursor: usize,
    default_cursor: usize,
    // Edit mode - if Some, we're editing an existing argument at this index
    editing_index: Option<usize>,
}

impl Default for ArgDialogState {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            default_value: String::new(),
            required: true,
            focus_field: 0,
            name_cursor: 0,
            desc_cursor: 0,
            default_cursor: 0,
            editing_index: None,
        }
    }
}

impl ArgDialogState {
    fn from_arg(arg: &ScriptArg, index: usize) -> Self {
        Self {
            name: arg.name.clone(),
            description: arg.description.clone().unwrap_or_default(),
            default_value: arg.default_value.clone().unwrap_or_default(),
            required: arg.required,
            focus_field: 0,
            name_cursor: arg.name.len(),
            desc_cursor: arg.description.as_ref().map(|s| s.len()).unwrap_or(0),
            default_cursor: arg.default_value.as_ref().map(|s| s.len()).unwrap_or(0),
            editing_index: Some(index),
        }
    }

    fn insert_char(&mut self, c: char) {
        match self.focus_field {
            0 => {
                self.name.insert(self.name_cursor, c);
                self.name_cursor += 1;
            }
            1 => {
                self.description.insert(self.desc_cursor, c);
                self.desc_cursor += 1;
            }
            2 => {
                self.default_value.insert(self.default_cursor, c);
                self.default_cursor += 1;
            }
            _ => {}
        }
    }

    fn delete_char(&mut self) {
        match self.focus_field {
            0 => {
                if self.name_cursor > 0 {
                    self.name_cursor -= 1;
                    self.name.remove(self.name_cursor);
                }
            }
            1 => {
                if self.desc_cursor > 0 {
                    self.desc_cursor -= 1;
                    self.description.remove(self.desc_cursor);
                }
            }
            2 => {
                if self.default_cursor > 0 {
                    self.default_cursor -= 1;
                    self.default_value.remove(self.default_cursor);
                }
            }
            _ => {}
        }
    }

    fn move_cursor_left(&mut self) {
        match self.focus_field {
            0 => {
                if self.name_cursor > 0 {
                    self.name_cursor -= 1;
                }
            }
            1 => {
                if self.desc_cursor > 0 {
                    self.desc_cursor -= 1;
                }
            }
            2 => {
                if self.default_cursor > 0 {
                    self.default_cursor -= 1;
                }
            }
            _ => {}
        }
    }

    fn move_cursor_right(&mut self) {
        match self.focus_field {
            0 => {
                if self.name_cursor < self.name.len() {
                    self.name_cursor += 1;
                }
            }
            1 => {
                if self.desc_cursor < self.description.len() {
                    self.desc_cursor += 1;
                }
            }
            2 => {
                if self.default_cursor < self.default_value.len() {
                    self.default_cursor += 1;
                }
            }
            _ => {}
        }
    }
}

/// Script Editor State
pub struct ScriptEditorState {
    script_name: String,
    script_content: String,
    arguments: Vec<ScriptArg>,
    cursor_pos: usize,
    scroll_offset: usize,
    focus: EditorFocus,
    selected_arg_index: usize,
    arg_dialog: Option<ArgDialogState>,
    message: Option<(String, bool)>, // (message, is_error)
    validation: ValidationErrors,
    should_save: bool,
    should_exit: bool,
    is_new_script: bool,
    show_name_dialog: bool,
    name_input_cursor: usize,
}

impl ScriptEditorState {
    pub fn new(name: String) -> Self {
        let is_new = name.is_empty() || name == "New";
        Self {
            script_name: if is_new { String::new() } else { name },
            script_content: if is_new { DEFAULT_SCRIPT_TEMPLATE.to_string() } else { String::new() },
            arguments: Vec::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            focus: if is_new { EditorFocus::NameInput } else { EditorFocus::ScriptContent },
            selected_arg_index: 0,
            arg_dialog: None,
            message: None,
            validation: ValidationErrors::new(),
            should_save: false,
            should_exit: false,
            is_new_script: is_new,
            show_name_dialog: is_new,
            name_input_cursor: 0,
        }
    }

    pub fn from_script(script: Script) -> Self {
        Self {
            script_name: script.name,
            script_content: script.content,
            arguments: script.arguments,
            cursor_pos: 0,
            scroll_offset: 0,
            focus: EditorFocus::ScriptContent,
            selected_arg_index: 0,
            arg_dialog: None,
            message: None,
            validation: ValidationErrors::new(),
            should_save: false,
            should_exit: false,
            is_new_script: false,
            show_name_dialog: false,
            name_input_cursor: 0,
        }
    }

    fn insert_char(&mut self, c: char) {
        self.script_content.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            let prev_char_pos = self.script_content[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.script_content.remove(prev_char_pos);
            self.cursor_pos = prev_char_pos;
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.script_content[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.script_content.len() {
            self.cursor_pos = self.script_content[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.script_content.len());
        }
    }

    fn validate(&mut self) -> bool {
        self.validation.name_error = ValidationErrors::validate_name(&self.script_name);
        self.validation.content_error = ValidationErrors::validate_content(&self.script_content);
        !self.validation.has_errors()
    }

    fn build_script(&self) -> Script {
        let mut script = Script::new(self.script_name.clone(), self.script_content.clone());
        script.arguments = self.arguments.clone();
        script
    }
}

/// Run the script editor TUI
pub async fn run_script_editor(name: Option<String>) -> Result<Option<Script>> {
    // Initialize state
    let manager = ScriptManager::new()?;
    
    let mut state = if let Some(ref script_name) = name {
        if script_name != "New" && manager.exists(script_name) {
            let script = manager.load(script_name)?;
            ScriptEditorState::from_script(script)
        } else {
            // New script - load with template
            ScriptEditorState::new(script_name.clone())
        }
    } else {
        ScriptEditorState::new(String::new())
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_editor_loop(&mut terminal, &mut state, &manager).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

async fn run_editor_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut ScriptEditorState,
    manager: &ScriptManager,
) -> Result<Option<Script>> {
    loop {
        terminal.draw(|f| draw_editor(f, state))?;

        if let Event::Key(key) = event::read()? {
            // Clear message on any key press
            state.message = None;

            // Handle name dialog if shown
            if state.show_name_dialog {
                match key.code {
                    KeyCode::Enter => {
                        // Validate name and proceed
                        if let Some(err) = ValidationErrors::validate_name(&state.script_name) {
                            state.message = Some((err, true));
                        } else {
                            state.show_name_dialog = false;
                            state.focus = EditorFocus::ScriptContent;
                            state.message = Some(("Name set! Now edit your script.".to_string(), false));
                        }
                    }
                    KeyCode::Esc => {
                        // Exit the editor completely
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        state.script_name.insert(state.name_input_cursor, c);
                        state.name_input_cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if state.name_input_cursor > 0 {
                            state.name_input_cursor -= 1;
                            state.script_name.remove(state.name_input_cursor);
                        }
                    }
                    KeyCode::Left => {
                        if state.name_input_cursor > 0 {
                            state.name_input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if state.name_input_cursor < state.script_name.len() {
                            state.name_input_cursor += 1;
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // Handle argument dialog if open
            if let Some(ref mut dialog) = state.arg_dialog {
                match key.code {
                    KeyCode::Esc => {
                        state.arg_dialog = None;
                    }
                    KeyCode::Tab => {
                        dialog.focus_field = (dialog.focus_field + 1) % 4;
                    }
                    KeyCode::BackTab => {
                        dialog.focus_field = if dialog.focus_field == 0 { 3 } else { dialog.focus_field - 1 };
                    }
                    // Enter ALWAYS saves (regardless of which field is focused)
                    KeyCode::Enter => {
                        if !dialog.name.is_empty() {
                            // Save argument
                            let arg = ScriptArg {
                                name: dialog.name.clone(),
                                description: if dialog.description.is_empty() { None } else { Some(dialog.description.clone()) },
                                default_value: if dialog.default_value.is_empty() { None } else { Some(dialog.default_value.clone()) },
                                required: dialog.required,
                            };
                            
                            // Check if editing or adding
                            if let Some(edit_idx) = dialog.editing_index {
                                state.arguments[edit_idx] = arg;
                                state.message = Some(("✅ Argument updated".to_string(), false));
                            } else {
                                state.arguments.push(arg);
                                state.message = Some(("✅ Argument added".to_string(), false));
                            }
                            state.arg_dialog = None;
                        } else {
                            state.message = Some(("❌ Argument name is required".to_string(), true));
                        }
                    }
                    // Space toggles Required checkbox (only when on that field)
                    KeyCode::Char(' ') if dialog.focus_field == 3 => {
                        dialog.required = !dialog.required;
                    }
                    KeyCode::Char(c) => {
                        dialog.insert_char(c);
                    }
                    KeyCode::Backspace => {
                        dialog.delete_char();
                    }
                    KeyCode::Left => {
                        dialog.move_cursor_left();
                    }
                    KeyCode::Right => {
                        dialog.move_cursor_right();
                    }
                    _ => {}
                }
                continue;
            }

            // ============================================
            // SIMPLE KEYBOARD SHORTCUTS (no Ctrl needed!)
            // ============================================
            match key.code {
                // F2 = Rename script
                KeyCode::F(2) => {
                    state.show_name_dialog = true;
                    state.name_input_cursor = state.script_name.len();
                }
                // F5 = Save script
                KeyCode::F(5) => {
                    if state.validate() {
                        let script = state.build_script();
                        match manager.save(&script) {
                            Ok(_) => {
                                state.message = Some(("✅ Script saved successfully!".to_string(), false));
                                state.should_save = true;
                            }
                            Err(e) => {
                                state.message = Some((format!("❌ Failed to save: {}", e), true));
                            }
                        }
                    } else {
                        if let Some(ref err) = state.validation.name_error {
                            state.message = Some((format!("❌ Name: {}", err), true));
                        } else if let Some(ref err) = state.validation.content_error {
                            state.message = Some((format!("❌ Content: {}", err), true));
                        }
                    }
                }
                // F3 = Add argument (works from any panel)
                KeyCode::F(3) => {
                    state.arg_dialog = Some(ArgDialogState::default());
                }
                // Esc = Exit
                KeyCode::Esc => {
                    state.should_exit = true;
                }
                // Tab = Switch panels
                KeyCode::Tab => {
                    state.focus = match state.focus {
                        EditorFocus::NameInput => EditorFocus::ScriptContent,
                        EditorFocus::ScriptContent => EditorFocus::Arguments,
                        EditorFocus::Arguments => EditorFocus::ScriptContent,
                        EditorFocus::ArgumentDialog => EditorFocus::ScriptContent,
                    };
                }
                // Panel-specific keys
                _ => match state.focus {
                    EditorFocus::ScriptContent => {
                        match key.code {
                            KeyCode::Char(c) => state.insert_char(c),
                            KeyCode::Enter => state.insert_char('\n'),
                            KeyCode::Backspace => state.delete_char(),
                            KeyCode::Left => state.move_cursor_left(),
                            KeyCode::Right => state.move_cursor_right(),
                            _ => {}
                        }
                    }
                    EditorFocus::Arguments => {
                        match key.code {
                            KeyCode::Up => {
                                if state.selected_arg_index > 0 {
                                    state.selected_arg_index -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if state.selected_arg_index < state.arguments.len().saturating_sub(1) {
                                    state.selected_arg_index += 1;
                                }
                            }
                            // 'a' = Add new argument
                            KeyCode::Char('a') => {
                                state.arg_dialog = Some(ArgDialogState::default());
                            }
                            // 'e' or Enter = Edit selected argument
                            KeyCode::Char('e') | KeyCode::Enter => {
                                if !state.arguments.is_empty() {
                                    let arg = &state.arguments[state.selected_arg_index];
                                    state.arg_dialog = Some(ArgDialogState::from_arg(arg, state.selected_arg_index));
                                }
                            }
                            // 'd' or Delete = Delete argument
                            KeyCode::Char('d') | KeyCode::Delete => {
                                if !state.arguments.is_empty() {
                                    state.arguments.remove(state.selected_arg_index);
                                    if state.selected_arg_index >= state.arguments.len() && state.selected_arg_index > 0 {
                                        state.selected_arg_index -= 1;
                                    }
                                    state.message = Some(("Argument deleted".to_string(), false));
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            if state.should_exit {
                if state.should_save {
                    return Ok(Some(state.build_script()));
                }
                return Ok(None);
            }
        }
    }
}

fn draw_editor(f: &mut Frame, state: &ScriptEditorState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title/Name
            Constraint::Min(10),    // Main content (script + args)
            Constraint::Length(3),  // Help bar
            Constraint::Length(2),  // Message bar
        ])
        .split(f.size());

    // Title with script name and validation status
    let name_display = if state.script_name.is_empty() {
        "<unnamed>".to_string()
    } else {
        state.script_name.clone()
    };
    
    let validation_indicator = if state.validation.has_errors() {
        " ⚠️"
    } else if !state.script_name.is_empty() {
        " ✓"
    } else {
        ""
    };

    let title = Paragraph::new(format!("📝 Script: {}{}", name_display, validation_indicator))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Wake Script Editor (F2 to rename)"));
    f.render_widget(title, chunks[0]);

    // Main content area - split into script and arguments
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70),  // Script content
            Constraint::Percentage(30),  // Arguments
        ])
        .split(chunks[1]);

    // Script content editor
    let script_style = if state.focus == EditorFocus::ScriptContent {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    
    let content_with_cursor = if state.focus == EditorFocus::ScriptContent {
        let (before, after) = state.script_content.split_at(state.cursor_pos);
        format!("{}│{}", before, after)
    } else {
        state.script_content.clone()
    };

    let script_block = Block::default()
        .borders(Borders::ALL)
        .title("Script Content (edit placeholders)")
        .border_style(script_style);
    
    let script_paragraph = Paragraph::new(content_with_cursor)
        .block(script_block)
        .wrap(Wrap { trim: false });
    f.render_widget(script_paragraph, content_chunks[0]);

    // Arguments panel with better title showing available keys
    let args_style = if state.focus == EditorFocus::Arguments {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let args_title = if state.focus == EditorFocus::Arguments {
        "Arguments [a:Add e:Edit d:Del]"
    } else {
        "Arguments"
    };

    let args_items: Vec<ListItem> = if state.arguments.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No arguments defined",
            Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)
        )),
        ListItem::new(Span::styled(
            "  Press 'a' to add one",
            Style::default().fg(Color::DarkGray)
        ))]
    } else {
        state.arguments.iter().enumerate().map(|(i, arg)| {
            let style = if i == state.selected_arg_index && state.focus == EditorFocus::Arguments {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            let req_indicator = if arg.required { " *" } else { "" };
            let default_str = arg.default_value.as_ref()
                .map(|v| format!(" = \"{}\"", v))
                .unwrap_or_default();
            
            ListItem::new(format!("• {}{}{}", arg.name, req_indicator, default_str))
                .style(style)
        }).collect()
    };

    let args_block = Block::default()
        .borders(Borders::ALL)
        .title(args_title)
        .border_style(args_style);
    
    let args_list = List::new(args_items).block(args_block);
    f.render_widget(args_list, content_chunks[1]);

    // Help bar - context-sensitive
    let help_text = if state.focus == EditorFocus::Arguments {
        vec![
            Span::styled("a", Style::default().fg(Color::Green)),
            Span::raw(" Add  "),
            Span::styled("e/Enter", Style::default().fg(Color::Green)),
            Span::raw(" Edit  "),
            Span::styled("d", Style::default().fg(Color::Green)),
            Span::raw(" Delete  "),
            Span::styled("↑/↓", Style::default().fg(Color::Green)),
            Span::raw(" Navigate  "),
            Span::styled("Tab", Style::default().fg(Color::Green)),
            Span::raw(" Switch  "),
            Span::styled("F5", Style::default().fg(Color::Green)),
            Span::raw(" Save"),
        ]
    } else {
        vec![
            Span::styled("F5", Style::default().fg(Color::Green)),
            Span::raw(" Save  "),
            Span::styled("F3", Style::default().fg(Color::Green)),
            Span::raw(" Add Arg  "),
            Span::styled("F2", Style::default().fg(Color::Green)),
            Span::raw(" Rename  "),
            Span::styled("Tab", Style::default().fg(Color::Green)),
            Span::raw(" Switch  "),
            Span::styled("Esc", Style::default().fg(Color::Green)),
            Span::raw(" Exit"),
        ]
    };
    let help = Paragraph::new(Line::from(help_text))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);

    // Message bar
    if let Some((ref msg, is_error)) = state.message {
        let style = if is_error {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        let message = Paragraph::new(msg.as_str()).style(style);
        f.render_widget(message, chunks[3]);
    }

    // Name dialog overlay (for new scripts)
    if state.show_name_dialog {
        draw_name_dialog(f, state);
    }

    // Argument dialog overlay
    if let Some(ref dialog) = state.arg_dialog {
        draw_arg_dialog(f, dialog);
    }
}

fn draw_name_dialog(f: &mut Frame, state: &ScriptEditorState) {
    let area = centered_rect(50, 30, f.size());
    
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("📝 Enter Script Name")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    // Instructions
    let instructions = Paragraph::new("Enter a name for your script (letters, numbers, _, -)")
        .style(Style::default().fg(Color::White));
    f.render_widget(instructions, inner[0]);

    // Name input field with cursor
    let name_with_cursor = {
        let (before, after) = state.script_name.split_at(state.name_input_cursor);
        format!("{}│{}", before, after)
    };
    let name_input = Paragraph::new(name_with_cursor)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Script Name *")
            .border_style(Style::default().fg(Color::Yellow)));
    f.render_widget(name_input, inner[1]);

    // Hint
    let hint = Paragraph::new("Press Enter to confirm, Esc to cancel")
        .style(Style::default().fg(Color::Gray));
    f.render_widget(hint, inner[2]);
}

fn draw_arg_dialog(f: &mut Frame, dialog: &ArgDialogState) {
    let area = centered_rect(60, 55, f.size());
    
    f.render_widget(Clear, area);
    
    let title = if dialog.editing_index.is_some() {
        "✏️  Edit Argument"
    } else {
        "➕ Add Argument"
    };
    
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    // Helper to add cursor to text
    let add_cursor = |text: &str, cursor: usize, is_focused: bool| -> String {
        if is_focused {
            let (before, after) = text.split_at(cursor.min(text.len()));
            format!("{}│{}", before, after)
        } else {
            text.to_string()
        }
    };

    // Name field
    let name_style = if dialog.focus_field == 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let name_text = add_cursor(&dialog.name, dialog.name_cursor, dialog.focus_field == 0);
    let name_input = Paragraph::new(name_text)
        .block(Block::default().borders(Borders::ALL).title("Name *").border_style(name_style));
    f.render_widget(name_input, inner[0]);

    // Description field
    let desc_style = if dialog.focus_field == 1 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let desc_text = add_cursor(&dialog.description, dialog.desc_cursor, dialog.focus_field == 1);
    let desc_input = Paragraph::new(desc_text)
        .block(Block::default().borders(Borders::ALL).title("Description").border_style(desc_style));
    f.render_widget(desc_input, inner[1]);

    // Default value field
    let default_style = if dialog.focus_field == 2 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let default_text = add_cursor(&dialog.default_value, dialog.default_cursor, dialog.focus_field == 2);
    let default_input = Paragraph::new(default_text)
        .block(Block::default().borders(Borders::ALL).title("Default Value").border_style(default_style));
    f.render_widget(default_input, inner[2]);

    // Required checkbox
    let req_style = if dialog.focus_field == 3 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let req_text = if dialog.required { "[X] Required" } else { "[ ] Required (Space to toggle)" };
    let req_input = Paragraph::new(req_text)
        .block(Block::default().borders(Borders::ALL).border_style(req_style));
    f.render_widget(req_input, inner[3]);

    // Hint
    let hint = Paragraph::new("Tab: next field | Enter: save | Esc: cancel | ←→: move cursor")
        .style(Style::default().fg(Color::Gray));
    f.render_widget(hint, inner[4]);
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
