// Enhanced vibrant color scheme for better visual experience
const DARK_BG: Color = Color::Black;
const NEON_CYAN: Color = Color::Rgb(0, 255, 255);           // Electric cyan for headers
const VIBRANT_GREEN: Color = Color::Rgb(50, 255, 50);       // Bright green for success
const ELECTRIC_ORANGE: Color = Color::Rgb(255, 140, 0);     // Bright orange for warnings  
const ROYAL_BLUE: Color = Color::Rgb(65, 105, 225);         // Royal blue for info
const HOT_PINK: Color = Color::Rgb(255, 20, 147);           // Hot pink for special states
const BRIGHT_WHITE: Color = Color::White;                   // Pure white for text
const SILVER: Color = Color::Rgb(192, 192, 192);            // Silver for secondary text
const LIME_GREEN: Color = Color::Rgb(50, 205, 50);          // Lime green for borders
const GOLD: Color = Color::Rgb(255, 215, 0);                // Gold for highlights
const SELECTION_BG: Color = Color::Rgb(25, 25, 112);        // Midnight blue for selection
const CRIMSON: Color = Color::Rgb(220, 20, 60);             // Crimson for errors

use crate::templates::executor::{UIUpdate, PodStatus, CommandStatus, CommandLog, PodExecutionState, TemplateExecutor};
use crate::templates::*;
use crate::k8s::pod::PodInfo;
use anyhow::Result;
use chrono::Local;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols::DOT,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame, Terminal,
};
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use unicode_width::UnicodeWidthStr;

/// Template UI state
#[derive(Debug, Clone)]
pub struct TemplateUIState {
    pub execution: TemplateExecution,
    pub pods: Vec<PodExecutionState>,
    pub template: Template,
    pub selected_pod: usize,
    pub log_scroll: usize,
    pub show_help: bool,
    pub execution_complete: bool,
    pub show_completion_dialog: bool,
    pub completion_time: Option<chrono::DateTime<Local>>, // Store fixed completion time
    pub global_logs: VecDeque<String>,
    pub error_message: Option<String>,
}

impl TemplateUIState {
    pub fn new(execution: TemplateExecution, pods: Vec<PodInfo>, template: Template) -> Self {
        // Calculate total commands including cleanup commands
        let total_commands = template.commands.len();
        
        let pod_states = pods
            .into_iter()
            .map(|pod| PodExecutionState {
                pod_info: pod,
                status: PodStatus::Starting,
                current_command_index: 0,
                total_commands, // This now includes cleanup commands
                command_logs: Vec::new(),
                downloaded_files: Vec::new(),
                error_message: None,
            })
            .collect();

        Self {
            execution,
            pods: pod_states,
            template,
            selected_pod: 0,
            log_scroll: 0,
            show_help: false,
            execution_complete: false,
            show_completion_dialog: false,
            global_logs: VecDeque::new(),
            error_message: None,
            completion_time: None,
        }
    }

