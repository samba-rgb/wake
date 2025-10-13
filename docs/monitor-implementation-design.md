# Wake Pod Monitor Feature: Low-Level Design Document

This document outlines the detailed design for implementing the Pod Monitoring feature in Wake, with a focus on following the Single Responsibility Principle and integrating properly with the existing codebase.

## 1. Architecture Overview

The Pod Monitor feature will be implemented with a clean separation of concerns across the following components:

1. **CLI Integration**: Add monitor flag to the existing CLI arguments
2. **Metrics API Client**: Extend the K8s module to interface with the Kubernetes Metrics API
3. **Metrics Collection**: Background service to collect and store metrics over time
4. **Monitor UI**: Terminal UI for visualizing and interacting with metrics data
5. **Metrics Data Structures**: Efficient storage of time-series metrics data

## 2. Component Design

### 2.1 CLI Integration (`src/cli/args.rs`)

Extend the existing `Args` struct with a monitor mode flag:

```rust
pub struct Args {
    // ...existing fields...
    
    /// Enable monitor mode to display resource usage metrics for pods and containers
    #[arg(short = 'm', long = "monitor", help = "Enable resource monitoring mode")]
    pub monitor: bool,
}
```

### 2.2 Metrics API Client (`src/k8s/metrics.rs`)

Create a new module in the `k8s` directory to interact with the Kubernetes Metrics API:

```rust
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Pod as K8sPod;
use kube::{
    api::{Api, ListParams},
    Client,
};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::k8s::pod::{Container, Pod};

pub struct MetricsClient {
    client: Client,
    namespace: String,
}

impl MetricsClient {
    pub async fn new(client: Client, namespace: String) -> Self {
        Self { client, namespace }
    }

    /// Check if the metrics API is available
    pub async fn check_metrics_api_available(&self) -> bool {
        // Try to call the metrics API and check if it's available
        match self.client
            .request::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(
                kube::core::Request::new("/apis/metrics.k8s.io/v1beta1/pods")
                    .get()
            )
            .await
        {
            Ok(_) => true,
            Err(e) => {
                warn!("Metrics API not available: {}", e);
                false
            }
        }
    }

    /// Fetch metrics for pods matching the selector
    pub async fn get_pod_metrics(
        &self,
        pod_selector: &str,
        pods: &[Pod],
    ) -> Result<HashMap<String, PodMetrics>> {
        // Call the Metrics API to get pod metrics
        let pod_metrics_api: Api<k8s_metrics::PodMetrics> = 
            Api::namespaced(self.client.clone(), &self.namespace);
        
        let list_params = ListParams::default()
            .labels(pod_selector);

        match pod_metrics_api.list(&list_params).await {
            Ok(metrics_list) => {
                let mut result = HashMap::new();
                
                // Process each pod's metrics
                for pod_metrics in metrics_list.items {
                    let pod_name = pod_metrics.metadata.name.clone().unwrap_or_default();
                    
                    // Find the corresponding pod in our list to get resource requests/limits
                    let pod = pods.iter().find(|p| p.name == pod_name);
                    
                    // Create metrics structure
                    let metrics = PodMetrics {
                        timestamp: DateTime::parse_from_rfc3339(&pod_metrics.timestamp)
                            .unwrap_or_else(|_| Utc::now().into())
                            .into(),
                        window: pod_metrics.window
                            .map(|w| parse_duration(&w).unwrap_or(Duration::from_secs(60))),
                        cpu: self.extract_resource_metrics("cpu", &pod_metrics.containers, pod),
                        memory: self.extract_resource_metrics("memory", &pod_metrics.containers, pod),
                    };
                    
                    result.insert(pod_name, metrics);
                }
                
                Ok(result)
            },
            Err(e) => {
                error!("Failed to get pod metrics: {}", e);
                Err(anyhow!("Failed to get pod metrics: {}", e))
            }
        }
    }

    /// Extract resource metrics for a specific resource type (cpu, memory)
    fn extract_resource_metrics(
        &self,
        resource_type: &str,
        containers: &[k8s_metrics::ContainerMetrics],
        pod: Option<&Pod>,
    ) -> ResourceMetrics {
        // Implementation to extract and combine metrics for all containers
        // and calculate utilization against pod requests/limits
        // ...
        unimplemented!()
    }
}

/// Holds metrics information for a pod
pub struct PodMetrics {
    pub timestamp: DateTime<Utc>,
    pub window: Option<Duration>,
    pub cpu: ResourceMetrics,
    pub memory: ResourceMetrics,
}

/// Holds metrics information for a specific resource type
pub struct ResourceMetrics {
    pub usage: String,           // Current usage (e.g., "120m" for CPU, "256Mi" for memory)
    pub usage_value: f64,        // Numeric value for comparisons
    pub request: Option<String>, // Resource request if specified
    pub limit: Option<String>,   // Resource limit if specified
    pub utilization: f64,        // Usage as percentage of request or limit
}

/// Helper function to parse Kubernetes duration strings
fn parse_duration(duration_str: &str) -> Result<Duration> {
    // Parse Kubernetes duration format like "60s"
    // Implementation here
    // ...
    unimplemented!()
}
```

