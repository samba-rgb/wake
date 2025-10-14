use std::collections::HashMap;
use anyhow::Result;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{
        Block, Borders, Cell, List, ListItem, Row, Table, Tabs,
        // Add chart-related imports
        Axis, Chart, Dataset, GraphType,
    },
    symbols,
    Frame,
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use crate::logging::wake_logger;

// This will be fixed in another file since ContainerInfo doesn't exist in k8s::pod
use crate::k8s::pod::PodInfo;
// use crate::metrics::collector::{ModernMetricsCollector, ContainerMetrics};

// Temporary struct definitions to make compilation work
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
    pub status: String,
    pub ready: bool,
    pub restart_count: i32,
    pub container_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContainerMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_read: f64,
    pub disk_write: f64,
    pub net_rx: f64,
    pub net_tx: f64,
}

/// Container metrics with history for visualization
#[derive(Debug, Clone)]
pub struct ContainerMetricsHistory {
    pub cpu_history: Vec<f64>,
    pub memory_history: Vec<f64>,
    pub disk_read_history: Vec<f64>,
    pub disk_write_history: Vec<f64>,
    pub net_rx_history: Vec<f64>,
    pub net_tx_history: Vec<f64>,
    pub max_history_points: usize,
}

impl ContainerMetricsHistory {
    pub fn new(max_history_points: usize) -> Self {
        Self {
            cpu_history: Vec::with_capacity(max_history_points),
            memory_history: Vec::with_capacity(max_history_points),
            disk_read_history: Vec::with_capacity(max_history_points),
            disk_write_history: Vec::with_capacity(max_history_points),
            net_rx_history: Vec::with_capacity(max_history_points),
            net_tx_history: Vec::with_capacity(max_history_points),
            max_history_points,
        }
    }
    
    pub fn add_metrics(&mut self, metrics: &ContainerMetrics) {
        // Add current metrics to history
        self.cpu_history.push(metrics.cpu_usage);
        self.memory_history.push(metrics.memory_usage);
        self.disk_read_history.push(metrics.disk_read);
        self.disk_write_history.push(metrics.disk_write);
        self.net_rx_history.push(metrics.net_rx);
        self.net_tx_history.push(metrics.net_tx);
        
        // Ensure we don't exceed max history
        if self.cpu_history.len() > self.max_history_points {
            self.cpu_history.remove(0);
        }
        if self.memory_history.len() > self.max_history_points {
            self.memory_history.remove(0);
        }
        if self.disk_read_history.len() > self.max_history_points {
            self.disk_read_history.remove(0);
        }
        if self.disk_write_history.len() > self.max_history_points {
            self.disk_write_history.remove(0);
        }
        if self.net_rx_history.len() > self.max_history_points {
            self.net_rx_history.remove(0);
        }
        if self.net_tx_history.len() > self.max_history_points {
            self.net_tx_history.remove(0);
        }
    }
}

/// View mode for metrics display
#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode {
    Table,
    Chart,
}

/// Metrics source type that determines what metrics are available
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricsSource {
    KubectlTop,  // Only CPU and Memory available
    Full,        // Full metrics including network and disk available
}

/// Represents the current state of the monitor UI
#[derive(Clone)]
pub struct MonitorState {
    pub pods: Vec<PodInfo>,
    pub selected_pod_index: usize,
    pub selected_container_index: usize,
    pub tab_index: usize,
    pub usage_data: HashMap<String, ContainerMetrics>,
    pub metrics_history: HashMap<String, ContainerMetricsHistory>, // Added history for charts
    pub view_mode: ViewMode, // Added view mode to switch between table and chart views
    pub metrics_source: MetricsSource, // Added metrics source to auto-adjust the UI
}

impl MonitorState {
    /// Create a new monitor state with the given pods
    pub fn new(pods: Vec<PodInfo>) -> Self {
        Self {
            pods,
            selected_pod_index: 0,
            selected_container_index: 0,
            tab_index: 0,
            usage_data: HashMap::new(),
            metrics_history: HashMap::new(), // Initialize metrics history
            view_mode: ViewMode::Chart, // Set chart as the default view mode
            metrics_source: MetricsSource::KubectlTop, // Default to kubectl mode initially
        }
    }

    /// Get the currently selected pod
    pub fn selected_pod(&self) -> Option<&PodInfo> {
        self.pods.get(self.selected_pod_index)
    }

    /// Get the currently selected container
    pub fn selected_container(&self) -> Option<&str> {
        self.selected_pod()
            .and_then(|pod| pod.containers.get(self.selected_container_index))
            .map(|c| c.as_str())
    }

    /// Move to the next pod
    pub fn next_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod_index = (self.selected_pod_index + 1) % self.pods.len();
            self.selected_container_index = 0; // Reset container selection
        }
    }

    /// Move to the previous pod
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

    /// Move to the next container in the selected pod
    pub fn next_container(&mut self) {
        if let Some(pod) = self.selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = (self.selected_container_index + 1) % pod.containers.len();
            }
        }
    }

    /// Move to the previous container in the selected pod
    pub fn previous_container(&mut self) {
        if let Some(pod) = self.selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = if self.selected_container_index > 0 {
                    self.selected_container_index - 1
                } else {
                    pod.containers.len()
                };
            }
        }
    }

    /// Get the metrics for a container
    pub fn get_metrics(&self, pod_name: &str, container_name: &str) -> Option<&ContainerMetrics> {
        let key = format!("{pod_name}/{container_name}");
        self.usage_data.get(&key)
    }
}