    pub fn update(&mut self, update: UIUpdate) {
        match update {
            UIUpdate::PodStatusChanged { pod_index, status } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.status = status;
                }
                // Check if all pods are completed
                self.check_all_pods_completed();
            }
            UIUpdate::CommandStarted {
                pod_index,
                command_index,
                description,
            } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.current_command_index = command_index;
                    pod.command_logs.push(CommandLog {
                        timestamp: Local::now(),
                        command_index,
                        description,
                        output: None,
                        status: CommandStatus::Running,
                    });
                }
            }
            UIUpdate::CommandOutput {
                pod_index,
                command_index,
                output,
            } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    if let Some(log) = pod
                        .command_logs
                        .iter_mut()
                        .find(|log| log.command_index == command_index)
                    {
                        log.output = Some(output);
                    }
                }
            }
            UIUpdate::CommandCompleted {
                pod_index,
                command_index,
                success,
            } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    if let Some(log) = pod
                        .command_logs
                        .iter_mut()
                        .find(|log| log.command_index == command_index)
                    {
                        log.status = if success {
                            CommandStatus::Completed
                        } else {
                            CommandStatus::Failed
                        };
                    }
                }
            }
            UIUpdate::FileDownloaded { pod_index, file } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.downloaded_files.push(file);
                }
            }
            UIUpdate::ExecutionCompleted => {
                self.execution_complete = true;
                self.show_completion_dialog = true;
                self.completion_time = Some(Local::now()); // Store fixed completion time
                self.global_logs
                    .push_back("🎉 Template execution completed!".to_string());
            }
        }
    }

    /// Check if all pods have completed and show completion dialog
    fn check_all_pods_completed(&mut self) {
        let all_completed = self.pods.iter().all(|pod| {
            matches!(pod.status, PodStatus::Completed | PodStatus::Failed { .. })
        });
        
        if all_completed && !self.pods.is_empty() && !self.show_completion_dialog {
            self.execution_complete = true;
            self.show_completion_dialog = true;
            self.completion_time = Some(Local::now()); // Store fixed completion time
        }
    }

    pub fn next_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod = (self.selected_pod + 1) % self.pods.len();
        }
    }

    pub fn previous_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod = if self.selected_pod == 0 {
                self.pods.len() - 1
            } else {
                self.selected_pod - 1
            };
        }
    }

    pub fn scroll_log_up(&mut self) {
        if self.log_scroll > 0 {
            self.log_scroll -= 1;
        }
    }

    pub fn scroll_log_down(&mut self) {
        self.log_scroll += 1;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

/// Run the template UI
pub async fn run_template_ui(
    ui_state: Arc<Mutex<TemplateUIState>>,
    mut ui_rx: mpsc::Receiver<UIUpdate>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Force black background
    terminal.clear()?;

    let app_result = run_template_app(&mut terminal, ui_state, &mut ui_rx).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    app_result
}

/// Main template UI application loop
async fn run_template_app<B: Backend>(
    terminal: &mut Terminal<B>,
    ui_state: Arc<Mutex<TemplateUIState>>,
    ui_rx: &mut mpsc::Receiver<UIUpdate>,
) -> Result<()> {
    loop {
        // Handle UI updates
        while let Ok(update) = ui_rx.try_recv() {
            let mut state = ui_state.lock().await;
            state.update(update);
        }

        // Draw UI
        {
            let state = ui_state.lock().await;
            terminal.draw(|f| draw_template_ui(f, &state))?;
        }

        // Handle input events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut state = ui_state.lock().await;
                
                // Handle help screen separately
                if state.show_help {
                    match key.code {
                        KeyCode::Char('h') | KeyCode::F(1) | KeyCode::Esc => {
                            state.show_help = false;
                        }
                        KeyCode::Char('q') => {
                            break; // Allow quitting from help screen
                        }
                        _ => {}
                    }
                    continue; // Skip other key processing when help is shown
                }

                // Handle completion dialog separately
                if state.show_completion_dialog {
                    match key.code {
                        KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => {
                            break; // Exit application
                        }
                        KeyCode::Char('v') => {
                            // Switch to detailed log view
                            state.show_completion_dialog = false;
                        }
                        KeyCode::Char('o') => {
                            // Open output directory (for now, just exit)
                            // TODO: Implement opening file manager
                            break;
                        }
                        KeyCode::Char('r') => {
                            // Run template again (for now, just exit)
                            // TODO: Implement template restart
                            break;
                        }
                        _ => {}
                    }
                    continue; // Skip other key processing when completion dialog is shown
                }
                
                // Handle main UI keys
                match key.code {
                    KeyCode::Char('q') => {
                        // Allow quitting at any time, not just when execution is complete
                        break;
                    }
                    KeyCode::Esc => {
                        // Esc can always quit or close help
                        if state.execution_complete {
                            break;
                        } else {
                            // If execution is running, show a confirmation or just quit
                            break;
                        }
                    }
                    KeyCode::Char('h') | KeyCode::F(1) => {
                        state.toggle_help();
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        state.next_pod();
                    }
                    KeyCode::Left | KeyCode::BackTab => {
                        state.previous_pod();
                    }
                    KeyCode::Up => {
                        state.scroll_log_up();
                    }
                    KeyCode::Down => {
                        state.scroll_log_down();
                    }
                    KeyCode::Enter => {
                        if state.execution_complete {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check if execution is complete
        {
            let state = ui_state.lock().await;
            if state.execution_complete {
                // Wait a bit more for user to review results
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Ok(())
}

/// Draw the template UI
fn draw_template_ui(f: &mut Frame, state: &TemplateUIState) {
    // Fill entire background with black
    let bg_block = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(bg_block, f.size());

    if state.show_help {
        draw_help_popup(f, state);
        return;
    }

    // Show completion dialog if all pods are done
    if state.show_completion_dialog {
        draw_completion_dialog(f, state);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),    // Main content
            Constraint::Length(3),  // Footer
        ])
        .split(f.size());

    // Header
    draw_header(f, chunks[0], state);

    // Main content area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // Pod list
    draw_pod_list(f, main_chunks[0], state);

    // Pod details
    draw_pod_details(f, main_chunks[1], state);

    // Footer
    draw_footer(f, chunks[2], state);
}

/// Draw header section
fn draw_header(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    let title = format!(
        "Wake Template Executor: {} (Execution ID: {})",
        state.template.name,
        &state.execution.execution_id[..8]
    );

    let header = Paragraph::new(title)
        .style(Style::default().fg(NEON_CYAN).bg(DARK_BG).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BRIGHT_WHITE)));

    f.render_widget(header, area);
}

/// Draw pod list
fn draw_pod_list(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    let items: Vec<ListItem> = state
        .pods
        .iter()
        .enumerate()
        .map(|(i, pod)| {
            let status_icon = get_status_icon(&pod.status);
            let status_color = get_status_color(&pod.status);
            let progress = format!(
                "{}/{}", 
                pod.current_command_index + 1, 
                pod.total_commands
            );

            // Highlight the selected pod name with bright colors
            let pod_name_style = if i == state.selected_pod {
                Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(SILVER)
            };

            let content = vec![Line::from(vec![
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(
                    &pod.pod_info.name,
                    pod_name_style,
                ),
                Span::raw(" ("),
                Span::styled(progress, Style::default().fg(SILVER)),
                Span::raw(")"),
            ])];

            ListItem::new(content).style(if i == state.selected_pod {
                Style::default().bg(SELECTION_BG)
            } else {
                Style::default().bg(DARK_BG)
            })
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_pod));

    let list = List::new(items)
        .style(Style::default().bg(DARK_BG))
        .block(
            Block::default()
                .title("Pods")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE))
                .style(Style::default().bg(DARK_BG)),
        )
        .highlight_style(Style::default().bg(SELECTION_BG));

    f.render_stateful_widget(list, area, &mut list_state);
}

/// Draw pod details
fn draw_pod_details(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    if let Some(pod) = state.pods.get(state.selected_pod) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Pod info
                Constraint::Min(5),    // Command logs
                Constraint::Length(3), // Progress
            ])
            .split(area);

        // Pod info
        draw_pod_info(f, chunks[0], pod);

        // Command logs
        draw_command_logs(f, chunks[1], pod, state.log_scroll);

        // Progress
        draw_progress(f, chunks[2], pod);
    }
}

