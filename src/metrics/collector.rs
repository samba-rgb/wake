use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use crate::k8s::pod::Pod;
use crate::metrics::timeseries::TimeSeries;

/// Structure to hold metrics data for a pod/container
#[derive(Debug, Clone)]
pub struct MetricsData {
    pub timestamp: DateTime<Utc>,
    pub pod_name: String,
    pub container_name: String,
    pub cpu_usage: f64,    // CPU usage in percentage (0-100)
    pub memory_usage: f64, // Memory usage in MB
}

/// Container metrics with time series data
#[derive(Debug, Clone)]
pub struct ContainerMetrics {
    pub cpu_series: TimeSeries<f64>,
    pub memory_series: TimeSeries<f64>,
}

impl ContainerMetrics {
    /// Create a new container metrics with default buffer size of 1000
    pub fn new(container_name: &str) -> Self {
        Self {
            cpu_series: TimeSeries::new(&format!("{}-cpu", container_name), 1000),
            memory_series: TimeSeries::new(&format!("{}-memory", container_name), 1000),
        }
    }
    
    /// Add a data point to the container metrics
    pub fn add_point(&mut self, timestamp: DateTime<Utc>, cpu: f64, memory: f64) {
        self.cpu_series.add_point(timestamp, cpu);
        self.memory_series.add_point(timestamp, memory);
    }
}

/// Metrics collection maps as per your design
#[derive(Debug, Clone, Default)]
pub struct MetricsMaps {
    // pod_name -> container_name -> metrics
    pub cpu_map: HashMap<String, HashMap<String, TimeSeries<f64>>>,
    pub memory_map: HashMap<String, HashMap<String, TimeSeries<f64>>>,
    pub last_update: DateTime<Utc>,
}

/// Metrics summary for the UI
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub total_pods: usize,
    pub monitored_pods: usize,
    pub total_containers: usize,
    pub cpu_usage_total: f64,
    pub memory_usage_total: f64,
    pub timestamp: DateTime<Utc>,
}

impl Default for MetricsSummary {
    fn default() -> Self {
        Self {
            total_pods: 0,
            monitored_pods: 0,
            total_containers: 0,
            cpu_usage_total: 0.0,
            memory_usage_total: 0.0,
            timestamp: Utc::now(),
        }
    }
}

/// Simple time series for metrics data
#[derive(Debug, Clone)]
pub struct MetricsTimeSeries {
    pub data: Vec<(DateTime<Utc>, f64, f64)>, // timestamp, CPU, memory
}

