use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::io;
use anyhow::Result;

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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(100),
            Constraint::Length(4),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("Select or Create a Script")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
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
                Style::default()
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
        .block(Block::default().borders(Borders::ALL).title("Scripts"));
    f.render_widget(list, chunks[1]);

    // Help text
    let help_text = vec![
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" Select  "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
        ]),
        Line::from(""),
        Line::from(Span::raw("• 'New' - Create a new script")),
        Line::from(Span::raw("• 'ALL' - Show all saved scripts")),
    ];

    let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[2]);
}

pub fn draw_script_query(f: &mut Frame, query: &str, filtered_scripts: &[String]) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(100),
        ])
        .split(f.size());

    // Query input
    let query_para = Paragraph::new(format!("Query: {}", query))
        .block(Block::default().borders(Borders::ALL).title("Filter Scripts"))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(query_para, chunks[0]);

    // Filtered results
    let items: Vec<ListItem> = filtered_scripts
        .iter()
        .map(|name| ListItem::new(format!("📜 {}", name)))
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Matching Scripts"));
    f.render_widget(list, chunks[1]);
}