/// Draw pod information
fn draw_pod_info(f: &mut Frame, area: Rect, pod: &PodExecutionState) {
    // Create a grid layout for better organization
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Row 1: Pod name and CPU
            Constraint::Length(1), // Row 2: Status and Memory
        ])
        .split(area.inner(&Margin { vertical: 1, horizontal: 1 }));

    // Format pod information
    let pod_name = format!("{}/{}", pod.pod_info.namespace, pod.pod_info.name);
    let status_text = get_status_text(&pod.status);
    let status_color = get_status_color(&pod.status);

    // Format CPU information
    let cpu_text = if let Some(cpu_percent) = pod.pod_info.cpu_usage_percent {
        format!("{:.1}%", cpu_percent)
    } else {
        "N/A".to_string()
    };
    let cpu_color = pod.pod_info.cpu_usage_percent.map(|cpu| {
        if cpu > 80.0 { CRIMSON }
        else if cpu > 60.0 { ELECTRIC_ORANGE }
        else { VIBRANT_GREEN }
    }).unwrap_or(SILVER);

    // Format memory information
    let (memory_text, memory_color) = if let Some(memory_percent) = pod.pod_info.memory_usage_percent {
        let memory_color = if memory_percent > 80.0 { CRIMSON }
        else if memory_percent > 60.0 { ELECTRIC_ORANGE }
        else { VIBRANT_GREEN };

        let memory_text = if let (Some(usage_bytes), Some(limit_bytes)) = 
            (pod.pod_info.memory_usage_bytes, pod.pod_info.memory_limit_bytes) {
            format!("{:.1}% ({}/{})", 
                memory_percent, 
                format_bytes(usage_bytes), 
                format_bytes(limit_bytes)
            )
        } else {
            format!("{:.1}%", memory_percent)
        };
        (memory_text, memory_color)
    } else if let Some(usage_bytes) = pod.pod_info.memory_usage_bytes {
        (format_bytes(usage_bytes), BRIGHT_WHITE)
    } else {
        ("N/A".to_string(), SILVER)
    };

    // Create horizontal layout for each row (left and right columns)
    let row1_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Left column: Pod info
            Constraint::Percentage(40), // Right column: CPU info
        ])
        .split(chunks[0]);

    let row2_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Left column: Status info
            Constraint::Percentage(40), // Right column: Memory info
        ])
        .split(chunks[1]);

    // Truncate text if too long for the available space
    let max_pod_width = (row1_chunks[0].width as usize).saturating_sub(6); // Account for "Pod: "
    let displayed_pod_name = if pod_name.len() > max_pod_width {
        format!("{}...", &pod_name[..max_pod_width.saturating_sub(3)])
    } else {
        pod_name
    };

    let max_status_width = (row2_chunks[0].width as usize).saturating_sub(9); // Account for "Status: "
    let displayed_status = if status_text.len() > max_status_width {
        format!("{}...", &status_text[..max_status_width.saturating_sub(3)])
    } else {
        status_text
    };

    // Row 1: Pod name and CPU
    let pod_line = Line::from(vec![
        Span::styled("Pod: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
        Span::styled(displayed_pod_name, Style::default().fg(SILVER)),
    ]);
    let pod_paragraph = Paragraph::new(pod_line).style(Style::default().bg(DARK_BG));
    f.render_widget(pod_paragraph, row1_chunks[0]);

    let cpu_line = Line::from(vec![
        Span::styled("CPU: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
        Span::styled(cpu_text, Style::default().fg(cpu_color)),
    ]);
    let cpu_paragraph = Paragraph::new(cpu_line).alignment(Alignment::Right).style(Style::default().bg(DARK_BG));
    f.render_widget(cpu_paragraph, row1_chunks[1]);

    // Row 2: Status and Memory
    let status_line = Line::from(vec![
        Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
        Span::styled(displayed_status, Style::default().fg(status_color)),
    ]);
    let status_paragraph = Paragraph::new(status_line).style(Style::default().bg(DARK_BG));
    f.render_widget(status_paragraph, row2_chunks[0]);

    let memory_line = Line::from(vec![
        Span::styled("Memory: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
        Span::styled(memory_text, Style::default().fg(memory_color)),
    ]);
    let memory_paragraph = Paragraph::new(memory_line).alignment(Alignment::Right).style(Style::default().bg(DARK_BG));
    f.render_widget(memory_paragraph, row2_chunks[1]);

    // Draw the border around the entire pod info area
    let border_block = Block::default()
        .title("Pod Information")
        .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BRIGHT_WHITE))
        .style(Style::default().bg(DARK_BG));
    f.render_widget(border_block, area);
}