/// Client for collecting metrics from Kubernetes
pub struct MetricsCollector {
    metrics: Arc<Mutex<HashMap<String, MetricsTimeSeries>>>,
    pods: Arc<Mutex<Vec<Pod>>>,
    collection_active: Arc<Mutex<bool>>,
    using_metrics_api: Arc<Mutex<bool>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(HashMap::new())),
            pods: Arc::new(Mutex::new(Vec::new())),
            collection_active: Arc::new(Mutex::new(false)),
            using_metrics_api: Arc::new(Mutex::new(true)),
        }
    }
    
    /// Set the pods to monitor
    pub fn set_pods(&self, pods: Vec<Pod>) {
        let mut pods_guard = self.pods.lock().unwrap();
        *pods_guard = pods;
    }
    
    /// Get the latest metrics for a pod
    pub fn get_latest_metrics(&self, pod_name: &str) -> Option<(DateTime<Utc>, f64, f64)> {
        let metrics_guard = self.metrics.lock().unwrap();
        if let Some(time_series) = metrics_guard.get(pod_name) {
            time_series.data.last().cloned()
        } else {
            None
        }
    }
    
    /// Get a summary of all metrics
    pub fn get_metrics_summary(&self) -> MetricsSummary {
        let metrics_guard = self.metrics.lock().unwrap();
        let pods_guard = self.pods.lock().unwrap();
        
        let mut summary = MetricsSummary {
            total_pods: pods_guard.len(),
            monitored_pods: 0,
            total_containers: 0,
            cpu_usage_total: 0.0,
            memory_usage_total: 0.0,
            timestamp: Utc::now(),
        };
        
        // Count containers in pods
        for pod in pods_guard.iter() {
            summary.total_containers += pod.containers.len();
        }
        
        // Calculate total resource usage from metrics
        for (_pod_name, time_series) in &*metrics_guard {
            if let Some((timestamp, cpu, memory)) = time_series.data.last() {
                summary.monitored_pods += 1;
                summary.cpu_usage_total += cpu;
                summary.memory_usage_total += memory;
                summary.timestamp = *timestamp;
            }
        }
        
        summary
    }
    
    /// Start collecting metrics with cancellation support
    pub async fn start_collection(
        &self,
        _namespace: String,
        _pod_selector: String,
        _container_selector: String,
        cancellation_token: CancellationToken,
    ) -> Result<mpsc::Receiver<MetricsData>> {
        let (tx, rx) = mpsc::channel::<MetricsData>(100);
        
        // Set collection_active to true
        {
            let mut active_guard = self.collection_active.lock().unwrap();
            *active_guard = true;
        }
        
        // Clone the necessary Arc pointers for the collection task
        let metrics = self.metrics.clone();
        let pods = self.pods.clone();
        let collection_active = self.collection_active.clone();
        let using_metrics_api = self.using_metrics_api.clone();
        
        // Start the metrics collection loop
        tokio::spawn(async move {
            // Check if Kubernetes metrics API is available
            let mut use_metrics_api = Self::is_metrics_api_available().await;
            
            // Store the API availability status
            {
                let mut api_guard = using_metrics_api.lock().unwrap();
                *api_guard = use_metrics_api;
            }
            
            if !use_metrics_api {
                info!("Kubernetes Metrics API not available, falling back to kubectl top");
            }
            
            // Collection loop
            let interval = Duration::from_secs(5); // Collect every 5 seconds
            let mut last_collection = tokio::time::Instant::now();
            
            while !cancellation_token.is_cancelled() {
                // Only collect if interval has elapsed
                if last_collection.elapsed() >= interval {
                    // Check if collection is still active
                    {
                        let active_guard = collection_active.lock().unwrap();
                        if !*active_guard {
                            break;
                        }
                    }
                    
                    // Get pod list
                    let pod_list = {
                        let pods_guard = pods.lock().unwrap();
                        pods_guard.clone()
                    };
                    
                    // Collect metrics
                    let collection_result = if use_metrics_api {
                        Self::collect_metrics_from_api(&pod_list).await
                    } else {
                        Self::collect_metrics_from_kubectl(&pod_list).await
                    };
                    
                    match collection_result {
                        Ok(new_metrics) => {
                            // Update the metrics map
                            let mut metrics_guard = metrics.lock().unwrap();
                            let timestamp = Utc::now();
                            
                            for (pod_name, (cpu, memory)) in new_metrics {
                                let entry = metrics_guard.entry(pod_name.clone()).or_insert_with(|| {
                                    MetricsTimeSeries {
                                        data: Vec::new(),
                                    }
                                });
                                
                                // Add the new data point
                                entry.data.push((timestamp, cpu, memory));
                                
                                // Limit the history to 100 points
                                if entry.data.len() > 100 {
                                    entry.data.remove(0);
                                }
                                
                                // Send the metrics data to the channel
                                let data = MetricsData {
                                    timestamp,
                                    cpu_usage: cpu,
                                    memory_usage: memory,
                                };
                                
                                if tx.try_send(data).is_err() {
                                    // Channel is full or closed, ignore the error
                                    debug!("Failed to send metrics data to channel");
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to collect metrics: {}", e);
                            
                            // If using metrics API fails, try switching to kubectl
                            if use_metrics_api {
                                warn!("Switching to kubectl top for metrics collection");
                                use_metrics_api = false;
                                
                                // Store the API availability status
                                {
                                    let mut api_guard = using_metrics_api.lock().unwrap();
                                    *api_guard = false;
                                }
                            }
                        }
                    }
                    
                    // Update last collection time
                    last_collection = tokio::time::Instant::now();
                }
                
                // Sleep to prevent CPU usage
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            
            // When the loop exits, set collection_active to false
            {
                let mut active_guard = collection_active.lock().unwrap();
                *active_guard = false;
            }
        });
        
        Ok(rx)
    }
    
    /// Check if the Kubernetes metrics API is available
    async fn is_metrics_api_available() -> bool {
        // Try to run 'kubectl api-resources | grep metrics.k8s.io' to check for metrics API
        let output = Command::new("kubectl")
            .arg("api-resources")
            .output();
        
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("metrics.k8s.io")
            }
            Err(_) => false,
        }
    }
    
    /// Collect metrics using the Kubernetes metrics API
    async fn collect_metrics_from_api(pods: &[Pod]) -> Result<HashMap<String, (f64, f64)>> {
        // This is just a stub implementation.
        // In a real implementation, this would use the metrics.k8s.io API to get metrics.
        let mut results = HashMap::new();
        
        // For demo purposes, generate simulated metrics
        for pod in pods {
            // Generate a deterministic but variable random number based on pod name
            let pod_name_hash = pod.name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let random_factor = (pod_name_hash as f64 % 100.0) / 100.0;
            
            // Simulated CPU (0.05-0.5 cores) and memory (50-500MB) usage with some randomness
            let cpu = 0.05 + (random_factor * 0.45);
            let memory = (50.0 + (random_factor * 450.0)) * 1024.0 * 1024.0; // Convert MB to bytes
            
            results.insert(pod.name.clone(), (cpu, memory));
        }
        
        Ok(results)
    }
    
    /// Collect metrics using kubectl top command
    async fn collect_metrics_from_kubectl(pods: &[Pod]) -> Result<HashMap<String, (f64, f64)>> {
        let mut results = HashMap::new();
        
        // Get the list of pod names
        let pod_names: Vec<String> = pods.iter()
            .map(|p| p.name.clone())
            .collect();
        
        if pod_names.is_empty() {
            return Err(anyhow!("No pods to monitor"));
        }
        
        // Use kubectl top pod to get metrics for all pods
        let output = Command::new("kubectl")
            .arg("top")
            .arg("pod")
            .arg("--no-headers")
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                
                // Parse the output and extract metrics
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let pod_name = parts[0];
                        
                        // Only process metrics for pods we're monitoring
                        if pod_names.contains(&pod_name.to_string()) {
                            // Parse CPU usage (format: 123m or 0.123)
                            let cpu_str = parts[1];
                            let cpu = if cpu_str.ends_with('m') {
                                // Convert millicores to cores
                                let millicore_str = &cpu_str[0..cpu_str.len() - 1];
                                match millicore_str.parse::<f64>() {
                                    Ok(mc) => mc / 1000.0,
                                    Err(_) => 0.0,
                                }
                            } else {
                                // Already in cores
                                match cpu_str.parse::<f64>() {
                                    Ok(c) => c,
                                    Err(_) => 0.0,
                                }
                            };
                            
                            // Parse memory usage (format: 123Mi, 456Ki, etc.)
                            let mem_str = parts[2];
                            let mem = parse_k8s_memory(mem_str);
                            
                            results.insert(pod_name.to_string(), (cpu, mem));
                        }
                    }
                }
                
                Ok(results)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!("kubectl command failed: {}", stderr))
            }
            Err(e) => {
                Err(anyhow!("Failed to execute kubectl: {}", e))
            }
        }
    }
}

