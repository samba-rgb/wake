// Similar color scheme as template_ui for visual consistency
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
const HIGH_CONTRAST_WHITE: Color = Color::Rgb(255, 255, 255); // High contrast white for light terminals

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
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, ListState, Paragraph, 
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Row, Cell, Tabs
    },
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Resource usage data point
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub timestamp: Instant,
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

/// Container resource data
#[derive(Debug, Clone)]
pub struct ContainerResources {
    pub container_name: String,
    pub usage_history: VecDeque<ResourceUsage>,
    pub current_cpu: f64,
    pub current_memory: f64,
    pub cpu_limit: f64,
    pub memory_limit: f64,
}

impl ContainerResources {
    pub fn new(container_name: String) -> Self {
        Self {
            container_name,
            usage_history: VecDeque::with_capacity(60), // Store 60 data points (2 minutes at 2s intervals)
            current_cpu: 0.0,
            current_memory: 0.0,
            cpu_limit: 100.0, // Default to 100% for CPU
            memory_limit: 0.0, // Will be updated when data is available
        }
    }

    pub fn add_usage(&mut self, cpu: f64, memory: f64) {
        // Add new data point
        let usage = ResourceUsage {
            timestamp: Instant::now(),
            cpu_percent: cpu,
            memory_mb: memory,
        };

        self.current_cpu = cpu;
        self.current_memory = memory;

        // Maintain a fixed-size history
        if self.usage_history.len() >= 60 {
            self.usage_history.pop_front();
        }
        self.usage_history.push_back(usage);
    }
}

/// Monitor UI state
#[derive(Debug)]
pub struct MonitorUIState {
    pub pods: Vec<PodInfo>,
    pub selected_pod_index: usize,
    pub selected_container_index: usize,
    pub container_resources: HashMap<String, HashMap<String, ContainerResources>>, // pod_name -> (container_name -> resources)
    pub view_mode: ViewMode,
    pub update_interval: Duration,
    pub last_update: Instant,
    pub show_help: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ViewMode {
    Table,
    Chart,
}

impl MonitorUIState {
    pub fn new(pods: Vec<PodInfo>) -> Self {
        // Initialize container resources for each pod and container
        let mut container_resources = HashMap::new();
        
        for pod in &pods {
            let mut pod_resources = HashMap::new();
            for container in &pod.containers {
                pod_resources.insert(
                    container.clone(),
                    ContainerResources::new(container.clone()),
                );
            }
            container_resources.insert(pod.name.clone(), pod_resources);
        }

        Self {
            pods,
            selected_pod_index: 0,
            selected_container_index: 0,
            container_resources,
            view_mode: ViewMode::Table,
            update_interval: Duration::from_secs(2),
            last_update: Instant::now(),
            show_help: false,
        }
    }

    pub fn next_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod_index = (self.selected_pod_index + 1) % self.pods.len();
            self.selected_container_index = 0; // Reset container selection
        }
    }

    pub fn previous_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod_index = if self.selected_pod_index > 0 {
                self.selected_pod_index - 1
            } else {
                self.pods.len() - 1
            };
            self.selected_container_index = 0; // Reset container selection
        }
    }

    pub fn next_container(&mut self) {
        if let Some(pod) = self.selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = (self.selected_container_index + 1) % pod.containers.len();
            }
        }
    }

    pub fn previous_container(&mut self) {
        if let Some(pod) = self.selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = if self.selected_container_index > 0 {
                    self.selected_container_index - 1
                } else {
                    pod.containers.len() - 1
                };
            }
        }
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Table => ViewMode::Chart,
            ViewMode::Chart => ViewMode::Table,
        };
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn selected_pod(&self) -> Option<&PodInfo> {
        self.pods.get(self.selected_pod_index)
    }

    pub fn selected_container_name(&self) -> Option<String> {
        self.selected_pod()
            .and_then(|pod| pod.containers.get(self.selected_container_index).cloned())
    }

    pub fn selected_container_resources(&self) -> Option<&ContainerResources> {
        if let (Some(pod), Some(container_name)) = (self.selected_pod(), self.selected_container_name()) {
            self.container_resources
                .get(&pod.name)
                .and_then(|containers| containers.get(&container_name))
        } else {
            None
        }
    }
}

