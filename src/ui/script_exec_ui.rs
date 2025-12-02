use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};
use anyhow::Result;
use crate::scripts::executor::PodScriptResult;

/// State for script execution UI
pub struct ScriptExecutionUIState {
    pub script_name: String,
    pub pod_results: Vec<PodScriptResult>,
    pub total_pods: usize,
    pub completed_pods: usize,
    pub selected_pod_index: usize,
}

impl ScriptExecutionUIState {
    pub fn new(script_name: String, total_pods: usize) -> Self {
        Self {
            script_name,
            pod_results: Vec::new(),
            total_pods,
            completed_pods: 0,
            selected_pod_index: 0,
        }
    }

    pub fn add_result(&mut self, result: PodScriptResult) {
        self.pod_results.push(result);
        self.completed_pods += 1;
    }

    pub fn select_next_pod(&mut self) {
        if self.selected_pod_index < self.pod_results.len().saturating_sub(1) {
            self.selected_pod_index += 1;
        }
    }

    pub fn select_prev_pod(&mut self) {
        if self.selected_pod_index > 0 {
            self.selected_pod_index -= 1;
        }
    }

    pub fn get_selected_result(&self) -> Option<&PodScriptResult> {
        self.pod_results.get(self.selected_pod_index)
    }

    pub fn progress_percentage(&self) -> u16 {
        if self.total_pods == 0 {
            0
        } else {
            ((self.completed_pods as f64 / self.total_pods as f64) * 100.0) as u16
        }
    }
}

pub fn draw_script_execution_ui(f: &mut Frame, state: &ScriptExecutionUIState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(f.size());

    // Title and progress
    let title = Paragraph::new(format!("Script: {}", state.script_name))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Progress bar
    let progress = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(state.progress_percentage() as f64 / 100.0)
        .label(format!("{}/{} pods", state.completed_pods, state.total_pods));
    f.render_widget(progress, chunks[1]);

    // Pod list
    let items: Vec<ListItem> = state
        .pod_results
        .iter()
        .enumerate()
        .map(|(idx, result)| {
            let is_selected = idx == state.selected_pod_index;
            let icon = if result.success { "✅" } else { "❌" };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!(
                "{} {}/{}",
                icon, result.pod_namespace, result.pod_name
            ))
            .style(style)
        })
        .collect();

    let pod_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Pods"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(pod_list, chunks[2]);

    // Selected pod output
    if let Some(selected) = state.get_selected_result() {
        let mut output_lines = vec![
            Line::from(vec![
                Span::styled("Pod: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}/{}", selected.pod_namespace, selected.pod_name)),
            ]),
            Line::from(vec![
                Span::styled("Exit Code: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(selected.exit_code.to_string()),
            ]),
            Line::from(""),
            Line::from(Span::styled("STDOUT:", Style::default().add_modifier(Modifier::BOLD))),
        ];

        for line in selected.stdout.lines().take(10) {
            output_lines.push(Line::from(line));
        }

        if !selected.stderr.is_empty() {
            output_lines.push(Line::from(""));
            output_lines.push(Line::from(Span::styled(
                "STDERR:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for line in selected.stderr.lines().take(10) {
                output_lines.push(Line::from(line));
            }
        }

        let output = Paragraph::new(output_lines)
            .block(Block::default().borders(Borders::ALL).title("Output"))
            .wrap(Wrap { trim: true });
        f.render_widget(output, chunks[3]);
    }
}