### 2.3 Metrics Collection (`src/metrics/mod.rs`, `src/metrics/collector.rs`)

Create a new metrics module to manage metrics collection and time-series data:

```rust
pub mod collector;
pub mod timeseries;

// src/metrics/timeseries.rs
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};

pub struct TimeSeriesPoint<T> {
    pub timestamp: DateTime<Utc>,
    pub value: T,
}

pub struct TimeSeries<T> {
    pub name: String,
    pub data: VecDeque<TimeSeriesPoint<T>>,
    pub max_points: usize,
    pub metadata: HashMap<String, String>,
}

impl<T: Clone> TimeSeries<T> {
    pub fn new(name: String, max_points: usize) -> Self {
        Self {
            name,
            data: VecDeque::with_capacity(max_points),
            max_points,
            metadata: HashMap::new(),
        }
    }
    
    pub fn add_point(&mut self, timestamp: DateTime<Utc>, value: T) {
        if self.data.len() >= self.max_points {
            self.data.pop_front();
        }
        self.data.push_back(TimeSeriesPoint { timestamp, value });
    }
    
    pub fn get_latest(&self) -> Option<&TimeSeriesPoint<T>> {
        self.data.back()
    }
    
    pub fn get_range(&self, duration: std::time::Duration) -> Vec<&TimeSeriesPoint<T>> {
        let cutoff = Utc::now() - chrono::Duration::from_std(duration).unwrap_or_default();
        self.data.iter()
            .filter(|point| point.timestamp >= cutoff)
            .collect()
    }
}

// src/metrics/collector.rs
use anyhow::Result;
use crate::k8s::metrics::{MetricsClient, PodMetrics, ResourceMetrics};
use crate::k8s::pod::Pod;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub struct MetricsCollector {
    metrics_client: MetricsClient,
    pods: Arc<Mutex<Vec<Pod>>>,
    pod_metrics: Arc<Mutex<HashMap<String, PodMetricsTimeSeries>>>,
    refresh_interval: Duration,
}

pub struct PodMetricsTimeSeries {
    pub cpu: TimeSeries<ResourceMetrics>,
    pub memory: TimeSeries<ResourceMetrics>,
}

impl MetricsCollector {
    pub fn new(
        metrics_client: MetricsClient,
        refresh_interval: Duration,
    ) -> Self {
        Self {
            metrics_client,
            pods: Arc::new(Mutex::new(Vec::new())),
            pod_metrics: Arc::new(Mutex::new(HashMap::new())),
            refresh_interval,
        }
    }
    
    pub fn set_pods(&self, pods: Vec<Pod>) {
        let mut pods_guard = self.pods.lock().unwrap();
        *pods_guard = pods;
    }
    
    pub async fn start_collection(
        &self,
        namespace: String,
        pod_selector: String,
        container_selector: String,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        // Clone necessary values for the async task
        let metrics_client = self.metrics_client.clone();
        let pods = self.pods.clone();
        let pod_metrics = self.pod_metrics.clone();
        let refresh_interval = self.refresh_interval;
        
        // Spawn background collection task
        tokio::spawn(async move {
            let mut interval = time::interval(refresh_interval);
            
            loop {
                interval.tick().await;
                
                if cancellation_token.is_cancelled() {
                    break;
                }
                
                // Get current pods
                let current_pods = {
                    let pods_guard = pods.lock().unwrap();
                    pods_guard.clone()
                };
                
                // Collect metrics for current pods
                match metrics_client.get_pod_metrics(&pod_selector, &current_pods).await {
                    Ok(new_metrics) => {
                        // Update metrics time series
                        let mut metrics_guard = pod_metrics.lock().unwrap();
                        
                        for (pod_name, pod_metrics) in new_metrics {
                            // Get or create time series for this pod
                            let time_series = metrics_guard
                                .entry(pod_name.clone())
                                .or_insert_with(|| PodMetricsTimeSeries {
                                    cpu: TimeSeries::new(format!("{}-cpu", pod_name), 300),
                                    memory: TimeSeries::new(format!("{}-memory", pod_name), 300),
                                });
                            
                            // Add new data points
                            time_series.cpu.add_point(pod_metrics.timestamp, pod_metrics.cpu);
                            time_series.memory.add_point(pod_metrics.timestamp, pod_metrics.memory);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to collect metrics: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    pub fn get_latest_metrics(&self, pod_name: &str) -> Option<(DateTime<Utc>, ResourceMetrics, ResourceMetrics)> {
        let metrics_guard = self.pod_metrics.lock().unwrap();
        
        if let Some(time_series) = metrics_guard.get(pod_name) {
            let cpu = time_series.cpu.get_latest().map(|p| p.value.clone());
            let memory = time_series.memory.get_latest().map(|p| p.value.clone());
            let timestamp = time_series.cpu.get_latest().map(|p| p.timestamp);
            
            if let (Some(cpu), Some(memory), Some(timestamp)) = (cpu, memory, timestamp) {
                return Some((timestamp, cpu, memory));
            }
        }
        
        None
    }
    
    pub fn get_metrics_for_timerange(
        &self,
        pod_name: &str,
        duration: Duration,
    ) -> Option<(Vec<(DateTime<Utc>, ResourceMetrics)>, Vec<(DateTime<Utc>, ResourceMetrics)>)> {
        let metrics_guard = self.pod_metrics.lock().unwrap();
        
        if let Some(time_series) = metrics_guard.get(pod_name) {
            let cpu_metrics: Vec<(DateTime<Utc>, ResourceMetrics)> = time_series.cpu
                .get_range(duration)
                .into_iter()
                .map(|p| (p.timestamp, p.value.clone()))
                .collect();
                
            let memory_metrics: Vec<(DateTime<Utc>, ResourceMetrics)> = time_series.memory
                .get_range(duration)
                .into_iter()
                .map(|p| (p.timestamp, p.value.clone()))
                .collect();
                
            return Some((cpu_metrics, memory_metrics));
        }
        
        None
    }
    
    pub fn get_all_pod_names(&self) -> Vec<String> {
        let metrics_guard = self.pod_metrics.lock().unwrap();
        metrics_guard.keys().cloned().collect()
    }
}
```