/// Run the monitor UI
pub async fn run_monitor_ui(
    ui_state: Arc<Mutex<MonitorUIState>>,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Force black background
    terminal.clear()?;

    // Run the application
    let app_result = run_monitor_app(&mut terminal, ui_state, shutdown_rx).await;

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

/// Main monitor UI application loop
async fn run_monitor_app<B: Backend>(
    terminal: &mut Terminal<B>,
    ui_state: Arc<Mutex<MonitorUIState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    // Poll for resource updates every second
    let mut update_interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        // Check if we need to shutdown
        let should_shutdown = tokio::select! {
            _ = update_interval.tick() => {
                // It's time to update metrics and redraw
                false
            },
            _ = shutdown_rx.recv() => {
                // Received shutdown signal
                true
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // No update needed, just check for input
                false
            }
        };

        if should_shutdown {
            break;
        }

        // Handle input events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut state = ui_state.lock().await;
                
                // Handle help screen separately
                if state.show_help {
                    match key.code {
                        KeyCode::Char('h') | KeyCode::F(1) | KeyCode::Esc => {
                            state.show_help = false;
                        }
                        KeyCode::Char('q') => {
                            break; // Exit application
                        }
                        _ => {}
                    }
                    continue;
                }
                
                // Handle main UI keys
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break; // Exit application
                    }
                    KeyCode::Char('h') | KeyCode::F(1) => {
                        state.toggle_help();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next_pod();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.previous_pod();
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        state.next_container();
                    }
                    KeyCode::Left => {
                        state.previous_container();
                    }
                    KeyCode::Tab => {
                        state.toggle_view_mode();
                    }
                    _ => {}
                }
            }
        }

        // Draw UI
        {
            let state = ui_state.lock().await;
            terminal.draw(|f| draw_monitor_ui(f, &state))?;
        }
    }

    Ok(())
}

/// Draw the monitor UI
fn draw_monitor_ui(f: &mut Frame, state: &MonitorUIState) {
    // Fill entire background with black
    let bg_block = Block::default().style(Style::default().bg(DARK_BG));
    f.render_widget(bg_block, f.size());

    if state.show_help {
        draw_help_popup(f, state);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.size());

    // Header
    draw_header(f, chunks[0], state);

    // Main content area
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Pod/Container selection
            Constraint::Percentage(70), // Resource display
        ])
        .split(chunks[1]);

    // Pod/Container selection
    draw_pod_container_list(f, main_chunks[0], state);

    // Resource display
    match state.view_mode {
        ViewMode::Table => draw_resource_table(f, main_chunks[1], state),
        ViewMode::Chart => draw_resource_chart(f, main_chunks[1], state),
    }

    // Footer
    draw_footer(f, chunks[2]);
}

/// Draw header section
fn draw_header(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    let pod_name = state.selected_pod().map_or("No pod selected".to_string(), |p| p.name.clone());
    let container_name = state.selected_container_name().unwrap_or_else(|| "No container selected".to_string());
    
    let title = format!("Wake Resource Monitor - Pod: {} / Container: {}", pod_name, container_name);

    let header = Paragraph::new(title)
        .style(Style::default().fg(NEON_CYAN).bg(DARK_BG).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BRIGHT_WHITE)));

    f.render_widget(header, area);
}

/// Draw pod and container selection list
fn draw_pod_container_list(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Pods
            Constraint::Percentage(50), // Containers
        ])
        .split(area);

    // Draw pod list
    let pod_items: Vec<ListItem> = state
        .pods
        .iter()
        .enumerate()
        .map(|(i, pod)| {
            let content = Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    &pod.name,
                    Style::default().fg(if i == state.selected_pod_index {
                        NEON_CYAN
                    } else {
                        SILVER
                    }),
                ),
            ]);

            ListItem::new(content).style(
                Style::default().bg(if i == state.selected_pod_index {
                    SELECTION_BG
                } else {
                    DARK_BG
                }),
            )
        })
        .collect();

    let mut pod_list_state = ListState::default();
    pod_list_state.select(Some(state.selected_pod_index));

    let pod_list = List::new(pod_items)
        .block(
            Block::default()
                .title("Pods")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">");

    f.render_stateful_widget(pod_list, chunks[0], &mut pod_list_state);

    // Draw container list
    let container_items: Vec<ListItem> = if let Some(pod) = state.selected_pod() {
        pod.containers
            .iter()
            .enumerate()
            .map(|(i, container)| {
                let content = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        container,
                        Style::default().fg(if i == state.selected_container_index {
                            NEON_CYAN
                        } else {
                            SILVER
                        }),
                    ),
                ]);

                ListItem::new(content).style(
                    Style::default().bg(if i == state.selected_container_index {
                        SELECTION_BG
                    } else {
                        DARK_BG
                    }),
                )
            })
            .collect()
    } else {
        vec![ListItem::new("No containers")]
    };

    let mut container_list_state = ListState::default();
    container_list_state.select(Some(state.selected_container_index));

    let container_list = List::new(container_items)
        .block(
            Block::default()
                .title("Containers")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">");

    f.render_stateful_widget(container_list, chunks[1], &mut container_list_state);
}

