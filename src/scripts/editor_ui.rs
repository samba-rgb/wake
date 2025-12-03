//! Script Editor UI - TUI for creating and editing scripts
//! Features:
//! - Black background forced
//! - Full cursor movement (up/down/left/right)
//! - Editor selection (Wake Editor / External Vim)
//! - TOML format for vim editing
//! - Validation after editing

use anyhow::{Result, Context};
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
use std::io::{self, Write};
use std::process::Command;
use std::path::PathBuf;
use chrono::{Local, Utc};

use super::manager::{Script, ScriptArg, ScriptManager};

/// Black background style - used consistently across all script screens
fn black_bg() -> Style {
    Style::default().bg(Color::Black)
}

/// Default script template for Wake editor
const DEFAULT_SCRIPT_TEMPLATE: &str = r#"#!/bin/sh
# ================================================
# Wake Script
# ================================================
# Description: Describe what this script does
# ================================================

# Use arguments with ${arg_name} syntax
# Example: echo "Value: ${my_arg}"

echo "Script started..."

# Add your commands here

echo "Script completed."
"#;

/// Generate TOML template for vim editing
fn generate_vim_template(name: &str) -> String {
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    format!(r#"# Wake Script Configuration
# Edit this file and save with :wq
# ============================================

name = "{name}"
description = "Describe what this script does"

# Script content (use triple quotes for multiline)
content = """
#!/bin/sh
# ================================================
# {name}
# ================================================

# Use arguments with ${{arg_name}} syntax
# Example: echo "Searching for: ${{pattern}}"

echo "Script started..."

# Add your commands here

echo "Script completed."
"""

# Arguments (copy this block to add more)
# [[arguments]]
# name = "arg_name"
# description = "What this argument does"
# default_value = "optional default"
# required = true

# Example argument:
[[arguments]]
name = "example_arg"
description = "An example argument (delete or modify this)"
default_value = ""
required = false

# Timestamps (auto-generated, don't modify)
created_at = "{now}"
updated_at = "{now}"
"#)
}

/// Parse TOML content from vim back to Script
fn parse_vim_toml(content: &str) -> Result<Script> {
    let mut script_name = String::new();
    let mut description = String::new();
    let mut script_content = String::new();
    let mut arguments: Vec<ScriptArg> = Vec::new();
    
    let mut in_content_block = false;
    let mut content_lines: Vec<&str> = Vec::new();
    let mut in_arguments_section = false;
    let mut current_arg: Option<ScriptArg> = None;
    
    for line in content.lines() {
        let trimmed = line.trim();
        
        // Skip comments (but not inside content block)
        if trimmed.starts_with('#') && !in_content_block {
            continue;
        }
        
        // Skip empty lines outside content block
        if trimmed.is_empty() && !in_content_block {
            continue;
        }
        
        // Handle multiline content block
        if in_content_block {
            if trimmed == r#"""""# {
                in_content_block = false;
                script_content = content_lines.join("\n");
                continue;
            } else if trimmed.ends_with(r#"""""#) && trimmed != r#"""""# {
                in_content_block = false;
                let line_content = &line[..line.rfind(r#"""""#).unwrap()];
                content_lines.push(line_content);
                script_content = content_lines.join("\n");
                continue;
            } else {
                content_lines.push(line);
                continue;
            }
        }
        
        // Start of content block
        if trimmed.starts_with("content") && trimmed.contains(r#"""""#) {
            in_content_block = true;
            content_lines.clear();
            continue;
        }
        
        // Detect [[arguments]] section
        if trimmed == "[[arguments]]" {
            // Save previous argument if exists
            if let Some(arg) = current_arg.take() {
                if !arg.name.is_empty() {
                    arguments.push(arg);
                }
            }
            in_arguments_section = true;
            current_arg = Some(ScriptArg {
                name: String::new(),
                description: None,
                default_value: None,
                required: false,
            });
            continue;
        }
        
        // Parse key = "value" pairs
        if let Some(eq_pos) = trimmed.find(" = ") {
            let key = trimmed[..eq_pos].trim();
            let value_part = trimmed[eq_pos + 3..].trim();
            let value = value_part.trim_matches('"');
            
            if in_arguments_section {
                // We're in an argument block
                if let Some(ref mut arg) = current_arg {
                    match key {
                        "name" => arg.name = value.to_string(),
                        "description" => arg.description = if value.is_empty() { None } else { Some(value.to_string()) },
                        "default_value" => arg.default_value = if value.is_empty() { None } else { Some(value.to_string()) },
                        "required" => arg.required = value == "true",
                        _ => {}
                    }
                }
            } else {
                // Top-level fields
                match key {
                    "name" => script_name = value.to_string(),
                    "description" => description = value.to_string(),
                    _ => {}
                }
            }
        }
        // Also handle key="value" without spaces
        else if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim();
            let value_part = trimmed[eq_pos + 1..].trim();
            let value = value_part.trim_matches('"');
            
            // Skip timestamps
            if key == "created_at" || key == "updated_at" {
                continue;
            }
            
            if in_arguments_section {
                if let Some(ref mut arg) = current_arg {
                    match key {
                        "name" => arg.name = value.to_string(),
                        "description" => arg.description = if value.is_empty() { None } else { Some(value.to_string()) },
                        "default_value" => arg.default_value = if value.is_empty() { None } else { Some(value.to_string()) },
                        "required" => arg.required = value == "true",
                        _ => {}
                    }
                }
            } else {
                match key {
                    "name" => script_name = value.to_string(),
                    "description" => description = value.to_string(),
                    _ => {}
                }
            }
        }
    }
    
    // Save last argument if exists
    if let Some(arg) = current_arg {
        if !arg.name.is_empty() {
            arguments.push(arg);
        }
    }
    
    // Validate
    if script_name.is_empty() {
        anyhow::bail!("Script name is required");
    }
    if script_content.trim().is_empty() {
        anyhow::bail!("Script content cannot be empty");
    }
    
    let mut script = Script::new(script_name, script_content);
    script.description = if description.is_empty() { None } else { Some(description) };
    script.arguments = arguments;
    
    Ok(script)
}

/// Editor choice
#[derive(Debug, Clone, Copy, PartialEq)]
enum EditorChoice {
    WakeEditor,
    ExternalVim,
}

/// Editor state
#[derive(Debug, Clone, Copy, PartialEq)]
enum EditorPhase {
    NameInput,
    EditorSelection,
    WakeEditing,
    ArgumentsPanel,
}

/// Cursor position in text
#[derive(Debug, Clone, Default)]
struct CursorPos {
    line: usize,
    col: usize,
}

/// Script Editor State
pub struct ScriptEditorState {
    // Script data
    script_name: String,
    script_content: String,
    arguments: Vec<ScriptArg>,
    
    // Cursor and navigation
    cursor: CursorPos,
    scroll_offset: usize,
    name_cursor: usize,
    
    // UI state
    phase: EditorPhase,
    editor_choice: EditorChoice,
    selected_arg_index: usize,
    
    // Dialogs
    arg_dialog: Option<ArgDialogState>,
    
    // Messages
    message: Option<(String, bool)>,
    
    // Flags
    should_save: bool,
    is_new_script: bool,
}

/// Argument dialog state
#[derive(Debug, Clone)]
struct ArgDialogState {
    name: String,
    description: String,
    default_value: String,
    required: bool,
    focus_field: usize,
    cursors: [usize; 3], // name, desc, default
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
            cursors: [0; 3],
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
            cursors: [
                arg.name.len(),
                arg.description.as_ref().map(|s| s.len()).unwrap_or(0),
                arg.default_value.as_ref().map(|s| s.len()).unwrap_or(0),
            ],
            editing_index: Some(index),
        }
    }
}

