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
                        
                        // Get metrics for this pod using kubectl directly for more reliability
                        let namespace = &pod.namespace;
                        let pod_name = &pod.name;
                        
                        // Execute kubectl top pod command directly
                        let output = std::process::Command::new("kubectl")
                            .args(&["top", "pod", pod_name, "-n", namespace, "--containers", "--no-headers"])
                            .output();
                        
                        match output {
                            Ok(output) if output.status.success() => {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                
                                // Parse each container's usage
                                for line in stdout.lines() {
                                    let parts: Vec<&str> = line.split_whitespace().collect();
                                    if parts.len() >= 3 {
                                        let container_name = if parts.len() >= 4 {
                                            parts[1].to_string()
                                        } else {
                                            // If format is unexpected, use "default"
                                            "default".to_string()
                                        };
                                        
                                        // Parse CPU usage (remove "m" suffix and convert to percentage)
                                        let cpu_str = if parts.len() >= 4 { parts[2] } else { parts[1] };
                                        let cpu_usage = if cpu_str.ends_with('m') {
                                            // Convert millicores to percentage (1000m = 1 core = 100%)
                                            let millicores = cpu_str.trim_end_matches('m')
                                                .parse::<f64>()
                                                .unwrap_or(0.0);
                                            millicores / 10.0 // 1000m = 100%
                                        } else {
                                            // Direct core count to percentage
                                            cpu_str.parse::<f64>()
                                                .unwrap_or(0.0) * 100.0
                                        };
                                        
                                        // Parse Memory usage
                                        let mem_str = if parts.len() >= 4 { parts[3] } else { parts[2] };
                                        
                                        // Better memory parsing to handle all formats (Ki, Mi, Gi, etc.)
                                        let mem_value: f64;
                                        let mem_unit = mem_str.chars()
                                            .skip_while(|c| c.is_digit(10) || *c == '.')
                                            .collect::<String>();
                                        let mem_number = mem_str.chars()
                                            .take_while(|c| c.is_digit(10) || *c == '.')
                                            .collect::<String>()
                                            .parse::<f64>()
                                            .unwrap_or(0.0);
                                            
                                        // Convert to MB based on unit
                                        mem_value = match mem_unit.as_str() {
                                            "Ki" => mem_number / 1024.0,
                                            "Mi" => mem_number,
                                            "Gi" => mem_number * 1024.0,
                                            "Ti" => mem_number * 1024.0 * 1024.0,
                                            "K" | "k" => mem_number / 1000.0,
                                            "M" => mem_number,
                                            "G" => mem_number * 1000.0,
                                            "T" => mem_number * 1000.0 * 1000.0,
                                            _ => mem_number / (1024.0 * 1024.0), // Assume bytes if no unit
                                        };
                                        
                                        // Only process if the container is in this pod's containers list
                                        if pod.containers.contains(&container_name) {
                                            // Create a usage key
                                            let key = format!("{}/{}", pod_name, container_name);
                                            
                                            // Create a new usage entry
                                            let usage = ResourceUsage {
                                                timestamp: Instant::now(),
                                                cpu_usage: cpu_usage,
                                                memory_usage: mem_value,
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
                            },
                            Ok(output) => {
                                eprintln!("Failed to get metrics: {}", String::from_utf8_lossy(&output.stderr));
                            },
                            Err(e) => {
                                eprintln!("Error executing kubectl: {}", e);
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
    // Split the area for pod and container dropdowns
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Pod dropdown
            Constraint::Percentage(50), // Container dropdown
        ])
        .split(area);
    
    // Create dropdown style for pod selection
    render_pod_dropdown(f, state, chunks[0]);
    render_container_dropdown(f, state, chunks[1]);
}

fn render_pod_dropdown(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    let pod_name = state.get_selected_pod().map_or("No pods".to_string(), |p| p.name.clone());
    
    // Create dropdown-like box for pod selection
    let pod_dropdown = Block::default()
        .title(Span::styled("Pod:", Style::default().fg(Color::Gray)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    f.render_widget(pod_dropdown, area);
    
    // Show selected pod inside the dropdown
    let pod_text = Paragraph::new(format!(" {} ", pod_name))
        .style(Style::default().fg(Color::Green))
        .alignment(ratatui::layout::Alignment::Left);
    
    // Get inner area of dropdown to render text
    let inner_area = area.inner(&ratatui::layout::Margin { 
        vertical: 0, 
        horizontal: 1 
    });
    
    f.render_widget(pod_text, inner_area);
    
    // Add dropdown indicators
    let dropdown_indicator = if !state.pods.is_empty() {
        Paragraph::new("▼")
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Right)
    } else {
        Paragraph::new(" ")
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Right)
    };
    
    f.render_widget(dropdown_indicator, inner_area);
    
    // Instructions text
    let instructions = Paragraph::new("[↑↓ to change]")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Right);
    
    let instruction_area = Rect::new(
        area.x + area.width - 15,
        area.y + area.height,
        15,
        1
    );
    
    f.render_widget(instructions, instruction_area);
}

fn render_container_dropdown(
    f: &mut Frame,
    state: &MonitorState,
    area: Rect,
) {
    let container_name = state.get_selected_container_name().unwrap_or_else(|| "No containers".to_string());
    
    // Create dropdown-like box for container selection
    let container_dropdown = Block::default()
        .title(Span::styled("Container:", Style::default().fg(Color::Gray)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    
    f.render_widget(container_dropdown, area);
    
    // Show selected container inside the dropdown
    let container_text = Paragraph::new(format!(" {} ", container_name))
        .style(Style::default().fg(Color::Yellow))
        .alignment(ratatui::layout::Alignment::Left);
    
    // Get inner area of dropdown to render text
    let inner_area = area.inner(&ratatui::layout::Margin { 
        vertical: 0, 
        horizontal: 1 
    });
    
    f.render_widget(container_text, inner_area);
    
    // Add dropdown indicators
    let has_containers = state.get_selected_pod()
        .map_or(false, |pod| !pod.containers.is_empty());
    
    let dropdown_indicator = if has_containers {
        Paragraph::new("▼")
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Right)
    } else {
        Paragraph::new(" ")
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Right)
    };
    
    f.render_widget(dropdown_indicator, inner_area);
    
    // Instructions text
    let instructions = Paragraph::new("[←→ to change]")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Right);
    
    let instruction_area = Rect::new(
        area.x + area.width - 15,
        area.y + area.height,
        15,
        1
    );
    
    f.render_widget(instructions, instruction_area);
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