use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;
use anyhow::Result;
use crate::scripts::manager::{ParameterDef, ParameterType};

/// State for script argument input
pub struct ScriptArgsInputState {
    pub parameters: Vec<ParameterDef>,
    pub arguments: HashMap<String, String>,
    pub current_param_index: usize,
    pub current_input: String,
    pub finished: bool,
}

impl ScriptArgsInputState {
    pub fn new(parameters: Vec<ParameterDef>) -> Self {
        Self {
            parameters,
            arguments: HashMap::new(),
            current_param_index: 0,
            current_input: String::new(),
            finished: false,
        }
    }

    pub fn get_current_param(&self) -> Option<&ParameterDef> {
        self.parameters.get(self.current_param_index)
    }

    pub fn submit_current(&mut self) -> Result<()> {
        if let Some(param) = self.get_current_param() {
            // Validate input based on parameter type
            match param.param_type {
                ParameterType::Integer => {
                    if !self.current_input.parse::<i64>().is_ok() {
                        return Err(anyhow::anyhow!("Invalid integer value"));
                    }
                }
                ParameterType::Duration => {
                    // Simple duration validation (e.g., "30s", "2m", "1h")
                    if !self.current_input.ends_with('s')
                        && !self.current_input.ends_with('m')
                        && !self.current_input.ends_with('h')
                    {
                        return Err(anyhow::anyhow!(
                            "Duration must end with 's', 'm', or 'h'"
                        ));
                    }
                }
                ParameterType::Boolean => {
                    let lower = self.current_input.to_lowercase();
                    if lower != "true" && lower != "false" && lower != "yes" && lower != "no" {
                        return Err(anyhow::anyhow!("Must be 'true', 'false', 'yes', or 'no'"));
                    }
                }
                _ => {}
            }

            self.arguments
                .insert(param.name.clone(), self.current_input.clone());
            self.current_input.clear();
            self.current_param_index += 1;

            // Check if all parameters are filled
            if self.current_param_index >= self.parameters.len() {
                self.finished = true;
            }
        }

        Ok(())
    }

    pub fn go_back(&mut self) {
        if self.current_param_index > 0 {
            let prev_param = &self.parameters[self.current_param_index - 1];
            self.current_input = self
                .arguments
                .remove(&prev_param.name)
                .unwrap_or_default();
            self.current_param_index -= 1;
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.current_input.push(ch);
    }

    pub fn backspace(&mut self) {
        self.current_input.pop();
    }
}

pub fn draw_script_args_input(f: &mut Frame, state: &ScriptArgsInputState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Percentage(100),
            Constraint::Length(4),
        ])
        .split(f.size());

    // Progress
    let progress = format!(
        "Parameter {}/{}",
        state.current_param_index + 1,
        state.parameters.len()
    );
    let progress_para = Paragraph::new(progress)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    f.render_widget(progress_para, chunks[0]);

    // Current parameter info
    if let Some(param) = state.get_current_param() {
        let mut info_lines = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&param.name),
            ]),
            Line::from(vec![
                Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(param.param_type.as_str()),
            ]),
        ];

        if let Some(desc) = &param.description {
            info_lines.push(Line::from(vec![
                Span::styled("Description: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(desc),
            ]));
        }

        if let Some(default) = &param.default_value {
            info_lines.push(Line::from(vec![
                Span::styled("Default: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(default),
            ]));
        }

        let info = Paragraph::new(info_lines)
            .block(Block::default().borders(Borders::ALL).title("Parameter Info"))
            .wrap(Wrap { trim: true });
        f.render_widget(info, chunks[1]);
    }

    // Input area
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Enter Value")
        .style(Style::default().fg(Color::Yellow));
    let input_para = Paragraph::new(state.current_input.as_str())
        .block(input_block)
        .wrap(Wrap { trim: true });
    f.render_widget(input_para, chunks[2]);

    // Help and controls
    let help_text = vec![
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" Next  "),
            Span::styled("Backspace", Style::default().fg(Color::Yellow)),
            Span::raw(" Back  "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
        ]),
        Line::from(""),
        Line::from("Type or paste the parameter value"),
    ];

    let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[3]);
}
