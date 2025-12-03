//! Script Selector UI - Interactive TUI for selecting scripts with autocomplete
//!
//! Shows suggestions: New (first), ALL (second), then saved scripts filtered by input

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;

use super::manager::ScriptManager;

/// Black background style - forced dark mode
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

/// Dark mode block with highlighted border
fn dark_block_highlight<'a>(title: &'a str, color: Color) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(title, Style::default().fg(color).add_modifier(Modifier::BOLD)))
        .style(dark_bg())
}

/// Selection result from the selector UI
#[derive(Debug, Clone)]
pub enum ScriptSelection {
    /// User wants to create a new script
    New,
    /// User wants to list/execute all scripts
    All,
    /// User selected a specific script by name
    Script(String),
    /// User cancelled
    Cancelled,
}

/// Selector state
pub struct ScriptSelectorState {
    input: String,
    suggestions: Vec<SuggestionItem>,
    selected_index: usize,
    should_exit: bool,
    selection: Option<ScriptSelection>,
}

#[derive(Debug, Clone)]
struct SuggestionItem {
    name: String,
    description: String,
    item_type: SuggestionType,
}

#[derive(Debug, Clone, PartialEq)]
enum SuggestionType {
    New,
    All,
    Script,
}

impl ScriptSelectorState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            suggestions: Vec::new(),
            selected_index: 0,
            should_exit: false,
            selection: None,
        }
    }

    pub fn with_initial_input(input: &str) -> Self {
        let mut state = Self::new();
        state.input = input.to_string();
        state
    }

    fn update_suggestions(&mut self, manager: &ScriptManager) {
        let input_lower = self.input.to_lowercase();
        let mut suggestions = Vec::new();

        // Always show "New" first if it matches the input
        if input_lower.is_empty() || "new".starts_with(&input_lower) {
            suggestions.push(SuggestionItem {
                name: "New".to_string(),
                description: "Create a new script".to_string(),
                item_type: SuggestionType::New,
            });
        }

        // Show "ALL" second if it matches
        if input_lower.is_empty() || "all".starts_with(&input_lower) {
            suggestions.push(SuggestionItem {
                name: "ALL".to_string(),
                description: "List all saved scripts".to_string(),
                item_type: SuggestionType::All,
            });
        }

        // Add saved scripts that match the input
        if let Ok(scripts) = manager.list() {
            for script in scripts {
                if input_lower.is_empty() || script.name.to_lowercase().starts_with(&input_lower) {
                    let arg_count = script.arguments.len();
                    let description = if arg_count > 0 {
                        format!("{} argument(s)", arg_count)
                    } else {
                        "No arguments".to_string()
                    };
                    
                    suggestions.push(SuggestionItem {
                        name: script.name,
                        description,
                        item_type: SuggestionType::Script,
                    });
                }
            }
        }

        self.suggestions = suggestions;
        
        // Reset selection if out of bounds
        if self.selected_index >= self.suggestions.len() {
            self.selected_index = 0;
        }
    }

    fn select_current(&mut self) {
        if let Some(item) = self.suggestions.get(self.selected_index) {
            self.selection = Some(match item.item_type {
                SuggestionType::New => ScriptSelection::New,
                SuggestionType::All => ScriptSelection::All,
                SuggestionType::Script => ScriptSelection::Script(item.name.clone()),
            });
            self.should_exit = true;
        }
    }
}

