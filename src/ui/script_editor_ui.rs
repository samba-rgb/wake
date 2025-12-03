use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::io;
use anyhow::Result;
use crate::scripts::manager::{SavedScript, ParameterDef, ParameterType};

/// State for the script editor
pub struct ScriptEditorState {
    pub script_name: String,
    pub script_content: String,
    pub parameters: Vec<ParameterDef>,
    pub description: Option<String>,
    
    // UI state
    pub focus: EditorFocus,
    pub script_cursor_x: usize,
    pub script_cursor_y: usize,
    pub name_cursor: usize,
    pub saved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorFocus {
    Name,
    Description,
    Script,
    Parameters,
}

impl ScriptEditorState {
    pub fn new() -> Self {
        Self {
            script_name: String::new(),
            script_content: String::new(),
            parameters: Vec::new(),
            description: None,
            focus: EditorFocus::Name,
            script_cursor_x: 0,
            script_cursor_y: 0,
            name_cursor: 0,
            saved: false,
        }
    }

    pub fn set_script_name(&mut self, name: String) {
        self.script_name = name;
        self.name_cursor = self.script_name.len();
    }

    pub fn insert_script_char(&mut self, ch: char) {
        let lines: Vec<&str> = self.script_content.lines().collect();
        if self.script_cursor_y < lines.len() {
            let line = lines[self.script_cursor_y].to_string();
            let mut new_line = line.clone();
            if self.script_cursor_x <= line.len() {
                new_line.insert(self.script_cursor_x, ch);
                self.script_cursor_x += 1;
            }
            
            // Rebuild script content
            let mut new_content = String::new();
            for (i, l) in lines.iter().enumerate() {
                if i == self.script_cursor_y {
                    new_content.push_str(&new_line);
                } else {
                    new_content.push_str(l);
                }
                if i < lines.len() - 1 {
                    new_content.push('\n');
                }
            }
            self.script_content = new_content;
        }
    }

    pub fn build_script(&self) -> Result<SavedScript> {
        let mut script = SavedScript::new(self.script_name.clone(), self.script_content.clone());
        if let Some(desc) = &self.description {
            script.description = Some(desc.clone());
        }
        script.parameters = self.parameters.clone();
        Ok(script)
    }
}

/// Run the script editor
pub async fn run_script_editor(initial_name: Option<String>) -> Result<Option<SavedScript>> {
    let mut state = ScriptEditorState::new();
    
    if let Some(name) = initial_name {
        state.set_script_name(name);
    }
    
    // In a real implementation, this would be a full TUI event loop
    // For now, we'll return a placeholder
    Ok(None)
}

pub fn draw_script_editor(f: &mut Frame, state: &ScriptEditorState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(70),
            Constraint::Percentage(30),
        ])
        .split(f.size());

    // Script name input
    let name_block = Block::default()
        .borders(Borders::ALL)
        .title("Script Name");
    let name_para = Paragraph::new(state.script_name.as_str())
        .block(name_block)
        .style(if state.focus == EditorFocus::Name {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        });
    f.render_widget(name_para, chunks[0]);

    // Description input
    let desc_block = Block::default()
        .borders(Borders::ALL)
        .title("Description (optional)");
    let desc_text = state.description.as_deref().unwrap_or("");
    let desc_para = Paragraph::new(desc_text)
        .block(desc_block)
        .style(if state.focus == EditorFocus::Description {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        });
    f.render_widget(desc_para, chunks[1]);

    // Script content editor
    let script_block = Block::default()
        .borders(Borders::ALL)
        .title("Script Content")
        .style(if state.focus == EditorFocus::Script {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let script_para = Paragraph::new(state.script_content.as_str())
        .block(script_block)
        .wrap(Wrap { trim: true });
    f.render_widget(script_para, chunks[2]);

    // Parameters and help
    let mut help_lines = vec![
        Line::from(vec![
            Span::styled("Ctrl+S", Style::default().fg(Color::Yellow)),
            Span::raw(" Save  "),
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(" Next Field  "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
        ]),
        Line::from(""),
        Line::from(format!("Parameters: {}", state.parameters.len())),
    ];

    for param in &state.parameters {
        help_lines.push(Line::from(format!(
            "  • {}: {} {}",
            param.name,
            param.param_type.as_str(),
            if param.required { "(required)" } else { "(optional)" }
        )));
    }

    let help_block = Block::default()
        .borders(Borders::ALL)
        .title("Help & Parameters");
    let help_para = Paragraph::new(help_lines)
        .block(help_block)
        .wrap(Wrap { trim: true });
    f.render_widget(help_para, chunks[3]);
}