impl ScriptEditorState {
    pub fn new(name: String) -> Self {
        let is_new = name.is_empty() || name == "New";
        Self {
            script_name: if is_new { String::new() } else { name },
            script_content: DEFAULT_SCRIPT_TEMPLATE.to_string(),
            arguments: Vec::new(),
            cursor: CursorPos::default(),
            scroll_offset: 0,
            name_cursor: 0,
            phase: EditorPhase::NameInput,
            editor_choice: EditorChoice::WakeEditor,
            selected_arg_index: 0,
            arg_dialog: None,
            message: None,
            should_save: false,
            is_new_script: is_new,
        }
    }

    pub fn from_script(script: Script) -> Self {
        Self {
            script_name: script.name,
            script_content: script.content,
            arguments: script.arguments,
            cursor: CursorPos::default(),
            scroll_offset: 0,
            name_cursor: 0,
            phase: EditorPhase::WakeEditing,
            editor_choice: EditorChoice::WakeEditor,
            selected_arg_index: 0,
            arg_dialog: None,
            message: None,
            should_save: false,
            is_new_script: false,
        }
    }

    /// Get lines of content
    fn lines(&self) -> Vec<&str> {
        self.script_content.lines().collect()
    }

    /// Get current line
    fn current_line(&self) -> &str {
        self.lines().get(self.cursor.line).copied().unwrap_or("")
    }

