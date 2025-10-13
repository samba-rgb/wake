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

/// Structure to hold metrics data for a pod/container
#[derive(Debug, Clone)]
pub struct MetricsData {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage: f64, // CPU usage in cores
    pub memory_usage: f64, // Memory usage in bytes
}

/// Structure for metrics time series
#[derive(Debug, Clone)]
pub struct MetricsTimeSeries {
    pub data: Vec<(DateTime<Utc>, f64, f64)>, // (timestamp, cpu_usage, memory_usage)
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