/// Draw resource table view
fn draw_resource_table(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    let resources = state.selected_container_resources();

    // Create header row
    let header_cells = vec![
        Cell::from(Span::styled("Metric", Style::default().fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Current", Style::default().fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Average", Style::default().fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Max", Style::default().fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD))),
    ];
    let header = Row::new(header_cells)
        .style(Style::default().fg(BRIGHT_WHITE))
        .height(1);

    // Create table rows
    let rows = if let Some(resources) = resources {
        let cpu_avg = if resources.usage_history.is_empty() {
            0.0
        } else {
            resources.usage_history.iter().map(|u| u.cpu_percent).sum::<f64>() / resources.usage_history.len() as f64
        };
        
        let mem_avg = if resources.usage_history.is_empty() {
            0.0
        } else {
            resources.usage_history.iter().map(|u| u.memory_mb).sum::<f64>() / resources.usage_history.len() as f64
        };

        let cpu_max = resources.usage_history.iter().map(|u| u.cpu_percent).fold(0.0, f64::max);
        let mem_max = resources.usage_history.iter().map(|u| u.memory_mb).fold(0.0, f64::max);

        vec![
            Row::new(vec![
                Cell::from("CPU Usage (%)"),
                Cell::from(format!("{:.2}%", resources.current_cpu)),
                Cell::from(format!("{:.2}%", cpu_avg)),
                Cell::from(format!("{:.2}%", cpu_max)),
            ]),
            Row::new(vec![
                Cell::from("Memory (MB)"),
                Cell::from(format!("{:.2} MB", resources.current_memory)),
                Cell::from(format!("{:.2} MB", mem_avg)),
                Cell::from(format!("{:.2} MB", mem_max)),
            ]),
        ]
    } else {
        vec![
            Row::new(vec![
                Cell::from("CPU Usage (%)"),
                Cell::from("N/A"),
                Cell::from("N/A"),
                Cell::from("N/A"),
            ]),
            Row::new(vec![
                Cell::from("Memory (MB)"),
                Cell::from("N/A"),
                Cell::from("N/A"),
                Cell::from("N/A"),
            ]),
        ]
    };

    let table = Table::new(rows, &[
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .header(header)
        .block(
            Block::default()
                .title("Resource Usage")
                .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BRIGHT_WHITE)),
        )
        .column_spacing(1);

    f.render_widget(table, area);
}

/// Draw resource usage charts
fn draw_resource_chart(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    // Split the area for CPU and memory charts
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // CPU chart
            Constraint::Percentage(50), // Memory chart
        ])
        .split(area);

    // Draw CPU chart
    draw_cpu_chart(f, chunks[0], state);

    // Draw memory chart
    draw_memory_chart(f, chunks[1], state);
}

/// Draw CPU usage chart
fn draw_cpu_chart(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    if let Some(resources) = state.selected_container_resources() {
        // Create data points for the chart
        let data_points: Vec<(f64, f64)> = resources
            .usage_history
            .iter()
            .enumerate()
            .map(|(i, usage)| (i as f64, usage.cpu_percent))
            .collect();

        // Create the dataset
        let datasets = vec![Dataset::default()
            .name("CPU %")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(VIBRANT_GREEN))
            .data(&data_points)];

        // Find min/max for y axis
        let max_y = resources.usage_history.iter()
            .map(|u| u.cpu_percent)
            .fold(0.0, f64::max)
            .max(1.0) // Ensure we have at least some range
            * 1.2; // Add some headroom

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("CPU Usage (%)")
                    .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BRIGHT_WHITE)),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(SILVER))
                    .bounds([0.0, if data_points.is_empty() { 60.0 } else { data_points.len() as f64 }]),
            )
            .y_axis(
                Axis::default()
                    .title("Percentage")
                    .style(Style::default().fg(SILVER))
                    .bounds([0.0, max_y]),
            );

        f.render_widget(chart, area);
    } else {
        // No data available
        let message = Paragraph::new("No CPU data available")
            .style(Style::default().fg(SILVER))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title("CPU Usage (%)")
                    .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BRIGHT_WHITE)),
            );
        
        f.render_widget(message, area);
    }
}