/// Parse Kubernetes memory string (e.g., 123Mi, 456Ki) to bytes
fn parse_k8s_memory(mem_str: &str) -> f64 {
    let mut num_str = String::new();
    let mut unit_str = String::new();
    
    for c in mem_str.chars() {
        if c.is_digit(10) || c == '.' {
            num_str.push(c);
        } else {
            unit_str.push(c);
        }
    }
    
    let value = match num_str.parse::<f64>() {
        Ok(v) => v,
        Err(_) => return 0.0,
    };
    
    // Convert to bytes based on unit
    match unit_str.as_str() {
        "Ki" => value * 1024.0,
        "Mi" => value * 1024.0 * 1024.0,
        "Gi" => value * 1024.0 * 1024.0 * 1024.0,
        "Ti" => value * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "K" | "k" => value * 1000.0,
        "M" => value * 1000.0 * 1000.0,
        "G" => value * 1000.0 * 1000.0 * 1000.0,
        "T" => value * 1000.0 * 1000.0 * 1000.0 * 1000.0,
        _ => value, // Assume bytes if no unit
    }
}

/// Modern metrics collector that tracks container-level metrics
pub struct ModernMetricsCollector {
    metrics_maps: Arc<Mutex<MetricsMaps>>,
    pods: Arc<Mutex<Vec<Pod>>>,
    collection_active: Arc<Mutex<bool>>,
    client: Option<Arc<kube::Client>>,
    using_metrics_api: Arc<Mutex<bool>>,
    metrics_api_failures: Arc<Mutex<u32>>,
    api_availability_check_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    reconnect_backoff_ms: Arc<Mutex<u64>>,
}