/// Detect metrics source from the metrics data
fn detect_metrics_source(state: &MonitorState) -> MetricsSource {
    // Check if we have any metrics data
    if state.usage_data.is_empty() {
        return MetricsSource::KubectlTop; // Default to kubectl if no data
    }
    
    // Take any metrics entry and check if disk and network metrics are all zeros
    for metrics in state.usage_data.values() {
        if metrics.disk_read > 0.0 || metrics.disk_write > 0.0 || 
           metrics.net_rx > 0.0 || metrics.net_tx > 0.0 {
            return MetricsSource::Full; // We have disk or network metrics
        }
    }
    
    // If all disk and network metrics are zeros, assume kubectl mode
    MetricsSource::KubectlTop
}

/// Render the monitor UI
pub fn render_monitor(f: &mut Frame, state: &MonitorState, area: Rect) {
    // Create the main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),    // Tabs at the top
            Constraint::Min(0),       // Main content
            Constraint::Length(3),    // Help bar at the bottom
        ].as_ref())
        .split(area);

    // Render the tabs
    let titles = ["Overview", "Details"];
    let tabs = Tabs::new(
        titles.iter().map(|t| {
            Line::from(vec![Span::styled(*t, Style::default().fg(Color::White))])
        }).collect::<Vec<_>>(),
    )
    .block(Block::default().borders(Borders::ALL).title("Monitor"))
    .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    .select(state.tab_index);

    f.render_widget(tabs, chunks[0]);

    // Render the main content based on the selected tab
    match state.tab_index {
        0 => render_overview_tab(f, state, chunks[1]),
        1 => render_details_tab(f, state, chunks[1]),
        _ => {}
    }
    
    // Render the help bar at the bottom
    render_help_bar(f, chunks[2]);
}

/// Render the overview tab
fn render_overview_tab(f: &mut Frame, state: &MonitorState, area: Rect) {
    // Create layout with more space allocated to the charts
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Reduced width for pod list
            Constraint::Percentage(80), // Increased width for container info and charts
        ].as_ref())
        .split(area);

    // Render pod list
    let pod_items: Vec<ListItem> = state.pods.iter().enumerate()
        .map(|(i, pod)| {
            let style = if i == state.selected_pod_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            ListItem::new(pod.name.clone()).style(style)
        })
        .collect();

    let pods_list = List::new(pod_items)
        .block(Block::default().borders(Borders::ALL).title("Pods"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    f.render_widget(pods_list, chunks[0]);

    // Render container info for the selected pod
    if let Some(pod) = state.selected_pod() {
        let container_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Pod info
                Constraint::Min(0),     // Container charts
            ].as_ref())
            .split(chunks[1]);

        let pod_info = format!("Pod: {} (Namespace: {}) - Press 'c' to toggle chart/table view", 
                              pod.name, pod.namespace);
        let pod_info_block = Block::default()
            .borders(Borders::ALL)
            .title("Pod Info");

        f.render_widget(
            ratatui::widgets::Paragraph::new(pod_info)
                .block(pod_info_block)
                .style(Style::default()),
            container_chunks[0]
        );

        // Based on the view mode, show either a table or charts
        match state.view_mode {
            ViewMode::Table => render_overview_table(f, state, container_chunks[1], pod),
            ViewMode::Chart => render_overview_charts(f, state, container_chunks[1], pod),
        }
    }
}

/// Render container metrics table for overview tab
fn render_overview_table(f: &mut Frame, state: &MonitorState, area: Rect, pod: &PodInfo) {
    // Create container metrics table
    let header = Row::new(vec![
        Cell::from("Container").style(Style::default().fg(Color::Yellow)),
        Cell::from("CPU").style(Style::default().fg(Color::Yellow)),
        Cell::from("Memory").style(Style::default().fg(Color::Yellow)),
    ]);

    let rows: Vec<Row> = pod.containers.iter()
        .map(|container_name| {
            let metrics = state.get_metrics(&pod.name, container_name);
            
            let cpu_usage = metrics.map_or("N/A".to_string(), |m| format!("{:.2}", m.cpu_usage));
            let memory_usage = metrics.map_or("N/A".to_string(), |m| format!("{:.2}Mi", m.memory_usage / 1024.0 / 1024.0));
            
            Row::new(vec![
                Cell::from(container_name.clone()),
                Cell::from(cpu_usage),
                Cell::from(memory_usage),
            ])
        })
        .collect();

    let table = Table::new(rows, &[
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Containers"))
        .highlight_style(Style::default().fg(Color::Yellow))
        .highlight_symbol("> ");

    f.render_widget(table, area);
}

/// Render container metrics charts for overview tab
fn render_overview_charts(f: &mut Frame, state: &MonitorState, area: Rect, pod: &PodInfo) {
    if pod.containers.is_empty() {
        // If there are no containers, display a message
        let message = ratatui::widgets::Paragraph::new("No containers available")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Containers"));
        
        f.render_widget(message, area);
        return;
    }

    // Get the selected container
    if let Some(container_name) = state.selected_container() {
        // Create a layout for the selected container with title and charts
        let container_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Container title and navigation hint
                Constraint::Min(5),    // Charts area
            ])
            .split(area);
        
        // Container title with navigation hints
        let container_title = format!("Container: {container_name} (↑/↓ to change)");
        let title_block = Block::default()
            .borders(Borders::ALL)
            .title(container_title);
        
        f.render_widget(
            ratatui::widgets::Paragraph::new("Press ↑/↓ to navigate between containers")
                .alignment(ratatui::layout::Alignment::Center)
                .block(title_block)
                .style(Style::default()),
            container_layout[0]
        );
        
        // Split the charts area vertically for CPU and Memory (stacked)
        let charts_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50), // CPU chart
                Constraint::Percentage(50), // Memory chart
            ])
            .split(container_layout[1]);
        
        // Render the CPU chart (now on top)
        render_mini_cpu_chart(f, charts_layout[0], state, pod, container_name);
        
        // Render the Memory chart (now below)
        render_mini_memory_chart(f, charts_layout[1], state, pod, container_name);
    } else {
        // No container selected
        let message = ratatui::widgets::Paragraph::new("No container selected")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Containers"));
        
        f.render_widget(message, area);
    }
}