### 2.4 Monitor UI Implementation (`src/ui/monitor/mod.rs`)

Create a new submodule under the UI directory for the monitoring interface:

```rust
pub mod app;
pub mod widgets;

// src/ui/monitor/app.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tui_tree_widget::{Tree, TreeItem, TreeState};
use tracing::{debug, error, info, warn};

use crate::cli::Args;
use crate::k8s::client;
use crate::k8s::metrics::{MetricsClient, ResourceMetrics};
use crate::k8s::pod::{Container, Pod};
use crate::metrics::collector::{MetricsCollector, PodMetricsTimeSeries};
use super::widgets::{MetricsChart, PodContainerTree};

pub enum Tab {
    Overview,
    CPU,
    Memory,
    Network,
    Disk,
}

pub enum TimeRange {
    Last5Minutes,
    Last15Minutes,
    LastHour,
    Last3Hours,
}

pub struct MonitorApp {
    metrics_collector: Arc<MetricsCollector>,
    tree_state: Arc<Mutex<TreeState>>,
    selected_pods: HashSet<String>,
    selected_containers: HashMap<String, HashSet<String>>,
    current_tab: Tab,
    time_range: TimeRange,
    auto_refresh: bool,
    namespace: String,
    pod_selector: String,
    container_selector: String,
}

impl MonitorApp {
    pub fn new(
        metrics_collector: Arc<MetricsCollector>,
        namespace: String,
        pod_selector: String,
        container_selector: String,
    ) -> Self {
        Self {
            metrics_collector,
            tree_state: Arc::new(Mutex::new(TreeState::default())),
            selected_pods: HashSet::new(),
            selected_containers: HashMap::new(),
            current_tab: Tab::Overview,
            time_range: TimeRange::Last5Minutes,
            auto_refresh: true,
            namespace,
            pod_selector,
            container_selector,
        }
    }
    
    pub async fn run(self, args: Args) -> Result<()> {
        // Initialize terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        
        // Set up cancellation token for graceful shutdown
        let cancellation_token = CancellationToken::new();
        
        // Start UI loop
        let res = self.ui_loop(&mut terminal, cancellation_token.clone()).await;
        
        // Cancel background tasks
        cancellation_token.cancel();
        
        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        
        res
    }
    
    async fn ui_loop<B: Backend>(&self, terminal: &mut Terminal<B>, cancellation_token: CancellationToken) -> Result<()> {
        let mut last_refresh = Instant::now();
        let refresh_interval = Duration::from_millis(250); // UI refresh rate
        
        loop {
            // Handle input events
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if !self.handle_key_event(key, &cancellation_token).await {
                        break;
                    }
                }
            }
            
            // Refresh UI periodically
            if last_refresh.elapsed() >= refresh_interval {
                terminal.draw(|f| self.render(f))?;
                last_refresh = Instant::now();
            }
        }
        
        Ok(())
    }
    
    async fn handle_key_event(&self, key: KeyEvent, cancellation_token: &CancellationToken) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                return false; // Exit app
            }
            KeyCode::Tab => {
                // Switch tab
            }
            KeyCode::Char('c') => {
                // Toggle comparison mode
            }
            KeyCode::Char('f') => {
                // Toggle auto-refresh
            }
            KeyCode::Char('1') => {
                // Switch to Overview tab
            }
            KeyCode::Char('2') => {
                // Switch to CPU tab
            }
            // Other key handlers
            _ => {}
        }
        
        true // Continue running
    }
    
    fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Main content
                Constraint::Length(3), // Status bar
                Constraint::Length(1), // Help text
            ])
            .split(f.size());
        
        // Render header (pod and container selectors)
        self.render_header(f, chunks[0]);
        
        // Split main content into tree and metrics panels
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // Tree
                Constraint::Percentage(70), // Metrics
            ])
            .split(chunks[1]);
        
        // Render pod/container tree
        self.render_tree(f, main_chunks[0]);
        
        // Render metrics panel based on current tab
        match self.current_tab {
            Tab::Overview => self.render_overview(f, main_chunks[1]),
            Tab::CPU => self.render_cpu_tab(f, main_chunks[1]),
            Tab::Memory => self.render_memory_tab(f, main_chunks[1]),
            Tab::Network => self.render_network_tab(f, main_chunks[1]),
            Tab::Disk => self.render_disk_tab(f, main_chunks[1]),
        }
        
        // Render status bar
        self.render_status_bar(f, chunks[2]);
        
        // Render help text
        self.render_help(f, chunks[3]);
    }
    
    fn render_header(&self, f: &mut Frame, area: Rect) {
        // Implement header rendering with pod/container selectors
    }
    
    fn render_tree(&self, f: &mut Frame, area: Rect) {
        // Implement pod/container tree rendering
    }
    
    fn render_overview(&self, f: &mut Frame, area: Rect) {
        // Implement overview panel rendering
    }
    
    fn render_cpu_tab(&self, f: &mut Frame, area: Rect) {
        // Implement CPU metrics panel rendering
    }
    
    fn render_memory_tab(&self, f: &mut Frame, area: Rect) {
        // Implement memory metrics panel rendering
    }
    
    fn render_network_tab(&self, f: &mut Frame, area: Rect) {
        // Implement network metrics panel rendering
    }
    
    fn render_disk_tab(&self, f: &mut Frame, area: Rect) {
        // Implement disk metrics panel rendering
    }
    
    fn render_status_bar(&self, f: &mut Frame, area: Rect) {
        // Implement status bar rendering
    }
    
    fn render_help(&self, f: &mut Frame, area: Rect) {
        // Implement help text rendering
    }
}

// src/ui/monitor/widgets.rs
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Canvas, Chart, Dataset, GraphType, Widget},
    Frame,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::k8s::pod::{Container, Pod};
use crate::k8s::metrics::ResourceMetrics;
use crate::metrics::timeseries::TimeSeries;

pub struct MetricsChart<'a> {
    title: &'a str,
    max_value: f64,
    datasets: Vec<(String, Vec<(f64, f64)>, Color)>,
    x_axis: (f64, f64),
    y_axis: (f64, f64),
}

impl<'a> MetricsChart<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            max_value: 100.0,
            datasets: Vec::new(),
            x_axis: (0.0, 100.0),
            y_axis: (0.0, 100.0),
        }
    }
    
    pub fn add_dataset(&mut self, name: String, data: Vec<(f64, f64)>, color: Color) {
        self.datasets.push((name, data, color));
    }
    
    pub fn set_x_axis(&mut self, min: f64, max: f64) {
        self.x_axis = (min, max);
    }
    
    pub fn set_y_axis(&mut self, min: f64, max: f64) {
        self.y_axis = (min, max);
        self.max_value = max;
    }
    
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let datasets: Vec<_> = self.datasets
            .iter()
            .map(|(name, data, color)| {
                Dataset::default()
                    .name(name)
                    .marker(symbols::Marker::Braille)
                    .style(Style::default().fg(*color))
                    .graph_type(GraphType::Line)
                    .data(data)
            })
            .collect();
        
        let chart = Chart::new(datasets)
            .block(Block::default()
                .title(self.title)
                .borders(Borders::ALL))
            .x_axis(ratatui::widgets::Axis::default()
                .bounds([self.x_axis.0, self.x_axis.1])
                .labels(vec![
                    Span::raw(format!("{:.1}", self.x_axis.0)),
                    Span::raw(format!("{:.1}", (self.x_axis.0 + self.x_axis.1) / 2.0)),
                    Span::raw(format!("{:.1}", self.x_axis.1))
                ]))
            .y_axis(ratatui::widgets::Axis::default()
                .bounds([self.y_axis.0, self.y_axis.1])
                .labels(vec![
                    Span::raw(format!("{:.1}", self.y_axis.0)),
                    Span::raw(format!("{:.1}", self.y_axis.1 / 2.0)),
                    Span::raw(format!("{:.1}", self.y_axis.1))
                ]));
        
        f.render_widget(chart, area);
    }
}

pub struct PodContainerTree<'a> {
    pods: &'a [Pod],
    selected_pods: &'a [String],
    selected_containers: &'a HashMap<String, HashSet<String>>,
    state: &'a TreeState,
}

impl<'a> PodContainerTree<'a> {
    pub fn new(
        pods: &'a [Pod],
        selected_pods: &'a [String],
        selected_containers: &'a HashMap<String, HashSet<String>>,
        state: &'a TreeState,
    ) -> Self {
        Self {
            pods,
            selected_pods,
            selected_containers,
            state,
        }
    }
    
    pub fn render(&self, f: &mut Frame, area: Rect) {
        // Create tree items from pods and containers
        let items: Vec<TreeItem> = self.pods
            .iter()
            .map(|pod| {
                let is_selected = self.selected_pods.contains(&pod.name);
                
                let mut pod_item = TreeItem::new(
                    pod.name.clone(),
                    pod.containers
                        .iter()
                        .map(|c| {
                            let container_selected = self.selected_containers
                                .get(&pod.name)
                                .map(|set| set.contains(&c.name))
                                .unwrap_or(false);
                                
                            TreeItem::new_leaf(c.name.clone(), container_selected)
                        })
                        .collect(),
                    is_selected,
                );
                
                pod_item
            })
            .collect();
        
        let tree = Tree::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Pods and Containers"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        
        f.render_stateful_widget(tree, area, &mut self.state.clone());
    }
}
```

