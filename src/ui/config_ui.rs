use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    layout::{Layout, Direction, Constraint, Alignment},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell, Clear},
    style::{Color, Style, Modifier},
    text::{Span, Line},
    Frame,
};
use std::io;
use tracing::{info, debug, error};

use crate::config::Config;

pub struct ConfigUI {
    config: Config,
    selected_row: usize,
    editing: bool,
    edit_value: String,
    config_keys: Vec<String>,
    error_message: Option<String>,
    success_message: Option<String>,
    show_help: bool,
}

impl ConfigUI {
    pub fn new() -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let config_keys = config.get_all_keys();
        
        Ok(Self {
            config,
            selected_row: 0,
            editing: false,
            edit_value: String::new(),
            config_keys,
            error_message: None,
            success_message: None,
            show_help: false,
        })
    }

    pub fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> bool {
        // Clear messages on new input
        self.error_message = None;
        self.success_message = None;

        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('?') => {
                    self.show_help = false;
                }
                _ => {}
            }
            return false;
        }

        if self.editing {
            match key.code {
                KeyCode::Enter => {
                    self.save_current_value();
                    self.editing = false;
                    self.edit_value.clear();
                }
                KeyCode::Esc => {
                    self.editing = false;
                    self.edit_value.clear();
                }
                KeyCode::Char(c) => {
                    self.edit_value.push(c);
                }
                KeyCode::Backspace => {
                    self.edit_value.pop();
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    return true; // Signal to quit
                }
                KeyCode::Char('h') | KeyCode::Char('?') => {
                    self.show_help = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.selected_row > 0 {
                        self.selected_row -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected_row < self.config_keys.len().saturating_sub(1) {
                        self.selected_row += 1;
                    }
                }
                KeyCode::Enter | KeyCode::Char('e') => {
                    self.start_editing();
                }
                KeyCode::Char('r') => {
                    self.reset_to_default();
                }
                KeyCode::Char('s') => {
                    self.save_config();
                }
                _ => {}
            }
        }
        false
    }

    fn start_editing(&mut self) {
        if self.selected_row < self.config_keys.len() {
            let key = &self.config_keys[self.selected_row];
            if let Ok(current_value) = self.config.get_value(key) {
                self.edit_value = current_value;
                self.editing = true;
            }
        }
    }

    fn save_current_value(&mut self) {
        if self.selected_row < self.config_keys.len() {
            let key = &self.config_keys[self.selected_row];
            match self.config.set_value(key, &self.edit_value) {
                Ok(()) => {
                    self.success_message = Some(format!("✅ Updated {}: {}", key, self.edit_value));
                    debug!("Config updated: {} = {}", key, self.edit_value);
                }
                Err(e) => {
                    self.error_message = Some(format!("❌ Error: {e}"));
                    error!("Failed to update config {}: {}", key, e);
                }
            }
        }
    }

    fn reset_to_default(&mut self) {
        if self.selected_row < self.config_keys.len() {
            let key = &self.config_keys[self.selected_row];
            
            // Reset to default values based on key
            let default_value = match key.as_str() {
                "autosave.enabled" => "false",
                "autosave.path" => "",
                "ui.buffer_expansion" => "10.0",
                "ui.theme" => "auto",
                "ui.show_timestamps" => "false",
                "web.endpoint" => "http://localhost:5080",
                "web.batch_size" => "10",
                "web.timeout_seconds" => "30",
                "pod_selector" => ".*",
                "container" => ".*",
                "namespace" => "default",
                "tail" => "10",
                "follow" => "true",
                "output" => "text",
                "buffer_size" => "20000",
                _ => "",
            };

            match self.config.set_value(key, default_value) {
                Ok(()) => {
                    self.success_message = Some(format!("🔄 Reset {key} to default: {default_value}"));
                    debug!("Config reset to default: {} = {}", key, default_value);
                }
                Err(e) => {
                    self.error_message = Some(format!("❌ Reset failed: {e}"));
                    error!("Failed to reset config {}: {}", key, e);
                }
            }
        }
    }

    fn save_config(&mut self) {
        match self.config.save() {
            Ok(()) => {
                self.success_message = Some("💾 Configuration saved successfully!".to_string());
                info!("Configuration saved successfully");
            }
            Err(e) => {
                self.error_message = Some(format!("❌ Save failed: {e}"));
                error!("Failed to save configuration: {}", e);
            }
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if self.show_help {
            self.render_help(frame);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Table
                Constraint::Length(3),  // Status/Error messages
                Constraint::Length(4),  // Help text
            ])
            .split(frame.size());

        // Title
        let title = Paragraph::new("Wake Configuration Editor")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        // Configuration table
        let header_cells = ["Key", "Value"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = self.config_keys.iter().enumerate().map(|(i, key)| {
            let value = self.config.get_value(key).unwrap_or_else(|_| "<error>".to_string());
            let display_value = if value.len() > 50 {
                format!("{}...", &value[..47])
            } else {
                value
            };

            let style = if i == self.selected_row {
                if self.editing {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Black).bg(Color::White)
                }
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(key.as_str()),
                Cell::from(if self.editing && i == self.selected_row {
                    format!("{}_", self.edit_value) // Show cursor
                } else {
                    display_value
                })
            ]).style(style)
        }).collect();

        let table = Table::new(rows, [Constraint::Percentage(40), Constraint::Percentage(60)])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("Configuration Values"));

        frame.render_widget(table, chunks[1]);

        // Status messages
        let status_text = if let Some(ref error) = self.error_message {
            vec![Line::from(Span::styled(error, Style::default().fg(Color::Red)))]
        } else if let Some(ref success) = self.success_message {
            vec![Line::from(Span::styled(success, Style::default().fg(Color::Green)))]
        } else if self.editing {
            vec![Line::from(Span::styled("Editing mode - Press Enter to save, Esc to cancel", Style::default().fg(Color::Yellow)))]
        } else {
            vec![Line::from(Span::styled("Ready - Press Enter to edit selected value", Style::default().fg(Color::Blue)))]
        };

        let status = Paragraph::new(status_text)
            .block(Block::default().borders(Borders::ALL).title("Status"));
        frame.render_widget(status, chunks[2]);

        // Help text
        let help_lines = vec![
            Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Navigate | "),
                Span::styled("Enter/e", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Edit | "),
                Span::styled("r", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Reset | "),
                Span::styled("s", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Save"),
            ]),
            Line::from(vec![
                Span::styled("h/?", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Help | "),
                Span::styled("q/Esc", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" Quit"),
            ]),
        ];

        let help = Paragraph::new(help_lines)
            .block(Block::default().borders(Borders::ALL).title("Controls"));
        frame.render_widget(help, chunks[3]);
    }

    fn render_help(&mut self, frame: &mut Frame) {
        let area = frame.size();
        
        // Clear the background
        frame.render_widget(Clear, area);
        
        // Create popup area
        let popup_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(area)[1];
        
        let popup_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(popup_area)[1];

        let help_text = vec![
            Line::from(Span::styled("Wake Configuration Editor Help", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/↓, j/k    - Move up/down in the configuration list"),
            Line::from(""),
            Line::from("Editing:"),
            Line::from("  Enter, e    - Start editing the selected configuration value"),
            Line::from("  Enter       - Save changes (when editing)"),
            Line::from("  Esc         - Cancel editing (when editing)"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  r           - Reset selected value to default"),
            Line::from("  s           - Save all configuration changes to file"),
            Line::from(""),
            Line::from("Other:"),
            Line::from("  h, ?        - Show/hide this help"),
            Line::from("  q, Esc      - Quit configuration editor"),
            Line::from(""),
            Line::from("Configuration Keys:"),
            Line::from("  autosave.*      - Automatic log saving settings"),
            Line::from("  ui.*            - User interface preferences"),
            Line::from("  web.*           - Web output configuration"),
            Line::from("  pod_selector    - Default pod selection pattern"),
            Line::from("  namespace       - Default Kubernetes namespace"),
            Line::from("  buffer_size     - Log buffer size in memory"),
            Line::from(""),
            Line::from(Span::styled("Press any key to close help", Style::default().fg(Color::Green))),
        ];

        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .style(Style::default().bg(Color::Black)))
            .style(Style::default().fg(Color::White).bg(Color::Black));

        frame.render_widget(help_paragraph, popup_area);
    }
}

pub async fn run_config_ui() -> Result<()> {
    info!("=== STARTING CONFIG UI ===");
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, DisableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create config UI
    let mut config_ui = ConfigUI::new()?;
    
    info!("Config UI: Entering main loop");
    
    // Main loop
    loop {
        terminal.draw(|f| {
            config_ui.render(f);
        })?;

        if let Event::Key(key) = event::read()? {
            if config_ui.handle_key_event(key) {
                break; // Quit signal received
            }
        }
    }

    // Cleanup
    info!("Config UI: Cleaning up terminal");
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    info!("=== CONFIG UI COMPLETED ===");
    Ok(())
}