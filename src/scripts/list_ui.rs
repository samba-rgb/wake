//! Script List UI - TUI for viewing, selecting, and editing saved scripts

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

use super::manager::{Script, ScriptManager};

/// Black background style
fn black_bg() -> Style {
    Style::default().bg(Color::Black)
}

/// Action to take after list UI
#[derive(Debug, Clone)]
pub enum ListAction {
    /// Edit the selected script
    Edit(String),
    /// Execute the selected script
    Execute(String),
    /// Delete the selected script
    Delete(String),
    /// Create a new script
    CreateNew,
    /// User cancelled
    Cancelled,
}

/// List UI state
struct ListState {
    scripts: Vec<Script>,
    selected_index: usize,
    show_preview: bool,
    show_delete_confirm: bool,
    message: Option<(String, bool)>, // (message, is_error)
    action: Option<ListAction>,
    should_exit: bool,
}

impl ListState {
    fn new(scripts: Vec<Script>) -> Self {
        Self {
            scripts,
            selected_index: 0,
            show_preview: true,
            show_delete_confirm: false,
            message: None,
            action: None,
            should_exit: false,
        }
    }

    fn selected_script(&self) -> Option<&Script> {
        self.scripts.get(self.selected_index)
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected_index < self.scripts.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }
}

/// Run the script list UI
pub async fn run_script_list_ui() -> Result<ListAction> {
    let manager = ScriptManager::new()?;
    let scripts = manager.list()?;

    if scripts.is_empty() {
        return Ok(ListAction::CreateNew);
    }

    let mut state = ListState::new(scripts);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_list_loop(&mut terminal, &mut state, &manager).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

async fn run_list_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut ListState,
    manager: &ScriptManager,
) -> Result<ListAction> {
    loop {
        terminal.draw(|f| draw_list_ui(f, state))?;

        if state.should_exit {
            return Ok(state.action.clone().unwrap_or(ListAction::Cancelled));
        }

        if let Event::Key(key) = event::read()? {
            // Clear message on any key
            state.message = None;

            // Handle delete confirmation dialog
            if state.show_delete_confirm {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Some(script) = state.selected_script() {
                            let name = script.name.clone();
                            match manager.delete(&name) {
                                Ok(_) => {
                                    state.message = Some((format!("Script '{}' deleted", name), false));
                                    // Refresh the list
                                    state.scripts = manager.list()?;
                                    if state.selected_index >= state.scripts.len() && state.selected_index > 0 {
                                        state.selected_index -= 1;
                                    }
                                    // If no scripts left, exit to create new
                                    if state.scripts.is_empty() {
                                        state.action = Some(ListAction::CreateNew);
                                        state.should_exit = true;
                                    }
                                }
                                Err(e) => {
                                    state.message = Some((format!("Delete failed: {}", e), true));
                                }
                            }
                        }
                        state.show_delete_confirm = false;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        state.show_delete_confirm = false;
                    }
                    _ => {}
                }
                continue;
            }

            // Normal key handling
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    state.action = Some(ListAction::Cancelled);
                    state.should_exit = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.move_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.move_down();
                }
                KeyCode::Enter | KeyCode::Char('e') => {
                    // Edit selected script
                    if let Some(script) = state.selected_script() {
                        state.action = Some(ListAction::Edit(script.name.clone()));
                        state.should_exit = true;
                    }
                }
                KeyCode::Char('x') => {
                    // Execute selected script
                    if let Some(script) = state.selected_script() {
                        state.action = Some(ListAction::Execute(script.name.clone()));
                        state.should_exit = true;
                    }
                }
                KeyCode::Char('d') | KeyCode::Delete => {
                    // Show delete confirmation
                    if state.selected_script().is_some() {
                        state.show_delete_confirm = true;
                    }
                }
                KeyCode::Char('n') => {
                    // Create new script
                    state.action = Some(ListAction::CreateNew);
                    state.should_exit = true;
                }
                KeyCode::Char('p') => {
                    // Toggle preview
                    state.show_preview = !state.show_preview;
                }
                _ => {}
            }
        }
    }
}