/// Draw memory usage chart
fn draw_memory_chart(f: &mut Frame, area: Rect, state: &MonitorUIState) {
    if let Some(resources) = state.selected_container_resources() {
        // Create data points for the chart
        let data_points: Vec<(f64, f64)> = resources
            .usage_history
            .iter()
            .enumerate()
            .map(|(i, usage)| (i as f64, usage.memory_mb))
            .collect();

        // Create the dataset
        let datasets = vec![Dataset::default()
            .name("Memory (MB)")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(ROYAL_BLUE))
            .data(&data_points)];

        // Find min/max for y axis
        let max_y = resources.usage_history.iter()
            .map(|u| u.memory_mb)
            .fold(0.0, f64::max)
            .max(1.0) // Ensure we have at least some range
            * 1.2; // Add some headroom

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title("Memory Usage (MB)")
                    .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BRIGHT_WHITE)),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(SILVER))
                    .bounds([0.0, if data_points.is_empty() { 60.0 } else { data_points.len() as f64 }]),
            )
            .y_axis(
                Axis::default()
                    .title("MB")
                    .style(Style::default().fg(SILVER))
                    .bounds([0.0, max_y]),
            );

        f.render_widget(chart, area);
    } else {
        // No data available
        let message = Paragraph::new("No memory data available")
            .style(Style::default().fg(SILVER))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title("Memory Usage (MB)")
                    .title_style(Style::default().fg(NEON_CYAN).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BRIGHT_WHITE)),
            );
        
        f.render_widget(message, area);
    }
}

/// Draw footer with help text
fn draw_footer(f: &mut Frame, area: Rect) {
    let help_text = "↑/↓/j/k: Select Pod | ←/→/h/l: Select Container | Tab: Toggle View | h: Help | q: Quit";

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(SILVER).bg(DARK_BG))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BRIGHT_WHITE)));

    f.render_widget(footer, area);
}

/// Draw help popup
fn draw_help_popup(f: &mut Frame, state: &MonitorUIState) {
    let popup_area = centered_rect(60, 60, f.size());
    
    f.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Wake Resource Monitor - Help",
            Style::default().add_modifier(Modifier::BOLD).fg(NEON_CYAN),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled("Keyboard Shortcuts:", Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE))]),
        Line::from(vec![
            Span::styled("  ↑/↓/j/k", Style::default().fg(GOLD)),
            Span::styled("     - Select previous/next pod", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  ←/→/h/l", Style::default().fg(GOLD)),
            Span::styled("     - Select previous/next container", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  Tab", Style::default().fg(GOLD)),
            Span::styled("         - Toggle between table and chart views", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  h / F1", Style::default().fg(GOLD)),
            Span::styled("      - Toggle this help", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc", Style::default().fg(GOLD)),
            Span::styled("     - Quit", Style::default().fg(BRIGHT_WHITE)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("About the Monitor:", Style::default().add_modifier(Modifier::BOLD).fg(BRIGHT_WHITE))]),
        Line::from(vec![
            Span::raw("The monitor displays real-time CPU and memory usage for selected"),
        ]),
        Line::from(vec![
            Span::raw("containers in Kubernetes pods. Data is collected using the"),
        ]),
        Line::from(vec![
            Span::raw("'kubectl top pod' command and refreshed every 2 seconds."),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Table View:", Style::default().fg(NEON_CYAN)),
            Span::raw(" Shows current, average, and maximum resource usage"),
        ]),
        Line::from(vec![
            Span::styled("Chart View:", Style::default().fg(NEON_CYAN)),
            Span::raw(" Displays resource usage over time as line charts"),
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
        .wrap(ratatui::widgets::Wrap { trim: true });

    f.render_widget(help_paragraph, popup_area);
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

/// Main Monitor UI controller
#[derive(Clone)]
pub struct MonitorUI {
    pub ui_state: Arc<Mutex<MonitorUIState>>,
    pub shutdown_tx: mpsc::Sender<()>,
}

impl MonitorUI {
    pub fn new(pods: Vec<PodInfo>) -> Self {
        let ui_state = Arc::new(Mutex::new(MonitorUIState::new(pods)));
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        let ui_state_clone = ui_state.clone();
        
        // Spawn background task to update resource usage
        tokio::spawn(async move {
            run_monitor_ui(ui_state_clone, shutdown_rx).await.unwrap_or_else(|e| {
                eprintln!("Monitor UI error: {}", e);
            });
        });
        
        Self {
            ui_state,
            shutdown_tx,
        }
    }
    
    pub async fn update_resources(&self, pod_name: &str, container_name: &str, cpu: f64, memory: f64) -> Result<()> {
        let mut state = self.ui_state.lock().await;
        
        // Update resource data for the specified pod and container
        if let Some(pod_resources) = state.container_resources.get_mut(pod_name) {
            if let Some(container_resources) = pod_resources.get_mut(container_name) {
                container_resources.add_usage(cpu, memory);
            }
        }
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.shutdown_tx.send(()).await;
        Ok(())
    }
}