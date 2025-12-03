use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use std::io;
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

/// State for script suggestion/selection
pub struct ScriptSuggestionState {
    pub options: Vec<String>, // "New", "ALL", or script names
    pub selected_index: usize,
    pub query: String,
}

impl ScriptSuggestionState {
    pub fn new(options: Vec<String>) -> Self {
        Self {
            options,
            selected_index: 0,
            query: String::new(),
        }
    }

    pub fn select_next(&mut self) {
        if self.selected_index < self.options.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn get_selected(&self) -> Option<&str> {
        self.options.get(self.selected_index).map(|s| s.as_str())
    }

    pub fn update_options(&mut self, options: Vec<String>) {
        self.options = options;
        self.selected_index = 0;
    }

    pub fn filter_options(&mut self, query: &str) {
        self.query = query.to_string();
        // This will be called externally with filtered results
    }
}

pub fn draw_script_suggestions(f: &mut Frame, state: &ScriptSuggestionState) {
    // Clear with black background
    let area = f.area();
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(dark_bg()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(100),
            Constraint::Length(6),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("Select or Create a Script")
        .style(Style::default().fg(Color::Cyan).bg(Color::Black).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Script list
    let items: Vec<ListItem> = state
        .options
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let is_selected = idx == state.selected_index;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White).bg(Color::Black)
            };

            let icon = match name.as_str() {
                "New" => "➕",
                "ALL" => "📋",
                _ => "📜",
            };

            ListItem::new(format!("{} {}", icon, name)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(dark_block("Scripts"))
        .style(dark_bg());
    f.render_widget(list, chunks[1]);

    // Help text
    let help_text = vec![
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Navigate  ", Style::default().fg(Color::Gray)),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Select  ", Style::default().fg(Color::Gray)),
            Span::styled("Ctrl+C", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("• 'New' - Create a new script", Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled("• 'ALL' - Show all saved scripts", Style::default().fg(Color::DarkGray))),
    ];

    let help = Paragraph::new(help_text)
        .block(dark_block("Help"))
        .style(dark_bg());
    f.render_widget(help, chunks[2]);
}

pub fn draw_script_query(f: &mut Frame, query: &str, filtered_scripts: &[String]) {
    // Clear with black background
    let area = f.area();
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(dark_bg()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(100),
        ])
        .split(f.area());

    // Query input
    let query_para = Paragraph::new(format!("Query: {}", query))
        .block(dark_block("Filter Scripts").border_style(Style::default().fg(Color::Yellow)))
        .style(Style::default().fg(Color::White).bg(Color::Black));
    f.render_widget(query_para, chunks[0]);

    // Filtered results
    let items: Vec<ListItem> = filtered_scripts
        .iter()
        .map(|name| ListItem::new(format!("📜 {}", name)).style(Style::default().fg(Color::White).bg(Color::Black)))
        .collect();

    let list = List::new(items)
        .block(dark_block("Matching Scripts"))
        .style(dark_bg());
    f.render_widget(list, chunks[1]);
}