/// Format bytes into human-readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Draw command logs
fn draw_command_logs(f: &mut Frame, area: Rect, pod: &PodExecutionState, scroll: usize) {
    let mut log_lines = Vec::new();

    for log in &pod.command_logs {
        let timestamp = log.timestamp.format("%H:%M:%S");
        let status_icon = match log.status {
            CommandStatus::Running => "⏳",
            CommandStatus::Completed => "✅",
            CommandStatus::Failed => "❌",
            CommandStatus::Waiting => "⏸️",
        };

        // Command description line with enhanced colors
        log_lines.push(Line::from(vec![
            Span::styled(format!("[{}] ", timestamp), Style::default().fg(SILVER)),
            Span::raw(status_icon),
            Span::raw(" "),
            Span::styled(&log.description, Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE)),
        ]));

        // Command output if available
        if let Some(ref output) = log.output {
            for line in output.lines() {
                log_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(line, Style::default().fg(BRIGHT_WHITE)),
                ]));
            }
        }

        log_lines.push(Line::from("")); // Empty line for spacing
    }

    // Handle scrolling
    let visible_lines = if scroll < log_lines.len() {
        &log_lines[scroll..]
    } else {
        &[]
    };

    let paragraph = Paragraph::new(visible_lines.to_vec())
        .style(Style::default().bg(DARK_BG))
        .block(
            Block::default()
                .title("Command Logs")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE))
                .style(Style::default().bg(DARK_BG)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);

    // Scrollbar
    if log_lines.len() > area.height as usize {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let mut scrollbar_state = ScrollbarState::new(log_lines.len().saturating_sub(area.height as usize))
            .position(scroll);
        f.render_stateful_widget(
            scrollbar,
            area.inner(&Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

/// Draw progress section
fn draw_progress(f: &mut Frame, area: Rect, pod: &PodExecutionState) {
    // Calculate progress based on pod status and completed commands
    let (completed_commands, progress_text) = match &pod.status {
        PodStatus::Completed => {
            // When completed, all commands are done
            (pod.total_commands, format!("{}/{} commands (Complete)", pod.total_commands, pod.total_commands))
        }
        PodStatus::Failed { .. } => {
            // When failed, show current progress
            let completed = pod.current_command_index + 1;
            (completed, format!("{}/{} commands (Failed)", completed, pod.total_commands))
        }
        PodStatus::Running { .. } => {
            // When running, current command is in progress
            let completed = pod.current_command_index;
            (completed, format!("{}/{} commands (Running)", completed + 1, pod.total_commands))
        }
        PodStatus::DownloadingFiles { .. } => {
            // When downloading, all commands are done
            (pod.total_commands, format!("{}/{} commands (Downloading)", pod.total_commands, pod.total_commands))
        }
        _ => {
            // Starting or waiting
            let completed = pod.current_command_index;
            (completed, format!("{}/{} commands", completed, pod.total_commands))
        }
    };

    let progress = if pod.total_commands > 0 {
        completed_commands as f64 / pod.total_commands as f64
    } else {
        0.0
    };

    let gauge = Gauge::default()
        .block(Block::default()
            .title("Progress")
            .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BRIGHT_WHITE))
            .style(Style::default().bg(DARK_BG)))
        .gauge_style(Style::default().fg(match &pod.status {
            PodStatus::Completed => VIBRANT_GREEN,
            PodStatus::Failed { .. } => CRIMSON,
            PodStatus::Running { .. } => ROYAL_BLUE,
            PodStatus::DownloadingFiles { .. } => HOT_PINK,
            _ => GOLD,
        }).bg(DARK_BG))
        .percent((progress * 100.0) as u16)
        .label(progress_text);

    f.render_widget(gauge, area);
}

/// Draw footer
fn draw_footer(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    let help_text = if state.execution_complete {
        "Press 'q' or Enter to exit"
    } else {
        "Tab/Shift+Tab: Switch pods | ↑/↓: Scroll logs | h: Help | q: Quit"
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(SILVER).bg(DARK_BG))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BRIGHT_WHITE)));

    f.render_widget(footer, area);
}