    /// Convert cursor position to byte offset
    fn cursor_to_offset(&self) -> usize {
        let lines = self.lines();
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == self.cursor.line {
                return offset + self.cursor.col.min(line.len());
            }
            offset += line.len() + 1; // +1 for newline
        }
        self.script_content.len()
    }

    /// Move cursor up
    fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.current_line().len();
            self.cursor.col = self.cursor.col.min(line_len);
            
            // Scroll if needed
            if self.cursor.line < self.scroll_offset {
                self.scroll_offset = self.cursor.line;
            }
        }
    }

    /// Move cursor down
    fn move_down(&mut self) {
        let lines = self.lines();
        if self.cursor.line < lines.len().saturating_sub(1) {
            self.cursor.line += 1;
            let line_len = self.current_line().len();
            self.cursor.col = self.cursor.col.min(line_len);
        }
    }

    /// Move cursor left
    fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            // Go to end of previous line
            self.cursor.line -= 1;
            self.cursor.col = self.current_line().len();
        }
    }

    /// Move cursor right
    fn move_right(&mut self) {
        let line_len = self.current_line().len();
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line < self.lines().len().saturating_sub(1) {
            // Go to start of next line
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// Move to start of line
    fn move_home(&mut self) {
        self.cursor.col = 0;
    }

    /// Move to end of line
    fn move_end(&mut self) {
        self.cursor.col = self.current_line().len();
    }

    /// Insert character at cursor
    fn insert_char(&mut self, c: char) {
        let offset = self.cursor_to_offset();
        self.script_content.insert(offset, c);
        self.cursor.col += 1;
    }

    /// Insert newline at cursor
    fn insert_newline(&mut self) {
        let offset = self.cursor_to_offset();
        self.script_content.insert(offset, '\n');
        self.cursor.line += 1;
        self.cursor.col = 0;
    }

    /// Delete character before cursor
    fn delete_char(&mut self) {
        if self.cursor.col > 0 {
            let offset = self.cursor_to_offset();
            if offset > 0 {
                self.script_content.remove(offset - 1);
                self.cursor.col -= 1;
            }
        } else if self.cursor.line > 0 {
            // Join with previous line
            let offset = self.cursor_to_offset();
            if offset > 0 {
                let prev_line_len = self.lines().get(self.cursor.line - 1).map(|l| l.len()).unwrap_or(0);
                self.script_content.remove(offset - 1);
                self.cursor.line -= 1;
                self.cursor.col = prev_line_len;
            }
        }
    }

    /// Validate script
    fn validate(&self) -> Result<(), String> {
        if self.script_name.is_empty() {
            return Err("Script name is required".to_string());
        }
        if self.script_name.len() > 50 {
            return Err("Name too long (max 50 chars)".to_string());
        }
        if !self.script_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err("Name: only letters, numbers, _, - allowed".to_string());
        }
        if self.script_content.trim().is_empty() {
            return Err("Script content cannot be empty".to_string());
        }
        if self.script_content.contains("<DESCRIBE_YOUR_SCRIPT_HERE>") {
            return Err("Please replace placeholder text".to_string());
        }
        Ok(())
    }

    /// Build script from state
    fn build_script(&self) -> Script {
        let mut script = Script::new(self.script_name.clone(), self.script_content.clone());
        script.arguments = self.arguments.clone();
        script
    }
}

