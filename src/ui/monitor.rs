use anyhow::Result;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, Tabs, Paragraph},
    Frame,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time;
use tokio::sync::mpsc;

use crate::k8s::pod::PodInfo;
use crate::k8s::metrics::{MetricsClient, MetricsSource};
use kube::Client;

#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub timestamp: Instant,
    pub cpu_usage: f64,    // CPU usage in percentage
    pub memory_usage: f64, // Memory usage in MB
}

#[derive(Debug, Clone)]
pub struct MonitorState {
    pub selected_pod_index: usize,
    pub selected_container_index: usize,
    pub pods: Vec<PodInfo>,
    pub usage_data: HashMap<String, Vec<ResourceUsage>>, // Key: "pod_name/container_name"
    pub update_interval: Duration,
    pub last_update: Instant,
    pub tab_index: usize,
    pub metrics_source: MetricsSource,
}

impl MonitorState {
    pub fn new(pods: Vec<PodInfo>) -> Self {
        Self {
            selected_pod_index: 0,
            selected_container_index: 0,
            pods,
            usage_data: HashMap::new(),
            update_interval: Duration::from_secs(2),
            last_update: Instant::now(),
            tab_index: 0,
            metrics_source: MetricsSource::KubectlTop, // Default to kubectl top for backward compatibility
        }
    }

    pub fn set_metrics_source(&mut self, source: MetricsSource) {
        self.metrics_source = source;
    }

    pub fn get_selected_pod(&self) -> Option<&PodInfo> {
        self.pods.get(self.selected_pod_index)
    }

    pub fn get_selected_container_name(&self) -> Option<String> {
        self.get_selected_pod().and_then(|pod| {
            pod.containers.get(self.selected_container_index).cloned()
        })
    }

    pub fn next_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod_index = (self.selected_pod_index + 1) % self.pods.len();
            self.selected_container_index = 0;
        }
    }

    pub fn previous_pod(&mut self) {
        if !self.pods.is_empty() {
            self.selected_pod_index = if self.selected_pod_index > 0 {
                self.selected_pod_index - 1
            } else {
                self.pods.len() - 1
            };
            self.selected_container_index = 0;
        }
    }

    pub fn next_container(&mut self) {
        if let Some(pod) = self.get_selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = (self.selected_container_index + 1) % pod.containers.len();
            }
        }
    }

    pub fn previous_container(&mut self) {
        if let Some(pod) = self.get_selected_pod() {
            if !pod.containers.is_empty() {
                self.selected_container_index = if self.selected_container_index > 0 {
                    self.selected_container_index - 1
                } else {
                    pod.containers.len() - 1
                };
            }
        }
    }

    pub fn get_usage_key(&self) -> Option<String> {
        if let (Some(pod), Some(container)) = (self.get_selected_pod(), self.get_selected_container_name()) {
            Some(format!("{}/{}", pod.name, container))
        } else {
            None
        }
    }

    pub fn should_update(&self) -> bool {
        self.last_update.elapsed() >= self.update_interval
    }
}