/// Draw help popup
fn draw_help_popup(f: &mut Frame, state: &TemplateUIState) {
    let popup_area = centered_rect(80, 70, f.size());
    
    f.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Wake Template Executor - Help",
            Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled("Keyboard Shortcuts:", Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE))]),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab", Style::default().fg(GOLD)),
            Span::styled("    - Switch between pods", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  ↑ / ↓", Style::default().fg(GOLD)),
            Span::styled("             - Scroll command logs", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  h / F1", Style::default().fg(GOLD)),
            Span::styled("            - Toggle this help", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc", Style::default().fg(GOLD)),
            Span::styled("           - Quit (when execution complete)", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(GOLD)),
            Span::styled("             - Exit (when execution complete)", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Template Information:", Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE))]),
        Line::from(vec![
            Span::styled("  Name: ", Style::default().fg(NEON_CYAN)),
            Span::styled(&state.template.name, Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  Description: ", Style::default().fg(NEON_CYAN)),
            Span::styled(&state.template.description, Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  Commands: ", Style::default().fg(NEON_CYAN)),
            Span::styled(state.template.commands.len().to_string(), Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  Output Files: ", Style::default().fg(NEON_CYAN)),
            Span::styled(state.template.output_files.len().to_string(), Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Status Icons:", Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE))]),
        Line::from(vec![
            Span::styled("  🟡 Starting", Style::default().fg(GOLD)),
            Span::styled("      🔵 Running", Style::default().fg(ROYAL_BLUE)),
        ]),
        Line::from(vec![
            Span::styled("  ⏳ Waiting", Style::default().fg(NEON_CYAN)),
            Span::styled("       📥 Downloading", Style::default().fg(HOT_PINK)),
        ]),
        Line::from(vec![
            Span::styled("  ✅ Completed", Style::default().fg(VIBRANT_GREEN)),
            Span::styled("     ❌ Failed", Style::default().fg(CRIMSON)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(SILVER)),
            Span::styled("'h'", Style::default().fg(GOLD)),
            Span::styled(" again to close this help", Style::default().fg(SILVER)),
        ]),
    ];

    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().bg(DARK_BG))
        .block(
            Block::default()
                .title("Help")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE))
                .style(Style::default().bg(DARK_BG)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(help_paragraph, popup_area);
}

/// Draw completion dialog when all pods are finished
fn draw_completion_dialog(f: &mut Frame, state: &TemplateUIState) {
    let popup_area = centered_rect(70, 60, f.size());
    
    // Force black background for the entire popup area
    let bg_clear = Block::default().style(Style::default().bg(DARK_BG));
    f.render_widget(bg_clear, popup_area);
    f.render_widget(Clear, popup_area);

    // Calculate execution summary
    let total_pods = state.pods.len();
    let successful_pods = state.pods.iter().filter(|pod| matches!(pod.status, PodStatus::Completed)).count();
    let failed_pods = state.pods.iter().filter(|pod| matches!(pod.status, PodStatus::Failed { .. })).count();
    
    let total_files = state.pods.iter().map(|pod| pod.downloaded_files.len()).sum::<usize>();
    
    // Use stored completion time instead of calculating live
    let execution_time = if let Some(completion_time) = state.completion_time {
        completion_time.signed_duration_since(state.execution.timestamp.with_timezone(&Local)).num_seconds()
    } else {
        Local::now().signed_duration_since(state.execution.timestamp.with_timezone(&Local)).num_seconds()
    };
    let minutes = execution_time / 60;
    let seconds = execution_time % 60;

    let dialog_text = vec![
        Line::from(vec![Span::styled(
            "🎉 Template Execution Complete!",
            Style::default().add_modifier(Modifier::BOLD).fg(VIBRANT_GREEN),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Template: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
            Span::styled(&state.template.name, Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Execution ID: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
            Span::styled(&state.execution.execution_id[..8], Style::default().fg(SILVER)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("Execution Summary:", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN))]),
        Line::from(vec![
            Span::raw("  📊 Total Pods: "),
            Span::styled(total_pods.to_string(), Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::raw("  ✅ Successful: "),
            Span::styled(successful_pods.to_string(), Style::default().fg(VIBRANT_GREEN)),
        ]),
        Line::from(vec![
            Span::raw("  ❌ Failed: "),
            Span::styled(failed_pods.to_string(), Style::default().fg(if failed_pods > 0 { CRIMSON } else { VIBRANT_GREEN })),
        ]),
        Line::from(vec![
            Span::raw("  📁 Files Downloaded: "),
            Span::styled(total_files.to_string(), Style::default().fg(NEON_CYAN)),
        ]),
        Line::from(vec![
            Span::raw("  ⏱️  Total Time: "),
            Span::styled(format!("{}m {}s", minutes, seconds), Style::default().fg(GOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Output Directory: ", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN)),
            Span::styled(state.execution.output_dir.display().to_string(), Style::default().fg(HOT_PINK)),
        ]),
        Line::from(""),
        if failed_pods > 0 {
            Line::from(vec![
                Span::styled("⚠️  Warning: ", Style::default().fg(ELECTRIC_ORANGE).add_modifier(Modifier::BOLD)),
                Span::styled("Some pods failed. Check the logs for details.", Style::default().fg(BRIGHT_WHITE)),
            ])
        } else {
            Line::from(vec![
                Span::styled("🎯 Success! ", Style::default().fg(VIBRANT_GREEN).add_modifier(Modifier::BOLD)),
                Span::styled("All pods completed successfully.", Style::default().fg(BRIGHT_WHITE)),
            ])
        },
        Line::from(""),
        Line::from(vec![Span::styled("Available Actions:", Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN))]),
        Line::from(vec![
            Span::styled("  [Enter] or [q]", Style::default().fg(GOLD)),
            Span::styled(" - Exit", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  [v]", Style::default().fg(GOLD)),
            Span::styled(" - View detailed logs", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  [o]", Style::default().fg(GOLD)),
            Span::styled(" - Open output directory", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  [r]", Style::default().fg(GOLD)),
            Span::styled(" - Run template again", Style::default().fg(BRIGHT_WHITE)),
        ]),
    ];

    let dialog = Paragraph::new(dialog_text)
        .style(Style::default().bg(DARK_BG))
        .block(
            Block::default()
                .title("Execution Complete")
                .title_style(Style::default().fg(VIBRANT_GREEN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE))
                .style(Style::default().bg(DARK_BG)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(dialog, popup_area);
}

/// Helper function to create a centered rectangle
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

/// Get status icon for display
fn get_status_icon(status: &PodStatus) -> &'static str {
    match status {
        PodStatus::Starting => "🟡",
        PodStatus::Running { command_index } => "🔵",
        PodStatus::WaitingLocal { .. } => "⏳",
        PodStatus::DownloadingFiles { .. } => "📥",
        PodStatus::Completed => "✅",
        PodStatus::Failed { .. } => "❌",
    }
}

/// Get status color for display
fn get_status_color(status: &PodStatus) -> Color {
    match status {
        PodStatus::Starting => GOLD,
        PodStatus::Running { .. } => ROYAL_BLUE,
        PodStatus::WaitingLocal { .. } => NEON_CYAN,
        PodStatus::DownloadingFiles { .. } => HOT_PINK,
        PodStatus::Completed => VIBRANT_GREEN,
        PodStatus::Failed { .. } => CRIMSON,
    }
}

/// Get status text for display
fn get_status_text(status: &PodStatus) -> String {
    match status {
        PodStatus::Starting => "Starting".to_string(),
        PodStatus::Running { command_index } => format!("Running command {}", command_index + 1),
        PodStatus::WaitingLocal { duration, progress } => {
            format!("Waiting {} ({:.1}%)", duration, progress * 100.0)
        }
        PodStatus::DownloadingFiles { current, total } => {
            format!("Downloading files ({}/{})", current, total)
        }
        PodStatus::Completed => "Completed".to_string(),
        PodStatus::Failed { error } => format!("Failed: {}", error),
    }
}

/// Main Template UI controller
pub struct TemplateUI {
    template_executor: TemplateExecutor,
}

impl TemplateUI {
    pub fn new(template_executor: TemplateExecutor) -> Self {
        Self {
            template_executor,
        }
    }
    
    pub async fn run_template(&mut self, template_name: &str, args: std::collections::HashMap<String, String>) -> Result<()> {
        // This is a placeholder implementation
        // In a real implementation, this would start template execution
        // and manage the UI state
        Ok(())
    }
}