impl ModernMetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics_maps: Arc::new(Mutex::new(MetricsMaps::default())),
            pods: Arc::new(Mutex::new(Vec::new())),
            collection_active: Arc::new(Mutex::new(false)),
            client: None,
            using_metrics_api: Arc::new(Mutex::new(true)),
            metrics_api_failures: Arc::new(Mutex::new(0)),
            api_availability_check_time: Arc::new(Mutex::new(None)),
            reconnect_backoff_ms: Arc::new(Mutex::new(500)), // Start with 500ms
        }
    }
    
    /// Initialize with a Kubernetes client
    pub fn with_client(client: kube::Client) -> Self {
        Self {
            metrics_maps: Arc::new(Mutex::new(MetricsMaps::default())),
            pods: Arc::new(Mutex::new(Vec::new())),
            collection_active: Arc::new(Mutex::new(false)),
            client: Some(Arc::new(client)),
            using_metrics_api: Arc::new(Mutex::new(true)),
            metrics_api_failures: Arc::new(Mutex::new(0)),
            api_availability_check_time: Arc::new(Mutex::new(None)),
            reconnect_backoff_ms: Arc::new(Mutex::new(500)), // Start with 500ms
        }
    }
    
    /// Set the pods to monitor
    pub fn set_pods(&self, pods: Vec<Pod>) {
        let mut pods_guard = self.pods.lock().unwrap();
        *pods_guard = pods;
    }
    
    /// Get the current metrics maps
    pub fn get_metrics_maps(&self) -> MetricsMaps {
        let metrics_guard = self.metrics_maps.lock().unwrap();
        metrics_guard.clone()
    }
    
    /// Check if metrics API should be used or if we need to verify its availability
    fn should_use_metrics_api(&self) -> bool {
        let api_guard = self.using_metrics_api.lock().unwrap();
        let failures_guard = self.metrics_api_failures.lock().unwrap();
        let last_check_guard = self.api_availability_check_time.lock().unwrap();
        
        // If we've already decided not to use the API, return false
        if !*api_guard {
            return false;
        }
        
        // If we've had too many failures, we may need to re-check availability
        if *failures_guard >= 3 {
            // Check if we've recently verified availability
            if let Some(last_check) = *last_check_guard {
                // Only re-check every 30 seconds
                if (Utc::now() - last_check).num_seconds() < 30 {
                    return false;
                }
            }
        }
        
        true
    }
    
    /// Reset the metrics API failure counter and update last check time
    fn reset_metrics_api_status(&self, available: bool) {
        let mut api_guard = self.using_metrics_api.lock().unwrap();
        let mut failures_guard = self.metrics_api_failures.lock().unwrap();
        let mut last_check_guard = self.api_availability_check_time.lock().unwrap();
        let mut backoff_guard = self.reconnect_backoff_ms.lock().unwrap();
        
        *api_guard = available;
        *failures_guard = 0;
        *last_check_guard = Some(Utc::now());
        
        if available {
            // Reset backoff on successful connection
            *backoff_guard = 500;
        }
    }
    
    /// Start collecting container-level metrics with cancellation support
    pub async fn start_collection(
        &self,
        namespace: String,
        pod_selector: String,
        container_selector: String,
        interval_secs: u64,
        cancellation_token: CancellationToken,
    ) -> Result<mpsc::Receiver<MetricsData>> {
        let (tx, rx) = mpsc::channel::<MetricsData>(100);
        
        // Set collection_active to true
        {
            let mut active_guard = self.collection_active.lock().unwrap();
            *active_guard = true;
        }
        
        // Clone the necessary Arc pointers for the collection task
        let metrics_maps = self.metrics_maps.clone();
        let pods = self.pods.clone();
        let collection_active = self.collection_active.clone();
        let using_metrics_api = self.using_metrics_api.clone();
        let metrics_api_failures = self.metrics_api_failures.clone();
        let api_availability_check_time = self.api_availability_check_time.clone();
        let reconnect_backoff_ms = self.reconnect_backoff_ms.clone();
        let client_option = self.client.clone();
        
        // Start the metrics collection loop
        tokio::spawn(async move {
            info!("Starting container-level metrics collection for namespace={} pods={} containers={}",
                 namespace, pod_selector, container_selector);
            
            // Track metrics client and its status
            let mut metrics_client_option: Option<crate::k8s::metrics::MetricsClient> = None;
            
            // Create metrics client if we have a Kubernetes client
            if let Some(client) = client_option.as_ref() {
                match crate::k8s::metrics::MetricsClient::new((*client).clone(), namespace.clone()).await {
                    Ok(mc) => {
                        info!("Using Kubernetes metrics API for container metrics");
                        metrics_client_option = Some(mc);
                        
                        // Update metrics API status
                        {
                            let mut api_guard = using_metrics_api.lock().unwrap();
                            let mut check_time_guard = api_availability_check_time.lock().unwrap();
                            *api_guard = true;
                            *check_time_guard = Some(Utc::now());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to create metrics client, falling back to kubectl: {}", e);
                        {
                            let mut api_guard = using_metrics_api.lock().unwrap();
                            let mut check_time_guard = api_availability_check_time.lock().unwrap();
                            *api_guard = false;
                            *check_time_guard = Some(Utc::now());
                        }
                    }
                }
            } else {
                {
                    let mut api_guard = using_metrics_api.lock().unwrap();
                    *api_guard = false;
                }
            };
            
            // Collection loop
            let interval = Duration::from_secs(interval_secs.max(1)); // Minimum 1 second
            let mut last_collection = tokio::time::Instant::now();
            
            // Periodically check metrics API availability after failures
            let mut next_api_check_time = tokio::time::Instant::now();
            
            while !cancellation_token.is_cancelled() {
                // Only collect if interval has elapsed
                if last_collection.elapsed() >= interval {
                    // Check if collection is still active
                    {
                        let active_guard = collection_active.lock().unwrap();
                        if !*active_guard {
                            break;
                        }
                    }
                    
                    // Get pod list
                    let pod_list = {
                        let pods_guard = pods.lock().unwrap();
                        pods_guard.clone()
                    };
                    
                    // Timestamp for this collection
                    let timestamp = Utc::now();
                    
                    // Check metrics API availability and recreate client if needed
                    let should_check_api = {
                        let use_api = {
                            let api_guard = using_metrics_api.lock().unwrap();
                            *api_guard
                        };
                        
                        let failures = {
                            let failures_guard = metrics_api_failures.lock().unwrap();
                            *failures_guard
                        };
                        
                        // Check API if we've had failures but aren't using kubectl yet
                        !use_api && metrics_client_option.is_some() && 
                        next_api_check_time.elapsed() >= Duration::from_secs(30) // Check every 30 seconds
                    };
                    
                    // If we need to check API availability and we have a client
                    if should_check_api && client_option.is_some() {
                        debug!("Checking metrics API availability after previous failures");
                        
                        if let Some(client) = client_option.as_ref() {
                            match crate::k8s::metrics::MetricsClient::new((*client).clone(), namespace.clone()).await {
                                Ok(mc) => {
                                    // Test the client with a simple request
                                    match mc.check_metrics_api_available().await {
                                        true => {
                                            info!("Metrics API is available again, switching back from kubectl");
                                            metrics_client_option = Some(mc);
                                            
                                            // Update metrics API status
                                            {
                                                let mut api_guard = using_metrics_api.lock().unwrap();
                                                let mut failures_guard = metrics_api_failures.lock().unwrap();
                                                let mut check_time_guard = api_availability_check_time.lock().unwrap();
                                                *api_guard = true;
                                                *failures_guard = 0;
                                                *check_time_guard = Some(Utc::now());
                                            }
                                        }
                                        false => {
                                            debug!("Metrics API still unavailable after check");
                                            // Update next check time with backoff
                                            let backoff = {
                                                let mut backoff_guard = reconnect_backoff_ms.lock().unwrap();
                                                // Increase backoff up to 5 minutes
                                                *backoff_guard = (*backoff_guard * 2).min(5 * 60 * 1000);
                                                *backoff_guard
                                            };
                                            
                                            next_api_check_time = tokio::time::Instant::now() + 
                                                Duration::from_millis(backoff);
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to recreate metrics client: {}", e);
                                    next_api_check_time = tokio::time::Instant::now() + Duration::from_secs(60);
                                }
                            }
                        }
                    }
                    
                    // Check if we should use metrics API
                    let use_api = {
                        let api_guard = using_metrics_api.lock().unwrap();
                        *api_guard && metrics_client_option.is_some()
                    };
                    
                    // Process each pod
                    for pod in pod_list.iter() {
                        // Skip pods without namespace information
                        let pod_namespace = match &pod.namespace {
                            Some(ns) => ns,
                            None => &namespace // Default to collection namespace
                        };
                        
                        // Collect container metrics
                        let result = if use_api {
                            if let Some(ref mc) = metrics_client_option {
                                Self::collect_container_metrics_from_api(mc, pod_namespace, &pod.name).await
                            } else {
                                // This should never happen (use_api is true but client is None)
                                Err(anyhow!("Metrics client not available"))
                            }
                        } else {
                            Self::collect_container_metrics_from_kubectl(pod_namespace, &pod.name).await
                        };
                        
                        // Process the collection result
                        match result {
                            Ok(container_metrics) => {
                                // Reset failure counter on success if using API
                                if use_api {
                                    let mut failures = metrics_api_failures.lock().unwrap();
                                    *failures = 0;
                                }
                                
                                // Update metrics maps
                                let mut metrics_guard = metrics_maps.lock().unwrap();
                                
                                for (container_name, (cpu, memory)) in container_metrics {
                                    // Insert into CPU map
                                    let pod_cpu_map = metrics_guard.cpu_map
                                        .entry(pod.name.clone())
                                        .or_insert_with(HashMap::new);
                                    
                                    let container_cpu_series = pod_cpu_map
                                        .entry(container_name.clone())
                                        .or_insert_with(|| TimeSeries::new(&format!("{}-{}-cpu", pod.name, container_name), 1000));
                                    
                                    container_cpu_series.add_point(timestamp, cpu);
                                    
                                    // Insert into Memory map
                                    let pod_mem_map = metrics_guard.memory_map
                                        .entry(pod.name.clone())
                                        .or_insert_with(HashMap::new);
                                    
                                    let container_mem_series = pod_mem_map
                                        .entry(container_name.clone())
                                        .or_insert_with(|| TimeSeries::new(&format!("{}-{}-memory", pod.name, container_name), 1000));
                                    
                                    container_mem_series.add_point(timestamp, memory);
                                    
                                    // Send the metrics data to the channel
                                    let data = MetricsData {
                                        timestamp,
                                        pod_name: pod.name.clone(),
                                        container_name,
                                        cpu_usage: cpu,
                                        memory_usage: memory,
                                    };
                                    
                                    if tx.try_send(data).is_err() {
                                        debug!("Failed to send container metrics data to channel");
                                    }
                                }
                                
                                // Update last update timestamp
                                metrics_guard.last_update = timestamp;
                            }
                            Err(e) => {
                                if use_api {
                                    // Check if this is a connection error like "buffer's worker closed unexpectedly"
                                    let is_connection_error = e.to_string().contains("worker closed") || 
                                                             e.to_string().contains("connection reset") ||
                                                             e.to_string().contains("broken pipe");
                                                             
                                    // Increment failure counter
                                    let mut failures = metrics_api_failures.lock().unwrap();
                                    *failures += 1;
                                    
                                    // Handle different error types differently
                                    if is_connection_error {
                                        // Connection errors might be temporary - log with appropriate level
                                        if *failures <= 2 {
                                            debug!("Connection error in metrics API (attempt {}): {}", *failures, e);
                                        } else {
                                            warn!("Persistent connection errors in metrics API (attempt {}): {}", *failures, e);
                                        }
                                    } else {
                                        // Other errors are more likely to be persistent
                                        warn!("Metrics API failed for pod {} (attempt {}): {}", pod.name, *failures, e);
                                    }
                                    
                                    // If too many failures or it's a critical error, switch to kubectl
                                    if *failures >= 3 || !is_connection_error {
                                        warn!("Metrics API failed {} times, switching to kubectl", *failures);
                                        
                                        {
                                            let mut api_guard = using_metrics_api.lock().unwrap();
                                            *api_guard = false;
                                        }
                                        
                                        // Try kubectl immediately for this pod
                                        match Self::collect_container_metrics_from_kubectl(pod_namespace, &pod.name).await {
                                            Ok(container_metrics) => {
                                                // Process metrics from kubectl (same as above)
                                                let mut metrics_guard = metrics_maps.lock().unwrap();
                                                
                                                for (container_name, (cpu, memory)) in container_metrics {
                                                    // Similar code as above for updating metrics
                                                    // Insert into CPU map
                                                    let pod_cpu_map = metrics_guard.cpu_map
                                                        .entry(pod.name.clone())
                                                        .or_insert_with(HashMap::new);
                                                    
                                                    let container_cpu_series = pod_cpu_map
                                                        .entry(container_name.clone())
                                                        .or_insert_with(|| TimeSeries::new(&format!("{}-{}-cpu", pod.name, container_name), 1000));
                                                    
                                                    container_cpu_series.add_point(timestamp, cpu);
                                                    
                                                    // Insert into Memory map
                                                    let pod_mem_map = metrics_guard.memory_map
                                                        .entry(pod.name.clone())
                                                        .or_insert_with(HashMap::new);
                                                    
                                                    let container_mem_series = pod_mem_map
                                                        .entry(container_name.clone())
                                                        .or_insert_with(|| TimeSeries::new(&format!("{}-{}-memory", pod.name, container_name), 1000));
                                                    
                                                    container_mem_series.add_point(timestamp, memory);
                                                    
                                                    // Send the metrics data to the channel
                                                    let data = MetricsData {
                                                        timestamp,
                                                        pod_name: pod.name.clone(),
                                                        container_name,
                                                        cpu_usage: cpu,
                                                        memory_usage: memory,
                                                    };
                                                    
                                                    if tx.try_send(data).is_err() {
                                                        debug!("Failed to send container metrics data to channel");
                                                    }
                                                }
                                                
                                                // Update last update timestamp
                                                metrics_guard.last_update = timestamp;
                                            }
                                            Err(kubectl_err) => {
                                                error!("Both metrics API and kubectl failed for pod {}: API error: {}, kubectl error: {}", 
                                                       pod.name, e, kubectl_err);
                                            }
                                        }
                                    }
                                } else {
                                    // Already using kubectl and it failed
                                    warn!("Failed to collect metrics for pod {} via kubectl: {}", pod.name, e);
                                }
                            }
                        }
                    }
                    
                    // Update last collection time
                    last_collection = tokio::time::Instant::now();
                }
                
                // Sleep to prevent high CPU usage
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            
            // When the loop exits, set collection_active to false
            {
                let mut active_guard = collection_active.lock().unwrap();
                *active_guard = false;
            }
            
            info!("Container metrics collection task completed");
        });
        
        Ok(rx)
    }
    
    /// Collect container metrics using the Kubernetes metrics API
    async fn collect_container_metrics_from_api(
        metrics_client: &crate::k8s::metrics::MetricsClient,
        namespace: &str,
        pod_name: &str
    ) -> Result<Vec<(String, (f64, f64))>> {
        // Use the metrics API client to fetch container metrics
        let container_metrics = metrics_client.get_pod_container_metrics(namespace, pod_name).await?;
        
        // Convert the metrics format
        let mut result = Vec::new();
        
        for (container_name, metrics) in container_metrics {
            let cpu_usage = metrics.cpu.usage_value * 100.0; // Convert to percentage (0-100)
            let memory_usage = metrics.memory.usage_value / (1024.0 * 1024.0); // Convert to MB
            
            result.push((container_name, (cpu_usage, memory_usage)));
        }
        
        if result.is_empty() {
            warn!("No container metrics found via API for pod {}/{}", namespace, pod_name);
        } else {
            debug!("Collected metrics for {} containers in pod {}/{} via API", 
                  result.len(), namespace, pod_name);
        }
        
        Ok(result)
    }
    
    /// Collect metrics for all containers in a pod using kubectl
    async fn collect_container_metrics_from_kubectl(namespace: &str, pod_name: &str) -> Result<Vec<(String, (f64, f64))>> {
        use std::process::Command;
        
        debug!("Collecting container metrics via kubectl for pod {}/{}", namespace, pod_name);
        
        // Use kubectl top pod to get container-level metrics
        let output = Command::new("kubectl")
            .args(&["top", "pod", pod_name, "-n", namespace, "--containers", "--no-headers"])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!(
                "Failed to get pod resource usage: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut container_resources = Vec::new();
        
        // Parse each container's usage with fixed parsing logic
        for line in output_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 { // Need pod, container, CPU, and memory
                let container_name = parts[1].to_string();
                
                // Parse CPU usage (remove "m" suffix and convert to percentage)
                let cpu_str = parts[2];
                let cpu_usage = if cpu_str.ends_with('m') {
                    // Convert millicores to percentage (1000m = 1 core = 100%)
                    let millicores = match cpu_str.trim_end_matches('m').parse::<f64>() {
                        Ok(mc) => mc,
                        Err(_) => {
                            warn!("Failed to parse CPU millicores: {}", cpu_str);
                            0.0
                        }
                    };
                    millicores / 10.0 // 1000m = 100%
                } else {
                    // Direct core count to percentage
                    match cpu_str.parse::<f64>() {
                        Ok(c) => c * 100.0, // Convert cores to percentage
                        Err(_) => {
                            warn!("Failed to parse CPU cores: {}", cpu_str);
                            0.0
                        }
                    }
                };
                
                // Parse Memory usage
                let mem_str = parts[3];
                
                // Extract numeric part and unit separately
                let mem_value: f64;
                let mem_unit = mem_str.chars()
                    .skip_while(|c| c.is_digit(10) || *c == '.')
                    .collect::<String>();
                
                let mem_number = match mem_str.chars()
                    .take_while(|c| c.is_digit(10) || *c == '.')
                    .collect::<String>()
                    .parse::<f64>() {
                    Ok(v) => v,
                    Err(_) => {
                        warn!("Failed to parse memory value: {}", mem_str);
                        0.0
                    }
                };
                    
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
                
                // Log the parsed values for debugging
                debug!(
                    "Container metrics: {} - CPU: {}% ({}), Memory: {}MB ({})",
                    container_name, cpu_usage, cpu_str, mem_value, mem_str
                );
                
                container_resources.push((container_name, (cpu_usage, mem_value)));
            }
        }
        
        // If no containers were found, log a warning
        if container_resources.is_empty() {
            warn!("No container metrics found via kubectl for pod {}/{}", namespace, pod_name);
        } else {
            debug!("Collected metrics for {} containers in pod {}/{} via kubectl", 
                 container_resources.len(), namespace, pod_name);
        }
        
        Ok(container_resources)
    }
    
    /// Legacy method that redirects to kubectl version for backward compatibility
    pub async fn collect_container_metrics(namespace: &str, pod_name: &str) -> Result<Vec<(String, (f64, f64))>> {
        Self::collect_container_metrics_from_kubectl(namespace, pod_name).await
    }
}