/// Run the script selector TUI
/// Returns the user's selection
pub async fn run_script_selector(initial_input: Option<&str>) -> Result<ScriptSelection> {
    let manager = ScriptManager::new()?;
    
    let mut state = match initial_input {
        Some(input) if !input.is_empty() => ScriptSelectorState::with_initial_input(input),
        _ => ScriptSelectorState::new(),
    };

    // Update initial suggestions
    state.update_suggestions(&manager);

    // If there's an exact match for a script name, select it directly
    if let Some(input) = initial_input {
        if !input.is_empty() {
            // Check for exact matches
            if input.eq_ignore_ascii_case("new") {
                return Ok(ScriptSelection::New);
            }
            if input.eq_ignore_ascii_case("all") {
                return Ok(ScriptSelection::All);
            }
            // Check if it's an exact script name match
            if manager.exists(input) {
                return Ok(ScriptSelection::Script(input.to_string()));
            }
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_selector_loop(&mut terminal, &mut state, &manager).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

async fn run_selector_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut ScriptSelectorState,
    manager: &ScriptManager,
) -> Result<ScriptSelection> {
    loop {
        terminal.draw(|f| draw_selector(f, state))?;

        if state.should_exit {
            return Ok(state.selection.clone().unwrap_or(ScriptSelection::Cancelled));
        }

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Esc => {
                    return Ok(ScriptSelection::Cancelled);
                }
                KeyCode::Enter => {
                    state.select_current();
                }
                KeyCode::Up => {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if state.selected_index < state.suggestions.len().saturating_sub(1) {
                        state.selected_index += 1;
                    }
                }
                KeyCode::Tab => {
                    // Auto-complete with selected suggestion
                    if let Some(item) = state.suggestions.get(state.selected_index) {
                        state.input = item.name.clone();
                        state.update_suggestions(manager);
                    }
                }
                KeyCode::Char(c) => {
                    state.input.push(c);
                    state.update_suggestions(manager);
                }
                KeyCode::Backspace => {
                    state.input.pop();
                    state.update_suggestions(manager);
                }
                _ => {}
            }
        }
    }
}

fn draw_selector(f: &mut Frame, state: &ScriptSelectorState) {
    // Fill entire screen with black background
    let area = f.size();
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(dark_bg()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(3),  // Input
            Constraint::Min(10),    // Suggestions
            Constraint::Length(3),  // Help
        ])
        .split(area);

    // Title
    let title = Paragraph::new("📜 Wake Scripts")
        .style(Style::default().fg(Color::Cyan).bg(Color::Black).add_modifier(Modifier::BOLD))
        .block(dark_block(""));
    f.render_widget(title, chunks[0]);

    // Input field with cursor
    let input_display = format!("{}│", state.input);
    let input = Paragraph::new(input_display)
        .style(Style::default().fg(Color::Yellow).bg(Color::Black))
        .block(dark_block_highlight("Type to filter scripts", Color::Yellow));
    f.render_widget(input, chunks[1]);

    // Suggestions list
    let items: Vec<ListItem> = state.suggestions.iter().enumerate().map(|(i, item)| {
        let (icon, color) = match item.item_type {
            SuggestionType::New => ("✨", Color::Green),
            SuggestionType::All => ("📋", Color::Cyan),
            SuggestionType::Script => ("📄", Color::White),
        };

        let style = if i == state.selected_index {
            Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(Color::Black).fg(Color::White)
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color).bg(if i == state.selected_index { Color::DarkGray } else { Color::Black })),
            Span::styled(&item.name, style.fg(color)),
            Span::styled(format!("  {}", item.description), Style::default().fg(Color::Gray).bg(if i == state.selected_index { Color::DarkGray } else { Color::Black })),
        ]);

        ListItem::new(line).style(style)
    }).collect();

    // Bind the title string to a variable to extend its lifetime
    let suggestions_title = format!("Suggestions ({})", state.suggestions.len());
    let suggestions = List::new(items)
        .block(dark_block(&suggestions_title))
        .style(dark_bg());
    f.render_widget(suggestions, chunks[2]);

    // Help bar
    let help_spans = vec![
        Span::styled("↑/↓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Select  ", Style::default().fg(Color::Gray)),
        Span::styled("Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" Autocomplete  ", Style::default().fg(Color::Gray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Confirm  ", Style::default().fg(Color::Gray)),
        Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Cancel", Style::default().fg(Color::Gray)),
    ];
    let help = Paragraph::new(Line::from(help_spans))
        .block(dark_block(""))
        .style(dark_bg());
    f.render_widget(help, chunks[3]);
}
