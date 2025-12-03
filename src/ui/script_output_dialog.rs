use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use anyhow::Result;

/// Dark mode base style - black background
fn dark_bg() -> Style {
    Style::default().bg(Color::Black)
}

/// Dark mode block with black background
fn dark_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(Color::Cyan)))
        .style(dark_bg())
}

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
    let area = f.area();
    
    // Clear entire screen with black background
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(dark_bg()), area);
    
    // Create a centered popup
    let popup_width = 60;
    let popup_height = 18;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);

    // Draw popup background with dark style
    let bg = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(dark_bg());
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
        .style(Style::default().fg(Color::Yellow).bg(Color::Black).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Summary
    let summary_text = vec![
        Line::from(vec![
            Span::styled("Total Pods: ", Style::default().fg(Color::Gray)),
            Span::styled(state.total_pods.to_string(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("✅ Successful: ", Style::default().fg(Color::Green)),
            Span::styled(state.successful_pods.to_string(), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("❌ Failed: ", Style::default().fg(Color::Red)),
            Span::styled(state.failed_pods.to_string(), Style::default().fg(Color::Red)),
        ]),
    ];
    let summary = Paragraph::new(summary_text).style(dark_bg());
    f.render_widget(summary, chunks[1]);

    // Merge option
    let merge_style = if state.selected_choice == OutputChoice::Merge {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green).bg(Color::Black)
    };
    let merge_line = Line::from(vec![
        Span::styled("[ ", Style::default().fg(Color::Gray).bg(Color::Black)),
        Span::styled("Merge", merge_style),
        Span::styled(" ] - Combine all outputs into a single file", Style::default().fg(Color::Gray).bg(Color::Black)),
    ]);
    let merge_para = Paragraph::new(merge_line).style(dark_bg());
    f.render_widget(merge_para, chunks[2]);

    // Separate option
    let separate_style = if state.selected_choice == OutputChoice::Separate {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan).bg(Color::Black)
    };
    let separate_line = Line::from(vec![
        Span::styled("[ ", Style::default().fg(Color::Gray).bg(Color::Black)),
        Span::styled("Separate", separate_style),
        Span::styled(" ] - Save individual pod outputs", Style::default().fg(Color::Gray).bg(Color::Black)),
    ]);
    let separate_para = Paragraph::new(separate_line).style(dark_bg());
    f.render_widget(separate_para, chunks[3]);

    // Help
    let help_text = Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" Navigate  ", Style::default().fg(Color::Gray)),
        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Select", Style::default().fg(Color::Gray)),
    ]);
    let help = Paragraph::new(help_text).style(dark_bg()).alignment(Alignment::Center);
    f.render_widget(help, chunks[4]);
}