fn draw_list_ui(f: &mut Frame, state: &ListState) {
    // Fill with black background
    let area = f.size();
    f.render_widget(Block::default().style(black_bg()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(10),    // Main content
            Constraint::Length(3),  // Help bar
            Constraint::Length(2),  // Message bar
        ])
        .split(area);

    // Title
    let title = Paragraph::new(format!("📜 Saved Scripts ({} total)", state.scripts.len()))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).style(black_bg()));
    f.render_widget(title, chunks[0]);

    // Main content - split into list and preview
    let content_chunks = if state.show_preview {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(60),
            ])
            .split(chunks[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(chunks[1])
    };

    // Script list
    let items: Vec<ListItem> = state.scripts.iter().enumerate().map(|(i, script)| {
        let style = if i == state.selected_index {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let arg_count = script.arguments.len();
        let arg_indicator = if arg_count > 0 {
            format!(" ({} args)", arg_count)
        } else {
            String::new()
        };

        let line = Line::from(vec![
            Span::styled("📄 ", Style::default().fg(Color::Yellow)),
            Span::styled(&script.name, style.fg(Color::White)),
            Span::styled(arg_indicator, Style::default().fg(Color::Gray)),
        ]);

        ListItem::new(line).style(style)
    }).collect();

    let list_block = Block::default()
        .borders(Borders::ALL)
        .title("Scripts")
        .border_style(Style::default().fg(Color::Yellow))
        .style(black_bg());
    
    let list = List::new(items).block(list_block);
    f.render_widget(list, content_chunks[0]);

    // Preview panel
    if state.show_preview && content_chunks.len() > 1 {
        let preview_content = if let Some(script) = state.selected_script() {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&script.name),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Arguments:", Style::default().add_modifier(Modifier::BOLD)),
                ]),
            ];

            if script.arguments.is_empty() {
                lines.push(Line::from(Span::styled("  (none)", Style::default().fg(Color::Gray))));
            } else {
                for arg in &script.arguments {
                    let req = if arg.required { "*" } else { "" };
                    let default = arg.default_value.as_ref()
                        .map(|v| format!(" = \"{}\"", v))
                        .unwrap_or_default();
                    lines.push(Line::from(format!("  • {}{}{}", arg.name, req, default)));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Script Content:", Style::default().add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from("─".repeat(40)));
            
            for line in script.content.lines().take(20) {
                lines.push(Line::from(Span::styled(line, Style::default().fg(Color::Green))));
            }
            
            if script.content.lines().count() > 20 {
                lines.push(Line::from(Span::styled("... (truncated)", Style::default().fg(Color::Gray))));
            }

            lines.push(Line::from(""));
            lines.push(Line::from("─".repeat(40)));
            lines.push(Line::from(vec![
                Span::styled("Created: ", Style::default().fg(Color::Gray)),
                Span::raw(script.created_at.format("%Y-%m-%d %H:%M").to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().fg(Color::Gray)),
                Span::raw(script.updated_at.format("%Y-%m-%d %H:%M").to_string()),
            ]));

            lines
        } else {
            vec![Line::from("No script selected")]
        };

        let preview = Paragraph::new(preview_content)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Preview")
                .border_style(Style::default().fg(Color::Cyan))
                .style(black_bg()))
            .wrap(Wrap { trim: false });
        f.render_widget(preview, content_chunks[1]);
    }

    // Help bar
    let help_spans = vec![
        Span::styled("↑/↓", Style::default().fg(Color::Green)),
        Span::raw(" Navigate  "),
        Span::styled("Enter/e", Style::default().fg(Color::Green)),
        Span::raw(" Edit  "),
        Span::styled("x", Style::default().fg(Color::Green)),
        Span::raw(" Execute  "),
        Span::styled("d", Style::default().fg(Color::Green)),
        Span::raw(" Delete  "),
        Span::styled("n", Style::default().fg(Color::Green)),
        Span::raw(" New  "),
        Span::styled("p", Style::default().fg(Color::Green)),
        Span::raw(" Preview  "),
        Span::styled("q", Style::default().fg(Color::Green)),
        Span::raw(" Quit"),
    ];
    let help = Paragraph::new(Line::from(help_spans))
        .block(Block::default().borders(Borders::ALL).style(black_bg()));
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

    // Delete confirmation dialog
    if state.show_delete_confirm {
        draw_delete_dialog(f, state);
    }
}

fn draw_delete_dialog(f: &mut Frame, state: &ListState) {
    let area = centered_rect(50, 25, f.size());
    
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("⚠️  Confirm Delete")
        .borders(Borders::ALL)
        .style(black_bg());
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    if let Some(script) = state.selected_script() {
        let question = Paragraph::new(format!("Delete script '{}'?", script.name))
            .style(Style::default().add_modifier(Modifier::BOLD));
        f.render_widget(question, inner[0]);
    }

    let options = Paragraph::new("Press Y to confirm, N to cancel")
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(options, inner[1]);
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