### 2.5 Integration with Main (`src/main.rs`)

Update the main entry point to handle the monitor flag:

```rust
async fn main() -> Result<()> {
    let args = cli::args::parse_args();
    
    // Handle monitor mode
    if args.monitor {
        info!("Starting Wake in monitor mode");
        
        // Create K8s client
        let client = k8s::client::create_client(&args).await?;
        
        // Create metrics client
        let metrics_client = k8s::metrics::MetricsClient::new(
            client.clone(),
            args.namespace.clone(),
        ).await;
        
        // Check if metrics API is available
        if !metrics_client.check_metrics_api_available().await {
            eprintln!("Error: Kubernetes metrics API is not available in this cluster");
            eprintln!("Please ensure the metrics-server is installed and running");
            return Err(anyhow::anyhow!("Metrics API not available"));
        }
        
        // Create metrics collector
        let metrics_collector = Arc::new(MetricsCollector::new(
            metrics_client,
            Duration::from_secs(5), // Refresh interval
        ));
        
        // Start the monitor app
        let app = ui::monitor::app::MonitorApp::new(
            metrics_collector,
            args.namespace.clone(),
            args.pod_selector.clone(),
            args.container.clone(),
        );
        
        return app.run(args).await;
    }
    
    // Continue with existing log streaming functionality
    // ...
}
```