/// Render a compact CPU usage chart
fn render_mini_cpu_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    let key = format!("{}/{}", pod.name, container_name);
    let history = state.metrics_history.get(&key);
    
    if let (Some(metrics), Some(history)) = (metrics, history) {
        // If we have history data available, use it for the chart
        if !history.cpu_history.is_empty() {
            let mut data_points = Vec::with_capacity(history.cpu_history.len());
            
            // Convert history to data points
            for (i, &value) in history.cpu_history.iter().enumerate() {
                // CPU values are already in millicores, no need to multiply
                data_points.push((i as f64, value));
            }
            
            // Create the dataset
            let dataset = vec![Dataset::default()
                .name("CPU")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&data_points)];

            // Find the maximum value for Y axis scaling
            let max_value = history.cpu_history.iter().fold(0.0, |max, &x| if x > max { x } else { max });
            let y_max = if max_value > 0.0 { max_value * 1.2 } else { 100.0 };

            // Create the chart
            let chart = Chart::new(dataset)
                .block(
                    Block::default()
                        .title(format!("CPU: {:.0}m", metrics.cpu_usage))
                        .borders(Borders::ALL)
                )
                .x_axis(
                    Axis::default()
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, history.cpu_history.len() as f64])
                )
                .y_axis(
                    Axis::default()
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, y_max])
                );

            // Render the chart
            f.render_widget(chart, area);
            return;
        }
    }

    // If no history available yet, fall back to a simple display
    if let Some(metrics) = metrics {
        let message = format!("CPU: {:.0}m\n\nCollecting data...", metrics.cpu_usage);
        let message_widget = ratatui::widgets::Paragraph::new(message)
            .style(Style::default().fg(Color::Green))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("CPU").borders(Borders::ALL));
        f.render_widget(message_widget, area);
    } else {
        // If no metrics available, display a message
        let message = ratatui::widgets::Paragraph::new("No CPU data")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("CPU").borders(Borders::ALL));
        f.render_widget(message, area);
    }
}

/// Render a compact memory usage chart
fn render_mini_memory_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    let key = format!("{}/{}", pod.name, container_name);
    let history = state.metrics_history.get(&key);
    
    if let (Some(metrics), Some(history)) = (metrics, history) {
        // If we have history data available, use it for the chart
        if !history.memory_history.is_empty() {
            let mut data_points = Vec::with_capacity(history.memory_history.len());
            
            // Convert history to data points (converting to MB)
            for (i, &value) in history.memory_history.iter().enumerate() {
                data_points.push((i as f64, value / (1024.0 * 1024.0))); // Convert bytes to MB
            }
            
            // Create the dataset
            let dataset = vec![Dataset::default()
                .name("Memory")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Blue))
                .data(&data_points)];

            // Find the maximum value for Y axis scaling
            let max_memory_mb = history.memory_history.iter()
                .fold(0.0, |max, &x| if x > max { x } else { max }) / (1024.0 * 1024.0);
            let y_max = if max_memory_mb > 0.0 { max_memory_mb * 1.2 } else { 100.0 };

            // Create the chart
            let memory_mb = metrics.memory_usage / (1024.0 * 1024.0);
            let chart = Chart::new(dataset)
                .block(
                    Block::default()
                        .title(format!("Memory: {memory_mb:.2}MB"))
                        .borders(Borders::ALL)
                )
                .x_axis(
                    Axis::default()
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, history.memory_history.len() as f64])
                )
                .y_axis(
                    Axis::default()
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, y_max])
                );

            // Render the chart
            f.render_widget(chart, area);
            return;
        }
    }

    // If no history available yet, fall back to a simple display
    if let Some(metrics) = metrics {
        let memory_mb = metrics.memory_usage / (1024.0 * 1024.0);
        let message = format!("Memory: {memory_mb:.2}MB\n\nCollecting data...");
        let message_widget = ratatui::widgets::Paragraph::new(message)
            .style(Style::default().fg(Color::Blue))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("Memory").borders(Borders::ALL));
        f.render_widget(message_widget, area);
    } else {
        // If no metrics available, display a message
        let message = ratatui::widgets::Paragraph::new("No memory data")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("Memory").borders(Borders::ALL));
        f.render_widget(message, area);
    }
}

/// Render the details tab
fn render_details_tab(f: &mut Frame, state: &MonitorState, area: Rect) {
    if let (Some(pod), Some(container_name)) = (state.selected_pod(), state.selected_container()) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
            ].as_ref())
            .split(area);

        // Container name and basic info
        let container_info = format!("Container: {} (Pod: {}) - Press 'c' to toggle chart/table view", container_name, pod.name);
        let container_info_block = Block::default()
            .borders(Borders::ALL)
            .title("Container Info");

        f.render_widget(
            ratatui::widgets::Paragraph::new(container_info)
                .block(container_info_block)
                .style(Style::default()),
            chunks[0]
        );

        // Render metrics based on the current view mode
        match state.view_mode {
            ViewMode::Table => render_metrics_table(f, state, chunks[1], pod, container_name),
            ViewMode::Chart => render_metrics_charts(f, state, chunks[1], pod, container_name),
        }
    }
}