/// Run the script editor TUI
pub async fn run_script_editor(name: Option<String>) -> Result<Option<Script>> {
    let manager = ScriptManager::new()?;
    
    let mut state = if let Some(ref script_name) = name {
        if script_name != "New" && manager.exists(script_name) {
            let script = manager.load(script_name)?;
            ScriptEditorState::from_script(script)
        } else {
            ScriptEditorState::new(script_name.clone())
        }
    } else {
        ScriptEditorState::new(String::new())
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_editor_loop(&mut terminal, &mut state, &manager).await;

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
            state.message = None;

            // Handle argument dialog first
            if let Some(ref mut dialog) = state.arg_dialog {
                match handle_arg_dialog_input(key.code, dialog) {
                    ArgDialogResult::Continue => continue,
                    ArgDialogResult::Cancel => {
                        state.arg_dialog = None;
                        continue;
                    }
                    ArgDialogResult::Save => {
                        let arg = ScriptArg {
                            name: dialog.name.clone(),
                            description: if dialog.description.is_empty() { None } else { Some(dialog.description.clone()) },
                            default_value: if dialog.default_value.is_empty() { None } else { Some(dialog.default_value.clone()) },
                            required: dialog.required,
                        };
                        if let Some(idx) = dialog.editing_index {
                            state.arguments[idx] = arg;
                            state.message = Some(("✓ Argument updated".to_string(), false));
                        } else {
                            state.arguments.push(arg);
                            state.message = Some(("✓ Argument added".to_string(), false));
                        }
                        state.arg_dialog = None;
                        continue;
                    }
                    ArgDialogResult::Error(e) => {
                        state.message = Some((e, true));
                        continue;
                    }
                }
            }

            // Handle based on current phase
            match state.phase {
                EditorPhase::NameInput => {
                    match key.code {
                        KeyCode::Enter => {
                            if state.script_name.is_empty() {
                                state.message = Some(("Name is required".to_string(), true));
                            } else if !state.script_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                                state.message = Some(("Only letters, numbers, _, - allowed".to_string(), true));
                            } else {
                                state.phase = EditorPhase::EditorSelection;
                            }
                        }
                        KeyCode::Esc => return Ok(None),
                        KeyCode::Char(c) => {
                            state.script_name.insert(state.name_cursor, c);
                            state.name_cursor += 1;
                        }
                        KeyCode::Backspace => {
                            if state.name_cursor > 0 {
                                state.name_cursor -= 1;
                                state.script_name.remove(state.name_cursor);
                            }
                        }
                        KeyCode::Left => {
                            if state.name_cursor > 0 { state.name_cursor -= 1; }
                        }
                        KeyCode::Right => {
                            if state.name_cursor < state.script_name.len() { state.name_cursor += 1; }
                        }
                        _ => {}
                    }
                }

                EditorPhase::EditorSelection => {
                    match key.code {
                        KeyCode::Up | KeyCode::Down => {
                            state.editor_choice = match state.editor_choice {
                                EditorChoice::WakeEditor => EditorChoice::ExternalVim,
                                EditorChoice::ExternalVim => EditorChoice::WakeEditor,
                            };
                        }
                        KeyCode::Enter => {
                            if state.editor_choice == EditorChoice::ExternalVim {
                                // Launch vim
                                let script = launch_vim_editor(&state.script_name, terminal).await?;
                                if let Some(s) = script {
                                    state.script_content = s.content;
                                    state.arguments = s.arguments;
                                    state.message = Some(("✓ Script loaded from vim".to_string(), false));
                                }
                            }
                            state.phase = EditorPhase::WakeEditing;
                        }
                        KeyCode::Esc => {
                            state.phase = EditorPhase::NameInput;
                        }
                        _ => {}
                    }
                }

                EditorPhase::WakeEditing => {
                    match key.code {
                        KeyCode::F(5) => {
                            // Save
                            match state.validate() {
                                Ok(()) => {
                                    let script = state.build_script();
                                    match manager.save(&script) {
                                        Ok(_) => {
                                            state.message = Some(("✓ Script saved!".to_string(), false));
                                            state.should_save = true;
                                        }
                                        Err(e) => state.message = Some((format!("Save failed: {}", e), true)),
                                    }
                                }
                                Err(e) => state.message = Some((e, true)),
                            }
                        }
                        KeyCode::F(2) => {
                            state.phase = EditorPhase::NameInput;
                            state.name_cursor = state.script_name.len();
                        }
                        KeyCode::F(3) => {
                            state.arg_dialog = Some(ArgDialogState::default());
                        }
                        KeyCode::Tab => {
                            state.phase = EditorPhase::ArgumentsPanel;
                        }
                        KeyCode::Esc => {
                            if state.should_save {
                                return Ok(Some(state.build_script()));
                            }
                            return Ok(None);
                        }
                        KeyCode::Up => state.move_up(),
                        KeyCode::Down => state.move_down(),
                        KeyCode::Left => state.move_left(),
                        KeyCode::Right => state.move_right(),
                        KeyCode::Home => state.move_home(),
                        KeyCode::End => state.move_end(),
                        KeyCode::Enter => state.insert_newline(),
                        KeyCode::Backspace => state.delete_char(),
                        KeyCode::Char(c) => state.insert_char(c),
                        _ => {}
                    }
                }

                EditorPhase::ArgumentsPanel => {
                    match key.code {
                        KeyCode::Tab | KeyCode::Esc => {
                            state.phase = EditorPhase::WakeEditing;
                        }
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
                        KeyCode::Char('a') => {
                            state.arg_dialog = Some(ArgDialogState::default());
                        }
                        KeyCode::Char('e') | KeyCode::Enter => {
                            if !state.arguments.is_empty() {
                                let arg = &state.arguments[state.selected_arg_index];
                                state.arg_dialog = Some(ArgDialogState::from_arg(arg, state.selected_arg_index));
                            }
                        }
                        KeyCode::Char('d') | KeyCode::Delete => {
                            if !state.arguments.is_empty() {
                                state.arguments.remove(state.selected_arg_index);
                                if state.selected_arg_index >= state.arguments.len() && state.selected_arg_index > 0 {
                                    state.selected_arg_index -= 1;
                                }
                                state.message = Some(("✓ Argument deleted".to_string(), false));
                            }
                        }
                        KeyCode::F(5) => {
                            // Save from arguments panel too
                            match state.validate() {
                                Ok(()) => {
                                    let script = state.build_script();
                                    match manager.save(&script) {
                                        Ok(_) => {
                                            state.message = Some(("✓ Script saved!".to_string(), false));
                                            state.should_save = true;
                                        }
                                        Err(e) => state.message = Some((format!("Save failed: {}", e), true)),
                                    }
                                }
                                Err(e) => state.message = Some((e, true)),
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Argument dialog result
enum ArgDialogResult {
    Continue,
    Cancel,
    Save,
    Error(String),
}

fn handle_arg_dialog_input(key: KeyCode, dialog: &mut ArgDialogState) -> ArgDialogResult {
    match key {
        KeyCode::Esc => ArgDialogResult::Cancel,
        KeyCode::Tab => {
            dialog.focus_field = (dialog.focus_field + 1) % 4;
            ArgDialogResult::Continue
        }
        KeyCode::Enter => {
            if dialog.name.is_empty() {
                ArgDialogResult::Error("Argument name is required".to_string())
            } else {
                ArgDialogResult::Save
            }
        }
        KeyCode::Char(' ') if dialog.focus_field == 3 => {
            dialog.required = !dialog.required;
            ArgDialogResult::Continue
        }
        KeyCode::Char(c) if dialog.focus_field < 3 => {
            let (field, cursor) = match dialog.focus_field {
                0 => (&mut dialog.name, &mut dialog.cursors[0]),
                1 => (&mut dialog.description, &mut dialog.cursors[1]),
                2 => (&mut dialog.default_value, &mut dialog.cursors[2]),
                _ => return ArgDialogResult::Continue,
            };
            field.insert(*cursor, c);
            *cursor += 1;
            ArgDialogResult::Continue
        }
        KeyCode::Backspace if dialog.focus_field < 3 => {
            let (field, cursor) = match dialog.focus_field {
                0 => (&mut dialog.name, &mut dialog.cursors[0]),
                1 => (&mut dialog.description, &mut dialog.cursors[1]),
                2 => (&mut dialog.default_value, &mut dialog.cursors[2]),
                _ => return ArgDialogResult::Continue,
            };
            if *cursor > 0 {
                *cursor -= 1;
                field.remove(*cursor);
            }
            ArgDialogResult::Continue
        }
        KeyCode::Left if dialog.focus_field < 3 => {
            let cursor = &mut dialog.cursors[dialog.focus_field];
            if *cursor > 0 { *cursor -= 1; }
            ArgDialogResult::Continue
        }
        KeyCode::Right if dialog.focus_field < 3 => {
            let field_len = match dialog.focus_field {
                0 => dialog.name.len(),
                1 => dialog.description.len(),
                2 => dialog.default_value.len(),
                _ => 0,
            };
            let cursor = &mut dialog.cursors[dialog.focus_field];
            if *cursor < field_len { *cursor += 1; }
            ArgDialogResult::Continue
        }
        _ => ArgDialogResult::Continue,
    }
}

/// Launch vim editor and return parsed script
async fn launch_vim_editor(
    name: &str,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<Option<Script>> {
    // Create temp file with TOML template
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("wake")
        .join("scripts");
    std::fs::create_dir_all(&config_dir)?;
    
    let temp_path = config_dir.join(format!(".tmp_{}.toml", name));
    let template = generate_vim_template(name);
    std::fs::write(&temp_path, &template)?;

    // Suspend TUI
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    // Launch vim
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let status = Command::new(&editor)
        .arg(&temp_path)
        .status()
        .context(format!("Failed to launch {}", editor))?;

    // Resume TUI
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;

    if !status.success() {
        std::fs::remove_file(&temp_path).ok();
        return Ok(None);
    }

    // Read and parse the file
    let content = std::fs::read_to_string(&temp_path)?;
    std::fs::remove_file(&temp_path).ok();

    match parse_vim_toml(&content) {
        Ok(script) => Ok(Some(script)),
        Err(e) => {
            // Keep file for debugging
            Err(e)
        }
    }
}

fn draw_editor(f: &mut Frame, state: &ScriptEditorState) {
    // Fill entire screen with black background
    let area = f.size();
    f.render_widget(Block::default().style(black_bg()), area);

    match state.phase {
        EditorPhase::NameInput => draw_name_dialog(f, state),
        EditorPhase::EditorSelection => draw_editor_selection(f, state),
        EditorPhase::WakeEditing | EditorPhase::ArgumentsPanel => draw_main_editor(f, state),
    }

    // Draw argument dialog overlay if open
    if let Some(ref dialog) = state.arg_dialog {
        draw_arg_dialog(f, dialog);
    }
}

fn draw_name_dialog(f: &mut Frame, state: &ScriptEditorState) {
    let area = centered_rect(50, 35, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title(Span::styled(" 📝 New Script ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(black_bg());
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

    let label = Paragraph::new("Enter a name for your script:")
        .style(Style::default().fg(Color::White));
    f.render_widget(label, inner[0]);

    let name_with_cursor = {
        let (before, after) = state.script_name.split_at(state.name_cursor.min(state.script_name.len()));
        format!("{}▌{}", before, after)
    };
    let input = Paragraph::new(name_with_cursor)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(black_bg()));
    f.render_widget(input, inner[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Continue  ", Style::default().fg(Color::Gray)),
        Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Cancel", Style::default().fg(Color::Gray)),
    ]));
    f.render_widget(hint, inner[2]);

    // Show message if any
    if let Some((ref msg, is_error)) = state.message {
        let style = if is_error { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) };
        let message = Paragraph::new(msg.as_str()).style(style);
        f.render_widget(message, inner[3]);
    }
}

fn draw_editor_selection(f: &mut Frame, state: &ScriptEditorState) {
    let area = centered_rect(60, 55, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title(Span::styled(" ✏️  Select Editor ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(black_bg());
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Length(1),   // Spacer
            Constraint::Length(7),   // Wake Editor option
            Constraint::Length(1),   // Spacer
            Constraint::Length(7),   // Vim option
            Constraint::Length(2),   // Hint
            Constraint::Min(0),
        ])
        .split(area);

    // Header
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("📜 Script: ", Style::default().fg(Color::Gray)),
            Span::styled(&state.script_name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Choose how you want to edit your script:", Style::default().fg(Color::White))),
    ]);
    f.render_widget(header, inner[0]);

    // Wake Editor option
    let opt1_selected = state.editor_choice == EditorChoice::WakeEditor;
    let opt1_border = if opt1_selected { Color::Green } else { Color::DarkGray };
    let opt1_icon = if opt1_selected { "▶ ◉" } else { "  ○" };
    let opt1_title_style = if opt1_selected {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let opt1 = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!("{} ", opt1_icon), opt1_title_style),
            Span::styled("Wake Editor", opt1_title_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   📝 ", Style::default().fg(Color::Cyan)),
            Span::styled("Built-in TUI editor", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("   📋 ", Style::default().fg(Color::Cyan)),
            Span::styled("Integrated argument manager", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("   ⚡ ", Style::default().fg(Color::Cyan)),
            Span::styled("Quick and simple", Style::default().fg(Color::Gray)),
        ]),
    ]).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(opt1_border)).style(black_bg()));
    f.render_widget(opt1, inner[2]);

    // External Editor option
    let opt2_selected = state.editor_choice == EditorChoice::ExternalVim;
    let opt2_border = if opt2_selected { Color::Magenta } else { Color::DarkGray };
    let opt2_icon = if opt2_selected { "▶ ◉" } else { "  ○" };
    let opt2_title_style = if opt2_selected {
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let editor_name = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let opt2 = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!("{} ", opt2_icon), opt2_title_style),
            Span::styled("External Editor", opt2_title_style),
            Span::styled(format!(" ({})", editor_name), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   🔧 ", Style::default().fg(Color::Yellow)),
            Span::styled("Full vim/nano power", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("   📄 ", Style::default().fg(Color::Yellow)),
            Span::styled("Edit TOML configuration", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("   🎯 ", Style::default().fg(Color::Yellow)),
            Span::styled("For power users", Style::default().fg(Color::Gray)),
        ]),
    ]).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(opt2_border)).style(black_bg()));
    f.render_widget(opt2, inner[4]);

    // Hint bar with styled keys
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Select  ", Style::default().fg(Color::Gray)),
        Span::styled(" Enter ", Style::default().fg(Color::Black).bg(Color::Green)),
        Span::styled(" Confirm  ", Style::default().fg(Color::Gray)),
        Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red)),
        Span::styled(" Back ", Style::default().fg(Color::Gray)),
    ]));
    f.render_widget(hint, inner[5]);
}