## 3. Data Flow

### 3.1 Startup Flow

```
1. User runs Wake with -m flag
2. CLI parser identifies monitor mode
3. main.rs creates K8s client
4. main.rs initializes MetricsClient with K8s client
5. main.rs checks if metrics API is available
6. main.rs creates MetricsCollector
7. main.rs starts MonitorApp
8. MonitorApp initializes UI and starts background collection
9. MonitorApp enters UI event loop
```

### 3.2 Metrics Collection Flow

```
1. MetricsCollector periodically polls metrics API
2. New metrics are fetched for matching pods/containers
3. Metrics are processed and normalized
4. TimeSeries data structures are updated with new metrics
5. Old metrics beyond retention period are pruned
```

### 3.3 UI Update Flow

```
1. UI refreshes periodically (250ms default)
2. Latest metrics data is retrieved from collector
3. Tree widget shows pods and containers
4. Chart widgets visualize time series data
5. Status bar shows summary statistics
```

### 3.4 User Interaction Flow

```
1. User presses key or clicks mouse
2. Event is processed by MonitorApp
3. State is updated based on action
   - Pod/container selection
   - Tab switching
   - Time range adjustment
   - Auto-refresh toggling
4. UI is re-rendered with updated state
```

## 4. Error Handling

### 4.1 API Unavailability