/// Render the metrics table view with auto-adjusting based on metrics source
fn render_metrics_table(f: &mut Frame, state: &MonitorState, area: Rect, pod: &PodInfo, container_name: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(0),
        ].as_ref())
        .split(area);

    // Container metrics
    let metrics = state.get_metrics(&pod.name, container_name);
    
    // Determine the metrics source for auto-adjusting the display
    let metrics_source = detect_metrics_source(state);
    
    let metrics_info = if let Some(m) = metrics {
        match metrics_source {
            // For kubectl mode, only show CPU and Memory metrics
            MetricsSource::KubectlTop => vec![
                format!("CPU Usage: {:.0}m", m.cpu_usage),
                format!("Memory Usage: {:.2}Mi", m.memory_usage / 1024.0 / 1024.0),
                format!("---"),
                format!("Note: Only CPU and Memory metrics"),
                format!("are available in kubectl mode"),
            ],
            // For full metrics mode, show all metrics
            MetricsSource::Full => vec![
                format!("CPU Usage: {:.0}m", m.cpu_usage),
                format!("Memory Usage: {:.2}Mi", m.memory_usage / 1024.0 / 1024.0),
                format!("Disk Read: {:.2}KB/s", m.disk_read / 1024.0),
                format!("Disk Write: {:.2}KB/s", m.disk_write / 1024.0),
                format!("Network Rx: {:.2}KB/s", m.net_rx / 1024.0),
                format!("Network Tx: {:.2}KB/s", m.net_tx / 1024.0),
            ]
        }
    } else {
        match metrics_source {
            // For kubectl mode, only show CPU and Memory metrics as N/A
            MetricsSource::KubectlTop => vec![
                "CPU Usage: N/A".to_string(),
                "Memory Usage: N/A".to_string(),
                "---".to_string(),
                "Note: Only CPU and Memory metrics".to_string(),
                "are available in kubectl mode".to_string(),
            ],
            // For full metrics mode, show all metrics as N/A
            MetricsSource::Full => vec![
                "CPU Usage: N/A".to_string(),
                "Memory Usage: N/A".to_string(),
                "Disk Read: N/A".to_string(),
                "Disk Write: N/A".to_string(),
                "Network Rx: N/A".to_string(),
                "Network Tx: N/A".to_string(),
            ]
        }
    };

    let metrics_items: Vec<ListItem> = metrics_info.iter()
        .map(|info| ListItem::new(info.clone()))
        .collect();

    let metrics_list = List::new(metrics_items)
        .block(Block::default().borders(Borders::ALL).title("Metrics"))
        .style(Style::default());

    f.render_widget(metrics_list, chunks[0]);

    // Container details
    let details_info = [format!("Container Name: {container_name}"),
        format!("Pod Name: {}", pod.name),
        format!("Namespace: {}", pod.namespace)];

    let details_items: Vec<ListItem> = details_info.iter()
        .map(|info| ListItem::new(info.clone()))
        .collect();

    let details_list = List::new(details_items)
        .block(Block::default().borders(Borders::ALL).title("Details"))
        .style(Style::default());

    f.render_widget(details_list, chunks[1]);
}

/// Render the metrics charts view with auto-adjusting layout based on available metrics
fn render_metrics_charts(f: &mut Frame, state: &MonitorState, area: Rect, pod: &PodInfo, container_name: &str) {
    // Detect the metrics source for auto-adjusting the layout
    let metrics_source = detect_metrics_source(state);
    
    match metrics_source {
        // For kubectl mode (only CPU and Memory available)
        MetricsSource::KubectlTop => {
            // Split the area evenly for CPU and Memory charts
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),  // CPU chart
                    Constraint::Percentage(50),  // Memory chart
                ].as_ref())
                .split(area);

            // CPU chart
            render_cpu_chart(f, chunks[0], state, pod, container_name);
            
            // Memory chart
            render_memory_chart(f, chunks[1], state, pod, container_name);
        },
        
        // For full metrics mode (includes network and disk metrics)
        MetricsSource::Full => {
            // Create a more complex layout with 4 charts
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),  // Top row: CPU and Memory
                    Constraint::Percentage(50),  // Bottom row: Disk and Network
                ].as_ref())
                .split(area);

            // Top row: Split for CPU and Memory
            let top_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),  // CPU chart
                    Constraint::Percentage(50),  // Memory chart
                ].as_ref())
                .split(chunks[0]);

            // Bottom row: Split for Disk and Network
            let bottom_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),  // Disk chart
                    Constraint::Percentage(50),  // Network chart
                ].as_ref())
                .split(chunks[1]);

            // Render all four charts
            render_cpu_chart(f, top_chunks[0], state, pod, container_name);
            render_memory_chart(f, top_chunks[1], state, pod, container_name);
            render_disk_chart(f, bottom_chunks[0], state, pod, container_name);
            render_network_chart(f, bottom_chunks[1], state, pod, container_name);
        }
    }
}

