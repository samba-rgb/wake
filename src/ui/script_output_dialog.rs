use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use anyhow::Result;

/// Dialog state for output handling (merge vs. separate)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputChoice {
    Merge,
    Separate,
}

pub struct ScriptOutputDialogState {
    pub selected_choice: OutputChoice,
    pub total_pods: usize,
    pub successful_pods: usize,
    pub failed_pods: usize,
}

impl ScriptOutputDialogState {
    pub fn new(total_pods: usize, successful_pods: usize) -> Self {
        Self {
            selected_choice: OutputChoice::Merge,
            total_pods,
            successful_pods,
            failed_pods: total_pods.saturating_sub(successful_pods),
        }
    }

    pub fn toggle_choice(&mut self) {
        self.selected_choice = match self.selected_choice {
            OutputChoice::Merge => OutputChoice::Separate,
            OutputChoice::Separate => OutputChoice::Merge,
        };
    }
}

pub fn draw_script_output_dialog(f: &mut Frame, state: &ScriptOutputDialogState) {
    let area = f.size();
    
    // Create a centered popup
    let popup_width = 60;
    let popup_height = 18;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    // Draw background
    let bg = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));
    f.render_widget(bg, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(4),
        ])
        .split(popup);

    // Title
    let title = Paragraph::new("Script Execution Complete")
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Summary
    let summary_text = vec![
        Line::from(format!("Total Pods: {}", state.total_pods)),
        Line::from(format!("✅ Successful: {}", state.successful_pods)),
        Line::from(format!("❌ Failed: {}", state.failed_pods)),
    ];
    let summary = Paragraph::new(summary_text);
    f.render_widget(summary, chunks[1]);

    // Merge option
    let merge_style = if state.selected_choice == OutputChoice::Merge {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let merge_line = Line::from(vec![
        Span::raw("[ "),
        Span::styled("Merge", merge_style),
        Span::raw(" ] - Combine all outputs into a single file"),
    ]);
    let merge_para = Paragraph::new(merge_line);
    f.render_widget(merge_para, chunks[2]);

    // Separate option
    let separate_style = if state.selected_choice == OutputChoice::Separate {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let separate_line = Line::from(vec![
        Span::raw("[ "),
        Span::styled("Separate", separate_style),
        Span::raw(" ] - Save individual pod outputs"),
    ]);
    let separate_para = Paragraph::new(separate_line);
    f.render_widget(separate_para, chunks[3]);

    // Help
    let help_text = Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
        Span::raw(" Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" Select"),
    ]);
    let help = Paragraph::new(help_text).alignment(Alignment::Center);
    f.render_widget(help, chunks[4]);
}