fn draw_main_editor(f: &mut Frame, state: &ScriptEditorState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(10),    // Main content
            Constraint::Length(3),  // Help bar
            Constraint::Length(2),  // Message bar
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(format!("📝 Script: {}", state.script_name))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).style(black_bg()));
    f.render_widget(title, chunks[0]);

    // Main content - split into editor and arguments
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[1]);

    // Script editor panel
    let editor_focused = state.phase == EditorPhase::WakeEditing;
    let editor_border = if editor_focused { Color::Yellow } else { Color::DarkGray };
    
    // Build content with cursor
    let lines = state.lines();
    let visible_height = content_chunks[0].height.saturating_sub(2) as usize;
    
    // Adjust scroll to keep cursor visible
    let scroll = state.scroll_offset;
    
    let content_lines: Vec<Line> = lines.iter().enumerate().skip(scroll).take(visible_height).map(|(i, line)| {
        if i == state.cursor.line && editor_focused {
            // Line with cursor
            let col = state.cursor.col.min(line.len());
            let (before, after) = line.split_at(col);
            Line::from(vec![
                Span::styled(before, Style::default().fg(Color::White)),
                Span::styled("▌", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
                Span::styled(after, Style::default().fg(Color::White)),
            ])
        } else {
            Line::from(Span::styled(*line, Style::default().fg(Color::White)))
        }
    }).collect();

    let editor_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(editor_border))
        .title(Span::styled(" Script Content ", Style::default().fg(editor_border)))
        .style(black_bg());
    
    let editor = Paragraph::new(content_lines)
        .block(editor_block)
        .wrap(Wrap { trim: false });
    f.render_widget(editor, content_chunks[0]);

    // Arguments panel
    let args_focused = state.phase == EditorPhase::ArgumentsPanel;
    let args_border = if args_focused { Color::Yellow } else { Color::DarkGray };
    let args_title = if args_focused { " Arguments [a]dd [e]dit [d]el " } else { " Arguments " };

    let args_items: Vec<ListItem> = if state.arguments.is_empty() {
        vec![
            ListItem::new(Span::styled("  No arguments", Style::default().fg(Color::DarkGray))),
            ListItem::new(Span::styled("  Press 'a' to add", Style::default().fg(Color::DarkGray))),
        ]
    } else {
        state.arguments.iter().enumerate().map(|(i, arg)| {
            let style = if i == state.selected_arg_index && args_focused {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let req = if arg.required { "*" } else { "" };
            let def = arg.default_value.as_ref().map(|v| format!("={}", v)).unwrap_or_default();
            ListItem::new(format!("• {}{}{}", arg.name, req, def)).style(style.fg(Color::White))
        }).collect()
    };

    let args_list = List::new(args_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(args_border))
            .title(Span::styled(args_title, Style::default().fg(args_border)))
            .style(black_bg()));
    f.render_widget(args_list, content_chunks[1]);

    // Help bar
    let help_text = if args_focused {
        vec![
            Span::styled("a", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Add  ", Style::default().fg(Color::Gray)),
            Span::styled("e", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Edit  ", Style::default().fg(Color::Gray)),
            Span::styled("d", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Del  ", Style::default().fg(Color::Gray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Editor  ", Style::default().fg(Color::Gray)),
            Span::styled("F5", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Save", Style::default().fg(Color::Gray)),
        ]
    } else {
        vec![
            Span::styled("↑↓←→", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Move  ", Style::default().fg(Color::Gray)),
            Span::styled("F5", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Save  ", Style::default().fg(Color::Gray)),
            Span::styled("F2", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Rename  ", Style::default().fg(Color::Gray)),
            Span::styled("F3", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" +Arg  ", Style::default().fg(Color::Gray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Args  ", Style::default().fg(Color::Gray)),
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Exit", Style::default().fg(Color::Gray)),
        ]
    };
    let help = Paragraph::new(Line::from(help_text))
        .block(Block::default().borders(Borders::ALL).style(black_bg()));
    f.render_widget(help, chunks[2]);

    // Message bar
    let msg_content = if let Some((ref msg, is_error)) = state.message {
        let style = if is_error { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) };
        Paragraph::new(msg.as_str()).style(style)
    } else {
        Paragraph::new(format!("Line {} Col {}", state.cursor.line + 1, state.cursor.col + 1))
            .style(Style::default().fg(Color::DarkGray))
    };
    f.render_widget(msg_content, chunks[3]);
}

fn draw_arg_dialog(f: &mut Frame, dialog: &ArgDialogState) {
    let area = centered_rect(60, 55, f.size());
    f.render_widget(Clear, area);
    
    let title = if dialog.editing_index.is_some() { " ✏️ Edit Argument " } else { " ➕ Add Argument " };
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(black_bg());
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
        ])
        .split(area);

    let add_cursor = |text: &str, cursor: usize, focused: bool| -> String {
        if focused {
            let c = cursor.min(text.len());
            let (before, after) = text.split_at(c);
            format!("{}▌{}", before, after)
        } else {
            text.to_string()
        }
    };

    // Name
    let name_style = if dialog.focus_field == 0 { Color::Yellow } else { Color::DarkGray };
    let name_text = add_cursor(&dialog.name, dialog.cursors[0], dialog.focus_field == 0);
    let name_input = Paragraph::new(name_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" Name * ").border_style(Style::default().fg(name_style)).style(black_bg()));
    f.render_widget(name_input, inner[0]);

    // Description
    let desc_style = if dialog.focus_field == 1 { Color::Yellow } else { Color::DarkGray };
    let desc_text = add_cursor(&dialog.description, dialog.cursors[1], dialog.focus_field == 1);
    let desc_input = Paragraph::new(desc_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" Description ").border_style(Style::default().fg(desc_style)).style(black_bg()));
    f.render_widget(desc_input, inner[1]);

    // Default
    let def_style = if dialog.focus_field == 2 { Color::Yellow } else { Color::DarkGray };
    let def_text = add_cursor(&dialog.default_value, dialog.cursors[2], dialog.focus_field == 2);
    let def_input = Paragraph::new(def_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" Default Value ").border_style(Style::default().fg(def_style)).style(black_bg()));
    f.render_widget(def_input, inner[2]);

    // Required
    let req_style = if dialog.focus_field == 3 { Color::Yellow } else { Color::DarkGray };
    let req_text = if dialog.required { "[X] Required" } else { "[ ] Required (Space to toggle)" };
    let req_input = Paragraph::new(req_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(req_style)).style(black_bg()));
    f.render_widget(req_input, inner[3]);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Next  ", Style::default().fg(Color::Gray)),
        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Save  ", Style::default().fg(Color::Gray)),
        Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Cancel", Style::default().fg(Color::Gray)),
    ]));
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