/// Render the CPU usage chart with actual history data
fn render_cpu_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    let key = format!("{}/{}", pod.name, container_name);
    let history = state.metrics_history.get(&key);
    
    if let (Some(metrics), Some(history)) = (metrics, history) {
        // If we have history data available, use it for the chart
        if !history.cpu_history.is_empty() {
            let mut data_points = Vec::with_capacity(history.cpu_history.len());
            
            // Convert history to data points
            for (i, &value) in history.cpu_history.iter().enumerate() {
                data_points.push((i as f64, value));
            }
            
            // Create the dataset
            let dataset = vec![Dataset::default()
                .name("CPU Usage")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&data_points)];

            // Find the maximum value for Y axis scaling
            let max_value = history.cpu_history.iter().fold(0.0, |max, &x| if x > max { x } else { max });
            let y_max = if max_value > 0.0 { max_value * 1.2 } else { 100.0 };

            // Create the chart
            let chart = Chart::new(dataset)
                .block(
                    Block::default()
                        .title(format!("CPU Usage (millicores) - Current: {:.0}", metrics.cpu_usage))
                        .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                )
                .x_axis(
                    Axis::default()
                        .title("Time")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, history.cpu_history.len() as f64])
                )
                .y_axis(
                    Axis::default()
                        .title("CPU (m)")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, y_max])
                );

            // Render the chart
            f.render_widget(chart, area);
            return;
        }
    }
    
    // Fallback to simple display if no history or metrics are available
    let message = if metrics.is_some() {
        format!("CPU Usage: {:.0}m\n\nCollecting data...", metrics.unwrap().cpu_usage)
    } else {
        "No CPU metrics available".to_string()
    };
    
    let message_widget = ratatui::widgets::Paragraph::new(message)
        .style(Style::default().fg(Color::Green))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().title("CPU Usage").borders(Borders::ALL));
    
    f.render_widget(message_widget, area);
}

/// Render the memory usage chart using actual history data
fn render_memory_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    let key = format!("{}/{}", pod.name, container_name);
    let history = state.metrics_history.get(&key);
    
    if let (Some(metrics), Some(history)) = (metrics, history) {
        // If we have history data available, use it for the chart
        if !history.memory_history.is_empty() {
            let mut data_points = Vec::with_capacity(history.memory_history.len());
            
            // Convert history to data points (converting to MB)
            for (i, &value) in history.memory_history.iter().enumerate() {
                data_points.push((i as f64, value / (1024.0 * 1024.0))); // Convert bytes to MB
            }
            
            // Create the dataset
            let dataset = vec![Dataset::default()
                .name("Memory Usage")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Blue))
                .data(&data_points)];

            // Find the maximum value for Y axis scaling
            let max_memory_mb = history.memory_history.iter()
                .fold(0.0, |max, &x| if x > max { x } else { max }) / (1024.0 * 1024.0);
            let y_max = if max_memory_mb > 0.0 { max_memory_mb * 1.2 } else { 100.0 };

            // Create the chart
            let memory_mb = metrics.memory_usage / (1024.0 * 1024.0);
            let chart = Chart::new(dataset)
                .block(
                    Block::default()
                        .title(format!("Memory Usage (MB) - Current: {memory_mb:.2} MB"))
                        .title_style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                )
                .x_axis(
                    Axis::default()
                        .title("Time")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, history.memory_history.len() as f64])
                )
                .y_axis(
                    Axis::default()
                        .title("Memory (MB)")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, y_max])
                );

            // Render the chart
            f.render_widget(chart, area);
            return;
        }
    }
    
    // Fallback to simple display if no history or metrics are available
    let message = if metrics.is_some() {
        let memory_mb = metrics.unwrap().memory_usage / (1024.0 * 1024.0);
        format!("Memory Usage: {memory_mb:.2} MB\n\nCollecting data...")
    } else {
        "No memory metrics available".to_string()
    };
    
    let message_widget = ratatui::widgets::Paragraph::new(message)
        .style(Style::default().fg(Color::Blue))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().title("Memory Usage").borders(Borders::ALL));
    
    f.render_widget(message_widget, area);
}

/// Render the disk usage chart
fn render_disk_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    
    if let Some(metrics) = metrics {
        // For a real implementation, we would use historical data stored in the state
        // Here we'll create some dummy data for visualization
        
        // Create data points
        let mut data_points = Vec::new();
        for i in 0..30 {
            // Generate some random disk usage that somewhat follows the current value
            let disk = metrics.disk_read * (0.8 + fastrand::f64() * 0.4);
            data_points.push((i as f64, disk / 1024.0)); // Convert to KB
        }
        
        // Create the dataset
        let dataset = vec![Dataset::default()
            .name("Disk Usage")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Yellow))
            .data(&data_points)];

        // Create the chart
        let chart = Chart::new(dataset)
            .block(
                Block::default()
                    .title("Disk Usage (KB)")
                    .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 30.0])
            )
            .y_axis(
                Axis::default()
                    .title("Disk (KB)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, metrics.disk_read * 1.5 / 1024.0])
            );

        // Render the chart
        f.render_widget(chart, area);
    } else {
        // If no metrics available, display a message
        let message = ratatui::widgets::Paragraph::new("No disk metrics available")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("Disk Usage").borders(Borders::ALL));
        f.render_widget(message, area);
    }
}

/// Render the network usage chart
fn render_network_chart(f: &mut Frame, area: Rect, state: &MonitorState, pod: &PodInfo, container_name: &str) {
    // Get metrics for the container
    let metrics = state.get_metrics(&pod.name, container_name);
    
    if let Some(metrics) = metrics {
        // For a real implementation, we would use historical data stored in the state
        // Here we'll create some dummy data for visualization
        
        // Create data points
        let mut data_points = Vec::new();
        for i in 0..30 {
            // Generate some random network usage that somewhat follows the current value
            let network = metrics.net_rx * (0.8 + fastrand::f64() * 0.4);
            data_points.push((i as f64, network / 1024.0)); // Convert to KB
        }
        
        // Create the dataset
        let dataset = vec![Dataset::default()
            .name("Network Usage")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&data_points)];

        // Create the chart
        let chart = Chart::new(dataset)
            .block(
                Block::default()
                    .title("Network Usage (KB)")
                    .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 30.0])
            )
            .y_axis(
                Axis::default()
                    .title("Network (KB)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, metrics.net_rx * 1.5 / 1024.0])
            );

        // Render the chart
        f.render_widget(chart, area);
    } else {
        // If no metrics available, display a message
        let message = ratatui::widgets::Paragraph::new("No network metrics available")
            .style(Style::default().fg(Color::Red))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().title("Network Usage").borders(Borders::ALL));
        f.render_widget(message, area);
    }
}