- Initial check ensures metrics API is available
- Background collection has resilient error handling
- UI degrades gracefully when metrics are missing
- Clear status messages inform user of API issues

### 4.2 Permission Issues

- Explicit error messages for permission problems
- Guidance for required RBAC permissions
- Fallback to available metrics when some are inaccessible

### 4.3 Data Presentation

- Sensible handling of missing or incomplete metrics
- Automatic scaling of charts based on available data
- Visual indicators for data quality issues

## 5. Implementation Phases

### 5.1 Phase 1: CLI and Core Infrastructure

- Add monitor flag to CLI arguments
- Create metrics API client
- Implement metrics data structures
- Set up basic time series storage

### 5.2 Phase 2: Basic Metrics Collection

- Implement pod metrics collection
- Add container metrics collection
- Create background collection service
- Implement time series management

### 5.3 Phase 3: Basic UI

- Create monitor app structure
- Implement pod/container tree
- Add simple metrics visualizations
- Implement basic navigation

### 5.4 Phase 4: Enhanced Features

- Add multi-pod/container comparison
- Implement time range selection
- Add detailed metrics tabs
- Enhance visualization quality

### 5.5 Phase 5: Polish and Optimization

- Optimize metrics collection for performance
- Enhance error handling and resilience
- Add help documentation
- Final UI polish and keyboard shortcuts

## 6. Testing Strategy

### 6.1 Unit Tests

- Test metrics data parsing and normalization
- Test time series data management
- Test UI component rendering

### 6.2 Integration Tests

- Test metrics API client with mock server
- Test end-to-end metrics collection flow
- Test UI with simulated metrics data

### 6.3 Manual Testing

- Test with various pod/container combinations
- Test with different terminal sizes
- Test error conditions and recovery