pub async fn run_monitor_loop(
    monitor_state: &mut MonitorState,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let mut interval = time::interval(Duration::from_secs(1));
    
    // Create Kubernetes client
    let client = Client::try_default().await?;
    
    // Create a metrics client that will use either API or kubectl top
    let mut metrics_client = MetricsClient::new(client, "*".to_string()).await?;
    
    // Set the preferred metrics source
    metrics_client.set_metrics_source(monitor_state.metrics_source);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if monitor_state.should_update() {
                    // Update resource usage for all pods
                    for pod in &monitor_state.pods {
                        // Create a regex that matches only this pod
                        let pod_selector = format!("^{}$", regex::escape(&pod.name));
                        
                        // Convert PodInfo to Pod objects for the metrics client
                        // We only need empty Pod objects with the correct metadata.name
                        let mut k8s_pods = Vec::new();
                        let k8s_pod = k8s_openapi::api::core::v1::Pod {
                            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                name: Some(pod.name.clone()),
                                namespace: Some(pod.namespace.clone()),
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        k8s_pods.push(k8s_pod);
                        
                        // Get metrics for this pod using the constructed Pod object
                        if let Ok(pod_metrics) = metrics_client.get_pod_metrics(&pod_selector, &k8s_pods).await {
                            if let Some(metrics) = pod_metrics.get(&pod.name) {
                                for container_name in &pod.containers {
                                    // Get container metrics
                                    let key = format!("{}/{}", pod.name, container_name);
                                    
                                    // Create a new usage entry
                                    let usage = ResourceUsage {
                                        timestamp: Instant::now(),
                                        // Convert CPU to percentage (0-100)
                                        cpu_usage: metrics.cpu.usage_value * 100.0,
                                        // Memory already in MB in the ResourceMetrics struct
                                        memory_usage: metrics.memory.usage_value / (1024.0 * 1024.0),
                                    };
                                    
                                    // Make sure the entry exists in the map
                                    if !monitor_state.usage_data.contains_key(&key) {
                                        monitor_state.usage_data.insert(key.clone(), Vec::new());
                                    }
                                    
                                    // Add the new usage data
                                    if let Some(data) = monitor_state.usage_data.get_mut(&key) {
                                        data.push(usage);
                                        
                                        // Keep only the last 60 data points
                                        if data.len() > 60 {
                                            data.remove(0);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    monitor_state.last_update = Instant::now();
                }
            },
            _ = shutdown_rx.recv() => {
                // Exit the monitoring loop when shutdown signal is received
                break;
            }
        }
    }

    Ok(())
}

pub fn render_monitor(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    // Create the layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Pod/container selection tabs
            Constraint::Min(10),   // Resource usage display
        ])
        .split(area);

    render_selection_tabs(f, state, chunks[0]);
    
    match state.tab_index {
        0 => render_table_view(f, state, chunks[1]),
        1 => render_chart_view(f, state, chunks[1]),
        _ => {}
    }
}

fn render_selection_tabs(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    let pod_name = state.get_selected_pod().map_or("No pods".to_string(), |p| p.name.clone());
    let container_name = state.get_selected_container_name().unwrap_or_else(|| "No containers".to_string());
    
    let titles = vec![
        Line::from(vec![
            Span::styled("Pod: ", Style::default().fg(Color::Gray)),
            Span::styled(&pod_name, Style::default().fg(Color::Green)),
            Span::raw(" | "),
            Span::styled("Container: ", Style::default().fg(Color::Gray)),
            Span::styled(&container_name, Style::default().fg(Color::Yellow)),
        ]),
    ];
    
    let view_tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(0);
    
    f.render_widget(view_tabs, area);
}

fn render_table_view(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    let usage_key = state.get_usage_key();
    let (current_cpu, current_mem, avg_cpu, avg_mem) = if let Some(key) = usage_key {
        if let Some(data) = state.usage_data.get(&key) {
            if !data.is_empty() {
                let current = data.last().unwrap();
                let avg_cpu = data.iter().map(|u| u.cpu_usage).sum::<f64>() / data.len() as f64;
                let avg_mem = data.iter().map(|u| u.memory_usage).sum::<f64>() / data.len() as f64;
                (current.cpu_usage, current.memory_usage, avg_cpu, avg_mem)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    // Create the table
    let header = Row::new(vec![
        Cell::from(Span::styled("Metric", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Current", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Average", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
    ]);

    let rows = vec![
        Row::new(vec![
            Cell::from("CPU Usage (%)"),
            Cell::from(format!("{:.2}%", current_cpu)),
            Cell::from(format!("{:.2}%", avg_cpu)),
        ]),
        Row::new(vec![
            Cell::from("Memory (MB)"),
            Cell::from(format!("{:.2} MB", current_mem)),
            Cell::from(format!("{:.2} MB", avg_mem)),
        ]),
    ];

    let table = Table::new(rows, &[
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .header(header)
        .block(Block::default()
            .title(Span::styled("Resource Usage", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL));

    f.render_widget(table, area);
}

fn render_chart_view(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    // Simple text-based chart representation
    let usage_key = state.get_usage_key();
    let message = if let Some(key) = usage_key {
        if let Some(data) = state.usage_data.get(&key) {
            if !data.is_empty() {
                format!("Monitoring: {} - CPU: {:.2}%, Memory: {:.2} MB", 
                    key, 
                    data.last().unwrap().cpu_usage,
                    data.last().unwrap().memory_usage)
            } else {
                "No data collected yet".to_string()
            }
        } else {
            "No data available".to_string()
        }
    } else {
        "No pod/container selected".to_string()
    };

    let para = Paragraph::new(message)
        .block(Block::default()
            .title("Resource Usage Chart")
            .borders(Borders::ALL));

    f.render_widget(para, area);
}