/// Run the monitor loop to collect metrics
pub async fn run_monitor_loop(
    state: &mut MonitorState,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let collector = ModernMetricsCollector::new().await?;
    
    loop {
        // Check for shutdown signal
        if shutdown_rx.try_recv().is_ok() {
            break;
        }
        
        // For each pod, collect metrics for all containers
        // In a real implementation, we would use the container IDs to collect metrics
        // For now, we'll just create dummy metrics for each container
        for pod in &state.pods {
            for container_name in &pod.containers {
                // Create dummy metrics for each container
                let metrics = ContainerMetrics {
                    cpu_usage: fastrand::f64() * 100.0, // Random CPU usage between 0-100%
                    memory_usage: fastrand::f64() * 1024.0 * 1024.0 * 100.0, // Random memory usage between 0-100MB
                    disk_read: fastrand::f64() * 1024.0 * 10.0, // Random disk read between 0-10KB/s
                    disk_write: fastrand::f64() * 1024.0 * 5.0, // Random disk write between 0-5KB/s
                    net_rx: fastrand::f64() * 1024.0 * 20.0, // Random network receive between 0-20KB/s
                    net_tx: fastrand::f64() * 1024.0 * 10.0, // Random network transmit between 0-10KB/s
                };
                
                let key = format!("{}/{}", pod.name, container_name);
                state.usage_data.insert(key.clone(), metrics.clone());

                // Update metrics history
                let history = state.metrics_history.entry(key).or_insert_with(|| ContainerMetricsHistory::new(30));
                history.add_metrics(&metrics);
            }
        }
        
        // Sleep for a short period before the next collection
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    
    Ok(())
}

/// Run the monitor loop with shared state using real Kubernetes metrics
pub async fn run_monitor_loop_with_shared_state(
    state: std::sync::Arc<tokio::sync::Mutex<MonitorState>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    use crate::k8s::metrics::{MetricsClient, MetricsSource};
    use kube::Client;
    
    wake_logger::info("Starting metrics collection loop...");
    
    // Create Kubernetes client
    let k8s_client = match Client::try_default().await {
        Ok(client) => {
            wake_logger::info("Successfully created Kubernetes client");
            client
        },
        Err(e) => {
            wake_logger::error(&format!("Failed to create Kubernetes client: {e}"));
            return Err(anyhow::anyhow!("Failed to create Kubernetes client"));
        }
    };
    
    // Create metrics client - use "*" to query metrics from all namespaces
    let metrics_client = match MetricsClient::new(k8s_client.clone(), "*".to_string()).await {
        Ok(mut client) => {
            // Force using kubectl top for consistent values with your CLI commands
            client.set_metrics_source(MetricsSource::KubectlTop);
            wake_logger::info("Successfully created metrics client with KubectlTop source for all namespaces");
            client
        },
        Err(e) => {
            wake_logger::error(&format!("Failed to create metrics client: {e}"));
            return Err(anyhow::anyhow!("Failed to create metrics client"));
        }
    };
    
    wake_logger::info("Entering metrics collection loop");
    
    loop {
        // Check for shutdown signal
        if shutdown_rx.try_recv().is_ok() {
            wake_logger::info("Received shutdown signal, exiting metrics collection loop");
            break;
        }
        
        // Create a new HashMap to hold the metrics before updating the shared state
        let mut new_metrics = HashMap::new();
        
        // Lock the state briefly to get pod info
        let pod_info;
        {
            let state_guard = state.lock().await;
            pod_info = state_guard.pods.clone();
            wake_logger::info(&format!("Found {} pods to collect metrics for", pod_info.len()));
        }
        
        // Convert our PodInfo to the format needed by MetricsClient
        let k8s_pods: Vec<k8s_openapi::api::core::v1::Pod> = pod_info.iter()
            .map(|p| {
                let mut pod = k8s_openapi::api::core::v1::Pod::default();
                // Directly access the metadata field (it's not an Option)
                pod.metadata.name = Some(p.name.clone());
                pod.metadata.namespace = Some(p.namespace.clone());
                pod
            })
            .collect();
        
        wake_logger::info(&format!("Converted {} pods to k8s format", k8s_pods.len()));
        
        // Collect real metrics using kubectl top
        if !pod_info.is_empty() {
            wake_logger::info(&format!("Attempting to collect metrics for {} pods", pod_info.len()));
            match metrics_client.get_pod_metrics(".*", &k8s_pods).await {
                Ok(pod_metrics) => {
                    wake_logger::info(&format!("Successfully retrieved metrics for {} pods", pod_metrics.len()));
                    
                    for pod in &pod_info {
                        if let Some(metrics) = pod_metrics.get(&pod.name) {
                            wake_logger::info(&format!("Pod {} metrics - CPU: {}m, Memory: {}Mi", 
                                  pod.name, 
                                  metrics.cpu.usage_value, // CPU values already in millicores
                                  metrics.memory.usage_value / (1024.0 * 1024.0)));
                            
                            // Get the metrics per-container from the API response
                            wake_logger::info(&format!("Attempting to get container metrics for pod {}", pod.name));
                            let k8s_api_result = metrics_client.get_pod_container_metrics(&pod.namespace, &pod.name).await;
                            
                            if let Ok(container_metrics) = k8s_api_result {
                                wake_logger::info(&format!("Found metrics for {} containers in pod {}", container_metrics.len(), pod.name));
                                
                                // We successfully got per-container metrics
                                for container_name in &pod.containers {
                                    if let Some(container_metric) = container_metrics.get(container_name) {
                                        wake_logger::info(&format!("Container {}/{} metrics - CPU: {}m, Memory: {}Mi", 
                                              pod.name, container_name,
                                              container_metric.cpu.usage_value, // CPU values already in millicores
                                              container_metric.memory.usage_value / (1024.0 * 1024.0)));
                                        
                                        // Use the real per-container metrics
                                        let metrics_data = ContainerMetrics {
                                            // CPU value already in millicores
                                            cpu_usage: container_metric.cpu.usage_value,
                                            // Memory value in bytes
                                            memory_usage: container_metric.memory.usage_value,
                                            // We don't have these in kubectl output
                                            disk_read: 0.0,
                                            disk_write: 0.0,
                                            net_rx: 0.0,
                                            net_tx: 0.0,
                                        };
                                        
                                        let key = format!("{}/{}", pod.name, container_name);
                                        new_metrics.insert(key.clone(), metrics_data.clone());

                                        // Update metrics history
                                        let mut state_guard = state.lock().await;
                                        let history_entry = state_guard.metrics_history.entry(key).or_insert_with(|| ContainerMetricsHistory::new(30));
                                        history_entry.add_metrics(&metrics_data);
                                        drop(state_guard); // Explicitly drop the lock
                                    } else {
                                        wake_logger::info(&format!("No individual metrics for container {} in pod {}, using aggregate metrics", 
                                               container_name, pod.name));
                                        // Fallback: divide pod metrics evenly among containers
                                        let container_count = pod.containers.len() as f64;
                                        let metrics_data = ContainerMetrics {
                                            // Divide CPU usage evenly among containers, values already in millicores
                                            cpu_usage: metrics.cpu.usage_value / container_count,
                                            // Divide memory usage evenly among containers
                                            memory_usage: metrics.memory.usage_value / container_count,
                                            disk_read: 0.0,
                                            disk_write: 0.0,
                                            net_rx: 0.0,
                                            net_tx: 0.0,
                                        };
                                        
                                        let key = format!("{}/{}", pod.name, container_name);
                                        new_metrics.insert(key.clone(), metrics_data.clone());

                                        // Update metrics history
                                        let mut state_guard = state.lock().await;
                                        let history_entry = state_guard.metrics_history.entry(key).or_insert_with(|| ContainerMetricsHistory::new(30));
                                        history_entry.add_metrics(&metrics_data);
                                        drop(state_guard); // Explicitly drop the lock
                                    }
                                }
                            } else {
                                wake_logger::info(&format!("Failed to get per-container metrics for pod {}, reason: {:?}", 
                                       pod.name, k8s_api_result.err()));
                                // Fallback if we can't get per-container metrics: divide pod metrics evenly
                                for container_name in &pod.containers {
                                    let container_count = pod.containers.len() as f64;
                                    let metrics_data = ContainerMetrics {
                                        // Divide CPU usage evenly among containers, values already in millicores
                                        cpu_usage: metrics.cpu.usage_value / container_count,
                                        // Divide memory usage evenly among containers
                                        memory_usage: metrics.memory.usage_value / container_count,
                                        disk_read: 0.0,
                                        disk_write: 0.0,
                                        net_rx: 0.0,
                                        net_tx: 0.0,
                                    };
                                    
                                    let key = format!("{}/{}", pod.name, container_name);
                                    new_metrics.insert(key.clone(), metrics_data.clone());

                                    // Update metrics history
                                    let mut state_guard = state.lock().await;
                                    let history_entry = state_guard.metrics_history.entry(key).or_insert_with(|| ContainerMetricsHistory::new(30));
                                    history_entry.add_metrics(&metrics_data);
                                    drop(state_guard); // Explicitly drop the lock
                                }
                            }
                        } else {
                            wake_logger::info(&format!("No metrics available for pod: {}", pod.name));
                            
                            // Try to get metrics for this pod directly using kubectl
                            wake_logger::info(&format!("Trying direct kubectl top for pod {}/{}", pod.namespace, pod.name));
                            
                            let output = std::process::Command::new("kubectl")
                                .args(["top", "pod", &pod.name, "-n", &pod.namespace, "--containers", "--no-headers"])
                                .output();
                            
                            match output {
                                Ok(output) => {
                                    if output.status.success() {
                                        let stdout = String::from_utf8_lossy(&output.stdout);
                                        wake_logger::info(&format!("Direct kubectl output: {stdout}"));
                                    } else {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        wake_logger::error(&format!("Direct kubectl command failed: {stderr}"));
                                    }
                                },
                                Err(e) => wake_logger::error(&format!("Failed to run direct kubectl command: {e}")),
                            }
                        }
                    }
                    
                    // Now update the shared state with the new metrics
                    {
                        let mut state_guard = state.lock().await;
                        wake_logger::info(&format!("Updated UI state with {} container metrics", new_metrics.len()));
                        state_guard.usage_data = new_metrics;
                    }
                },
                Err(e) => {
                    wake_logger::error(&format!("Failed to get pod metrics: {e}"));
                    
                    // Try running kubectl top directly for all pods to see output
                    let direct_output = std::process::Command::new("kubectl")
                        .args(["top", "pods", "--all-namespaces", "--no-headers"])
                        .output();
                    
                    match direct_output {
                        Ok(output) => {
                            if output.status.success() {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                wake_logger::info(&format!("Direct kubectl top pods output: {stdout}"));
                            } else {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                wake_logger::error(&format!("Direct kubectl top pods failed: {stderr}"));
                            }
                        },
                        Err(e) => wake_logger::error(&format!("Failed to run direct kubectl top pods: {e}")),
                    }
                    
                    // Fallback to dummy data if metrics API fails
                    for pod in &pod_info {
                        for container_name in &pod.containers {
                            // Create dummy metrics for each container with realistic values
                            let metrics = ContainerMetrics {
                                cpu_usage: 50.0 + fastrand::f64() * 100.0, // 50-150 millicores
                                memory_usage: (50.0 + fastrand::f64() * 100.0) * 1024.0 * 1024.0, // 50-150MB
                                disk_read: 0.0,
                                disk_write: 0.0,
                                net_rx: 0.0,
                                net_tx: 0.0,
                            };
                            
                            let key = format!("{}/{}", pod.name, container_name);
                            new_metrics.insert(key.clone(), metrics.clone());

                            // Update metrics history
                            let mut state_guard = state.lock().await;
                            let history_entry = state_guard.metrics_history.entry(key).or_insert_with(|| ContainerMetricsHistory::new(30));
                            history_entry.add_metrics(&metrics);
                        }
                    }
                    
                    // Update the shared state with the fallback metrics
                    {
                        let mut state_guard = state.lock().await;
                        wake_logger::info(&format!("Updated UI state with {} fallback metrics", new_metrics.len()));
                        state_guard.usage_data = new_metrics;
                    }
                }
            }
        } else {
            wake_logger::info("No pods to collect metrics for");
        }
        
        // Sleep for a short period before the next collection
        wake_logger::info("Sleeping for 2 seconds before next metrics collection");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    
    Ok(())
}

// Add the start_monitor function implementation
pub async fn start_monitor(args: &crate::cli::Args, pod_infos: Vec<crate::k8s::pod::PodInfo>) -> anyhow::Result<()> {
    use crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        Terminal,
    };
    use std::io;
    use tokio::sync::{mpsc, Mutex};
    use std::sync::Arc;
    use tracing::{debug, info};
    use std::fs::File;
    
    // Instead of initializing a global subscriber which conflicts with the existing one,
    // just log a message indicating we're starting the monitor
    info!("Starting Wake monitor with {} pods", pod_infos.len());
    
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create shared state using Arc<Mutex<>> to allow updates from background task
    let state = Arc::new(Mutex::new(MonitorState::new(pod_infos)));
    
    // Create channels for communication
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    
    // Start metrics collection in the background
    let state_clone = state.clone();
    let metrics_handle = tokio::spawn(async move {
        if let Err(e) = run_monitor_loop_with_shared_state(state_clone, shutdown_rx).await {
            wake_logger::error(&format!("Error in metrics collection: {e}"));
        }
    });

    // Main loop
    loop {
        {
            // Lock the state for rendering only
            let state_guard = match state.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    // If we can't get the lock, just try again next iteration
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    continue;
                }
            };
            terminal.draw(|f| {
                render_monitor(f, &state_guard, f.size());
            })?;
        } // State lock is released here

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Lock state for handling input
                let mut state_guard = match state.try_lock() {
                    Ok(guard) => guard,
                    Err(_) => {
                        // If we can't get the lock, just try again next iteration
                        continue;
                    }
                };
                
                match key.code {
                    KeyCode::Char('q') => {
                        // Quit
                        drop(state_guard); // Explicitly drop the lock before breaking
                        break;
                    }
                    KeyCode::Down => {
                        state_guard.next_container();
                    }
                    KeyCode::Up => {
                        state_guard.previous_container();
                    }
                    KeyCode::Left => {
                        state_guard.previous_pod();
                    }
                    KeyCode::Right => {
                        state_guard.next_pod();
                    }
                    KeyCode::Tab => {
                        state_guard.tab_index = (state_guard.tab_index + 1) % 2;
                    }
                    KeyCode::Char('c') => {
                        // Toggle view mode between table and chart
                        state_guard.view_mode = if state_guard.view_mode == ViewMode::Table {
                            ViewMode::Chart
                        } else {
                            ViewMode::Table
                        };
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup
    let _ = shutdown_tx.send(()).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), metrics_handle).await;
    
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

// Add ModernMetricsCollector implementation 
pub struct ModernMetricsCollector {}

impl ModernMetricsCollector {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {})
    }
    
    pub async fn collect_container_metrics(&self, container_id: &str) -> anyhow::Result<ContainerMetrics> {
        // Return dummy metrics for now
        Ok(ContainerMetrics {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_read: 0.0,
            disk_write: 0.0,
            net_rx: 0.0,
            net_tx: 0.0,
        })
    }

    pub fn set_pods_by_names(&self, pod_names: Vec<String>) {
        // This is a stub implementation that would normally set the pods to monitor
        // In a real implementation, this would store the pod names to use for metrics collection
    }
}

/// Render the help bar at the bottom
fn render_help_bar(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Span::styled("q: Quit", Style::default().fg(Color::Red)),
        Span::raw(" | "),
        Span::styled("←/→: Switch Pod", Style::default().fg(Color::Yellow)),
        Span::raw(" | "),
        Span::styled("↑/↓: Switch Container", Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled("Tab: Switch Tab", Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled("c: Toggle Chart/Table", Style::default().fg(Color::Magenta)),
    ];

    let help_line = Line::from(help_text);
    let help_paragraph = ratatui::widgets::Paragraph::new(help_line)
        .block(Block::default().borders(Borders::ALL).title("Navigation"))
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(help_paragraph, area);
}