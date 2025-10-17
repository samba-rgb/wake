use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use k8s_openapi::{
    api::core::v1::{Container, Pod}, 
    apimachinery::pkg::api::resource::Quantity
};
use kube::{
    api::{Api, ListParams},
    Client,
};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use crate::logging::wake_logger;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetricsSource {
    Api,
    KubectlTop,
}

/// Client for interacting with Kubernetes metrics (via API or kubectl top)
pub struct MetricsClient {
    client: Client,
    namespace: String,
    metrics_api_available: bool,
    preferred_source: MetricsSource,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PodMetrics {
    pub timestamp: DateTime<Utc>,
    pub window: Option<Duration>,
    pub cpu: ResourceMetrics,
    pub memory: ResourceMetrics,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceMetrics {
    pub usage: String,           // Current usage (e.g., "120m" for CPU, "256Mi" for memory)
    pub usage_value: f64,        // Numeric value for comparisons
    pub request: Option<String>, // Resource request if specified
    pub limit: Option<String>,   // Resource limit if specified
    pub utilization: f64,        // Usage as percentage of request or limit
}

impl MetricsClient {
    pub async fn new(client: Client, namespace: String) -> Result<Self> {
        let instance = Self {
            client,
            namespace,
            metrics_api_available: false,
            preferred_source: MetricsSource::Api,
        };
        
        // Check if metrics API is available
        let available = instance.check_metrics_api_available().await;
        
        Ok(Self {
            metrics_api_available: available,
            preferred_source: if available { MetricsSource::Api } else { MetricsSource::KubectlTop },
            ..instance
        })
    }

    /// Set the preferred metrics source
    pub fn set_metrics_source(&mut self, source: MetricsSource) {
        self.preferred_source = source;
        if source == MetricsSource::Api && !self.metrics_api_available {
            warn!("Metrics API requested but not available. Falling back to kubectl top.");
            self.preferred_source = MetricsSource::KubectlTop;
        }
    }

    /// Check if the metrics API is available
    pub async fn check_metrics_api_available(&self) -> bool {
        // Try to access the metrics API endpoint
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri("/apis/metrics.k8s.io/v1beta1/pods")
            .body(vec![])
            .map_err(|_| kube::Error::Api(kube::error::ErrorResponse{
                status: "RequestError".to_string(),
                code: 500,
                message: "Failed to build request".to_string(),
                reason: "InternalError".to_string(),
            }));
        
        match self.client
            .request::<serde_json::Value>(request.unwrap_or_default())
            .await
        {
            Ok(_) => {
                info!("Metrics API is available");
                true
            },
            Err(e) => {
                warn!("Metrics API is not available: {}", e);
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
        match self.preferred_source {
            MetricsSource::Api => {
                if self.metrics_api_available {
                    self.get_pod_metrics_from_api(pod_selector, pods).await
                } else {
                    info!("Metrics API not available, falling back to kubectl top");
                    self.get_pod_metrics_from_kubectl(pod_selector, pods).await
                }
            },
            MetricsSource::KubectlTop => {
                self.get_pod_metrics_from_kubectl(pod_selector, pods).await
            }
        }
    }

    /// Fetch metrics for pods using the metrics API
    async fn get_pod_metrics_from_api(
        &self,
        pod_selector: &str,
        pods: &[Pod],
    ) -> Result<HashMap<String, PodMetrics>> {
        // Prepare URL with namespace filter
        let url = if self.namespace == "*" {
            "/apis/metrics.k8s.io/v1beta1/pods".to_string()
        } else {
            format!("/apis/metrics.k8s.io/v1beta1/namespaces/{}/pods", self.namespace)
        };

        // Request metrics data
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .body(vec![])
            .map_err(|e| anyhow!("Failed to build HTTP request: {}", e))?;
        
        let metrics_response = self.client
            .request::<serde_json::Value>(request)
            .await?;

        // Parse response
        let pod_selector_regex = Regex::new(pod_selector)
            .map_err(|e| anyhow!("Invalid pod selector regex: {}", e))?;

        let mut result = HashMap::new();
        
        // Extract items array from the response
        if let Some(items) = metrics_response.get("items").and_then(|i| i.as_array()) {
            for pod_metrics in items {
                // Extract pod name from metadata
                let pod_name = pod_metrics
                    .get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| anyhow!("Pod metrics missing name"))?;
                
                // Apply pod selector filter
                if !pod_selector_regex.is_match(pod_name) {
                    continue;
                }
                
                // Extract timestamp
                let timestamp_str = pod_metrics
                    .get("timestamp")
                    .and_then(|ts| ts.as_str())
                    .ok_or_else(|| anyhow!("Pod metrics missing timestamp"))?;
                
                let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
                    .map_err(|e| anyhow!("Invalid timestamp format: {}", e))?
                    .into();
                
                // Extract window
                let window = pod_metrics
                    .get("window")
                    .and_then(|w| w.as_str())
                    .map(|w| parse_k8s_duration(w).unwrap_or_else(|_| Duration::from_secs(60)));
                
                // Extract container metrics and aggregate for the pod
                let containers = pod_metrics
                    .get("containers")
                    .and_then(|c| c.as_array())
                    .ok_or_else(|| anyhow!("Pod metrics missing containers"))?;
                
                // Find the pod in our list to get resource limits/requests
                let pod = pods.iter().find(|p| {
                    p.metadata.name.as_ref().map_or(false, |name| name == pod_name)
                });
                
                // Extract CPU metrics
                let cpu = self.extract_resource_metrics("cpu", containers, pod);
                
                // Extract memory metrics
                let memory = self.extract_resource_metrics("memory", containers, pod);
                
                // Create pod metrics entry
                let pod_metrics_entry = PodMetrics {
                    timestamp,
                    window,
                    cpu,
                    memory,
                };
                
                result.insert(pod_name.to_string(), pod_metrics_entry);
            }
        }
        
        Ok(result)
    }

    /// Fetch metrics for pods using kubectl top command
    async fn get_pod_metrics_from_kubectl(
        &self,
        pod_selector: &str,
        pods: &[Pod],
    ) -> Result<HashMap<String, PodMetrics>> {
        let mut result = HashMap::new();
        let timestamp = Utc::now();

        wake_logger::info(&format!("Fetching metrics using kubectl top for {} pods with selector {}", pods.len(), pod_selector));
        
        // Process each pod individually to get more accurate results
        for pod in pods {
            let pod_name = if let Some(name) = &pod.metadata.name {
                name
            } else {
                wake_logger::debug("Pod has no name, skipping");
                continue;
            };
            
            let namespace = if let Some(ns) = &pod.metadata.namespace {
                ns
            } else {
                wake_logger::debug(&format!("Pod {} has no namespace, using default", pod_name));
                "default"
            };
            
            wake_logger::debug(&format!("Getting metrics for pod {}/{}", namespace, pod_name));
            
            // Use the --containers flag to get per-container metrics
            let output = Command::new("kubectl")
                .args(["top", "pod", pod_name, "-n", namespace, "--containers", "--no-headers"])
                .output()
                .map_err(|e| {
                    wake_logger::error(&format!("Failed to execute kubectl top for pod {}: {}", pod_name, e));
                    anyhow!("Failed to execute kubectl top for pod {}: {}", pod_name, e)
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                wake_logger::error(&format!("kubectl top failed for pod {}: {}", pod_name, stderr));
                continue; // Skip this pod but continue processing others
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            wake_logger::debug(&format!("kubectl top output for pod {}: {}", pod_name, stdout));
            
            // Parse the output of kubectl top pod --containers
            // Output format: POD NAME     CONTAINER NAME     CPU(cores)   MEMORY(bytes)
            // or if pod name is included: POD     CONTAINER   CPU    MEMORY
            
            // We'll aggregate the container metrics to get pod metrics
            let mut cpu_total = 0.0;
            let mut mem_total = 0.0;
            let mut containers_count = 0;
            
            for line in stdout.lines() {
                let fields: Vec<&str> = line.split_whitespace().collect();
                
                // If we have pod name included, need at least 4 fields
                if fields.len() < 4 {
                    wake_logger::debug(&format!("Line doesn't have enough fields, skipping: {}", line));
                    continue;
                }
                
                // The container's CPU will be in the third field (index 2)
                let cpu_str = fields[2];
                
                // The container's memory will be in the fourth field (index 3)
                let mem_str = fields[3];
                
                wake_logger::debug(&format!("Container metrics: CPU={}, Memory={}", cpu_str, mem_str));
                
                // Parse CPU usage
                let cpu_value = if cpu_str.ends_with('m') {
                    // If in millicores, keep as millicores
                    match cpu_str[0..cpu_str.len()-1].parse::<f64>() {
                        Ok(v) => v,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse CPU value: {}", cpu_str));
                            continue;
                        }
                    }
                } else {
                    // Convert cores to millicores
                    match cpu_str.parse::<f64>() {
                        Ok(v) => v * 1000.0,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse CPU value: {}", cpu_str));
                            continue;
                        }
                    }
                };
                
                // Parse memory usage
                let mem_value = if mem_str.ends_with("Mi") {
                    // If in MiB, convert to bytes
                    match mem_str[0..mem_str.len()-2].parse::<f64>() {
                        Ok(v) => v * 1024.0 * 1024.0,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse memory value: {}", mem_str));
                            continue;
                        }
                    }
                } else if mem_str.ends_with("Ki") {
                    // If in KiB, convert to bytes
                    match mem_str[0..mem_str.len()-2].parse::<f64>() {
                        Ok(v) => v * 1024.0,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse memory value: {}", mem_str));
                            continue;
                        }
                    }
                } else if mem_str.ends_with("Gi") {
                    // If in GiB, convert to bytes
                    match mem_str[0..mem_str.len()-2].parse::<f64>() {
                        Ok(v) => v * 1024.0 * 1024.0 * 1024.0,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse memory value: {}", mem_str));
                            continue;
                        }
                    }
                } else {
                    // Assume it's in bytes
                    match mem_str.parse::<f64>() {
                        Ok(v) => v,
                        Err(_) => {
                            wake_logger::error(&format!("Failed to parse memory value: {}", mem_str));
                            continue;
                        }
                    }
                };
                
                cpu_total += cpu_value;
                mem_total += mem_value;
                containers_count += 1;
            }
            
            if containers_count > 0 {
                // Create resource metrics for the pod by aggregating container metrics
                let cpu = ResourceMetrics {
                    usage: format!("{} cores", cpu_total),
                    usage_value: cpu_total,
                    request: None, // We don't have this from kubectl top
                    limit: None,   // We don't have this from kubectl top
                    utilization: 0.0, // Utilization cannot be calculated without request/limit
                };
                
                let memory = ResourceMetrics {
                    usage: format!("{} bytes", mem_total),
                    usage_value: mem_total,
                    request: None, // We don't have this from kubectl top
                    limit: None,   // We don't have this from kubectl top
                    utilization: 0.0, // Utilization cannot be calculated without request/limit
                };
                
                // Create pod metrics entry
                let pod_metrics_entry = PodMetrics {
                    timestamp,
                    window: Some(Duration::from_secs(60)), // kubectl top typically uses a ~60s window
                    cpu,
                    memory,
                };
                
                wake_logger::debug(&format!("Created pod metrics for {}: CPU={}, Memory={}", 
                       pod_name, cpu_total, mem_total));
                
                result.insert(pod_name.to_string(), pod_metrics_entry);
            } else {
                wake_logger::debug(&format!("No container metrics found for pod {}", pod_name));
            }
        }
        
        wake_logger::info(&format!("Collected metrics for {} pods using kubectl top", result.len()));
        
        Ok(result)
    }

    /// Create ResourceMetrics from kubectl top output
    fn create_resource_metrics_from_kubectl(
        &self,
        resource_type: &str,
        usage_str: &str,
        pod: Option<&Pod>,
    ) -> ResourceMetrics {
        // Parse the usage value
        let usage_value = parse_quantity(usage_str).unwrap_or(0.0);
        
        // Determine resource requests and limits from pod spec
        let (request, limit) = if let Some(pod) = pod {
            let mut req_sum = 0.0;
            let mut lim_sum = 0.0;
            
            if let Some(spec) = &pod.spec {
                let containers = &spec.containers;
                
                for container in containers {
                    if let Some(resources) = &container.resources {
                        // Get resource requests
                        if let Some(requests) = &resources.requests {
                            if let Some(req) = requests.get(resource_type) {
                                if let Some(value) = parse_quantity(&req.0) {
                                    req_sum += value;
                                }
                            }
                        }
                        
                        // Get resource limits
                        if let Some(limits) = &resources.limits {
                            if let Some(lim) = limits.get(resource_type) {
                                if let Some(value) = parse_quantity(&lim.0) {
                                    lim_sum += value;
                                }
                            }
                        }
                    }
                }
            }
            
            // Format request and limit strings
            let req_str = if req_sum > 0.0 {
                Some(if resource_type == "cpu" {
                    format!("{}m", (req_sum * 1000.0) as u64)
                } else { // memory
                    format!("{}Ki", (req_sum / 1024.0) as u64)
                })
            } else {
                None
            };
            
            let lim_str = if lim_sum > 0.0 {
                Some(if resource_type == "cpu" {
                    format!("{}m", (lim_sum * 1000.0) as u64)
                } else { // memory
                    format!("{}Ki", (lim_sum / 1024.0) as u64)
                })
            } else {
                None
            };
            
            (req_str, lim_str)
        } else {
            (None, None)
        };
        
        // Calculate utilization percentage
        let utilization = if let Some(request) = &request {
            if let Some(req_value) = parse_quantity(request) {
                if req_value > 0.0 {
                    usage_value / req_value * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else if let Some(limit) = &limit {
            if let Some(lim_value) = parse_quantity(limit) {
                if lim_value > 0.0 {
                    usage_value / lim_value * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        ResourceMetrics {
            usage: usage_str.to_string(),
            usage_value,
            request,
            limit,
            utilization,
        }
    }

    /// Extract and aggregate resource metrics for a specific resource type
    fn extract_resource_metrics(
        &self,
        resource_type: &str,
        containers: &[serde_json::Value],
        pod: Option<&Pod>,
    ) -> ResourceMetrics {
        // Sum up usage across all containers
        let mut total_usage = 0.0;
        let mut usage_str = String::new();
        
        // containers is Vec<serde_json::Value>, not an Option<Vec<>>
        for container in containers {
            if let Some(usage) = container
                .get("usage")
                .and_then(|u| u.get(resource_type))
                .and_then(|v| v.as_str())
            {
                // Parse the usage value
                if let Some(value) = parse_quantity(usage) {
                    total_usage += value;
                    
                    // Use the first container's usage format as the template
                    if usage_str.is_empty() {
                        usage_str = usage.to_string();
                    }
                }
            }
        }
        
        // Format the total usage
        if usage_str.is_empty() {
            usage_str = if resource_type == "cpu" {
                format!("{}m", total_usage as u64) // Format directly in millicores
            } else { // memory
                format!("{}Ki", (total_usage / 1024.0) as u64)
            };
        } else {
            // Replace the value in the original format
            if let Some(value_idx) = usage_str.find(|c: char| c.is_digit(10)) {
                let suffix = &usage_str[value_idx..].chars().skip_while(|c| c.is_digit(10) || *c == '.').collect::<String>();
                usage_str = format!("{}{}", total_usage, suffix);
            }
        }
        
        // Determine resource requests and limits from pod spec
        let (request, limit) = if let Some(pod) = pod {
            let mut req_sum = 0.0;
            let mut lim_sum = 0.0;
            
            if let Some(spec) = &pod.spec {
                // Access containers from the spec - spec.containers is Vec<Container>, not Option<Vec<Container>>
                let containers = &spec.containers;
                
                // Process each container in the vector
                for container in containers {
                    if let Some(resources) = &container.resources {
                        // Get resource requests
                        if let Some(requests) = &resources.requests {
                            if let Some(req) = requests.get(resource_type) {
                                if let Some(value) = parse_quantity(&req.0) {
                                    req_sum += value;
                                }
                            }
                        }
                        
                        // Get resource limits
                        if let Some(limits) = &resources.limits {
                            if let Some(lim) = limits.get(resource_type) {
                                if let Some(value) = parse_quantity(&lim.0) {
                                    lim_sum += value;
                                }
                            }
                        }
                    }
                }
            }
            
            // Format request and limit strings
            let req_str = if req_sum > 0.0 {
                Some(if resource_type == "cpu" {
                    format!("{}m", req_sum as u64) // Format directly in millicores
                } else { // memory
                    format!("{}Ki", (req_sum / 1024.0) as u64)
                })
            } else {
                None
            };
            
            let lim_str = if lim_sum > 0.0 {
                Some(if resource_type == "cpu" {
                    format!("{}m", lim_sum as u64) // Format directly in millicores
                } else { // memory
                    format!("{}Ki", (lim_sum / 1024.0) as u64)
                })
            } else {
                None
            };
            
            (req_str, lim_str)
        } else {
            (None, None)
        };
        
        // Calculate utilization percentage
        let utilization = if let Some(request) = &request {
            if let Some(req_value) = parse_quantity(request) {
                if req_value > 0.0 {
                    total_usage / req_value * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else if let Some(limit) = &limit {
            if let Some(lim_value) = parse_quantity(limit) {
                if lim_value > 0.0 {
                    total_usage / lim_value * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        ResourceMetrics {
            usage: usage_str,
            usage_value: total_usage,
            request,
            limit,
            utilization,
        }
    }

    /// Get metrics for individual containers in a pod
    pub async fn get_pod_container_metrics(
        &self,
        namespace: &str,
        pod_name: &str
    ) -> Result<HashMap<String, PodMetrics>> {
        // First check if metrics API is available before attempting to use it
        if (!self.metrics_api_available) {
            debug!("Metrics API not available, cannot fetch container metrics");
            return Err(anyhow!("Metrics API is not available"));
        }

        // Number of retry attempts
        let max_retries = 3;
        let mut retry_count = 0;
        let mut last_error = None;
        let mut connection_error_count = 0;

        // Retry loop
        while retry_count <= max_retries {
            if retry_count > 0 {
                debug!("Retrying container metrics fetch for pod {} (attempt {}/{})", pod_name, retry_count, max_retries);
                // Add a progressive delay before retrying based on error type
                let delay = if connection_error_count > 0 {
                    // Use exponential backoff for connection errors
                    std::time::Duration::from_millis(500 * 2_u64.pow(connection_error_count as u32))
                } else {
                    std::time::Duration::from_millis(500 * retry_count)
                };
                tokio::time::sleep(delay).await;
            }

            // Use a timeout for the metrics API request to avoid hanging
            let timeout_result = tokio::time::timeout(
                std::time::Duration::from_secs(5), // 5 second timeout
                async {
                    // Prepare URL to get metrics for a specific pod
                    let url = format!("/apis/metrics.k8s.io/v1beta1/namespaces/{}/pods/{}", namespace, pod_name);
                    
                    // Request metrics data for the specific pod
                    let request = http::Request::builder()
                        .method(http::Method::GET)
                        .uri(&url)
                        .header("Accept", "application/json")
                        .header("Connection", "keep-alive")
                        .body(vec![])
                        .map_err(|e| anyhow!("Failed to build HTTP request: {}", e))?;
                    
                    match self.client.request::<serde_json::Value>(request).await {
                        Ok(pod_metrics) => {
                            // Extract container metrics from the response
                            let mut result = HashMap::new();
                            let timestamp = pod_metrics
                                .get("timestamp")
                                .and_then(|ts| ts.as_str())
                                .and_then(|ts_str| DateTime::parse_from_rfc3339(ts_str).ok())
                                .map(|dt| dt.into())
                                .unwrap_or_else(Utc::now);
                            
                            let window = pod_metrics
                                .get("window")
                                .and_then(|w| w.as_str())
                                .and_then(|w| parse_k8s_duration(w).ok());
                            
                            // Parse container metrics from the pod metrics response
                            if let Some(containers) = pod_metrics.get("containers").and_then(|c| c.as_array()) {
                                for container in containers {
                                    // Extract container name
                                    if let Some(container_name) = container.get("name").and_then(|n| n.as_str()) {
                                        // Extract CPU and memory usage
                                        if let Some(usage) = container.get("usage") {
                                            let cpu = self.extract_container_resource_metric("cpu", usage);
                                            let memory = self.extract_container_resource_metric("memory", usage);
                                            
                                            result.insert(container_name.to_string(), PodMetrics {
                                                timestamp,
                                                window,
                                                cpu,
                                                memory,
                                            });
                                        }
                                    }
                                }
                            }
                            
                            if result.is_empty() {
                                Err(anyhow!("No container metrics found in the response"))
                            } else {
                                Ok(result)
                            }
                        },
                        Err(e) => {
                            debug!("Failed to fetch container metrics for pod {}: {}", pod_name, e);
                            
                            // Check for specific error types that indicate connection issues
                            let error_msg = e.to_string();
                            if error_msg.contains("worker closed") || 
                               error_msg.contains("connection reset") ||
                               error_msg.contains("broken pipe") ||
                               error_msg.contains("connection refused") ||
                               error_msg.contains("EOF during handshake") {
                                connection_error_count += 1;
                                warn!("Connection error fetching container metrics (count: {}): {}", 
                                      connection_error_count, e);
                                
                                // If we've seen too many connection errors, force a metrics API availability check
                                if connection_error_count >= 2 {
                                    // Trigger availability check on next metrics fetch
                                    Err(anyhow!("Connection unstable, need to verify metrics API availability"))
                                } else {
                                    Err(anyhow!("Connection error fetching container metrics: {}", e))
                                }
                            } else {
                                Err(anyhow!("Failed to fetch container metrics: {}", e))
                            }
                        }
                    }
                }
            ).await;

            // Process the timeout result
            match timeout_result {
                Ok(inner_result) => match inner_result {
                    Ok(metrics) => {
                        // Success, reset any connection error tracking on success
                        if connection_error_count > 0 {
                            debug!("Container metrics fetch succeeded after {} connection errors", connection_error_count);
                        }
                        debug!("Successfully fetched container metrics for pod {}", pod_name);
                        return Ok(metrics); // Success, return metrics
                    },
                    Err(err) => {
                        // Store error for potential later return
                        let err_str = err.to_string();
                        debug!("Error fetching container metrics: {}", err_str);
                        last_error = Some(err);
                        
                        // Special handling for connection stability checks
                        if err_str.contains("need to verify metrics API availability") && connection_error_count >= 2 {
                            debug!("Too many connection errors, will verify metrics API availability");
                            // Force an API check before continuing
                            let api_available = self.check_metrics_api_available().await;
                            if !api_available {
                                warn!("Metrics API no longer available after connection errors");
                                return Err(anyhow!("Metrics API no longer available"));
                            }
                        }
                    }
                },
                Err(_) => {
                    connection_error_count += 1;
                    debug!("Timed out fetching container metrics for pod {} (timeout count: {})", 
                          pod_name, connection_error_count);
                    last_error = Some(anyhow!("Timed out fetching container metrics"));
                }
            }

            // If we got here, there was an error - increment retry counter
            retry_count += 1;
        }

        // If we exhausted all retries, return the last error with context
        let final_error = last_error.unwrap_or_else(|| anyhow!("Failed to fetch container metrics"));
        
        if connection_error_count > 0 {
            Err(anyhow!("Failed to fetch container metrics after {} retries with {} connection errors: {}", 
                       max_retries, connection_error_count, final_error))
        } else {
            Err(anyhow!("Failed to fetch container metrics after {} retries: {}", 
                       max_retries, final_error))
        }
    }
    
    /// Helper to extract resource metrics for a container
    fn extract_container_resource_metric(&self, resource_type: &str, usage: &serde_json::Value) -> ResourceMetrics {
        let usage_str = usage
            .get(resource_type)
            .and_then(|u| u.as_str())
            .unwrap_or("0");
        
        let usage_value = parse_quantity(usage_str).unwrap_or(0.0);
        
        ResourceMetrics {
            usage: usage_str.to_string(),
            usage_value,
            request: None,
            limit: None,
            utilization: 0.0,
        }
    }
}

/// Parse a Kubernetes quantity string into a float value
/// For CPU values, converts everything to millicores (m)
/// For memory values, converts to bytes
fn parse_quantity(quantity: &str) -> Option<f64> {
    // Extract numeric part
    let mut numeric_part = String::new();
    let mut unit_part = String::new();
    
    let mut seen_digit = false;
    for c in quantity.chars() {
        if c.is_digit(10) || c == '.' {
            numeric_part.push(c);
            seen_digit = true;
        } else if seen_digit {
            unit_part.push(c);
        }
    }
    
    if let Ok(value) = numeric_part.parse::<f64>() {
        // Apply unit multiplier based on the unit part
        match unit_part.as_str() {
            // CPU units - convert everything to millicores
            "m" => Some(value), // already in millicores
            "n" => Some(value / 1_000_000.0), // nanocores to millicores
            "" => Some(value * 1000.0), // cores to millicores
            
            // Memory units - convert to bytes
            "Ki" | "KiB" => Some(value * 1024.0),
            "Mi" | "MiB" => Some(value * 1024.0 * 1024.0),
            "Gi" | "GiB" => Some(value * 1024.0 * 1024.0 * 1024.0),
            "Ti" | "TiB" => Some(value * 1024.0 * 1024.0 * 1024.0 * 1024.0),
            
            // Decimal memory units
            "K" | "k" | "KB" | "kB" => Some(value * 1000.0),
            "M" | "MB" => Some(value * 1000.0 * 1000.0),
            "G" | "GB" => Some(value * 1000.0 * 1000.0 * 1000.0),
            "T" | "TB" => Some(value * 1000.0 * 1000.0 * 1000.0 * 1000.0),
            
            // Unknown unit
            _ => {
                warn!("Unknown unit in quantity: {}", quantity);
                Some(value)
            }
        }
    } else {
        None
    }
}

/// Parse a Kubernetes duration string (e.g., "60s") into a Duration
fn parse_k8s_duration(duration: &str) -> Result<Duration> {
    let re = Regex::new(r"^(\d+)([smhd])$")?;
    
    if let Some(caps) = re.captures(duration) {
        let value = caps[1].parse::<u64>()?;
        let unit = &caps[2];
        
        match unit {
            "s" => Ok(Duration::from_secs(value)),
            "m" => Ok(Duration::from_secs(value * 60)),
            "h" => Ok(Duration::from_secs(value * 3600)),
            "d" => Ok(Duration::from_secs(value * 86400)),
            _ => Err(anyhow!("Unknown duration unit: {}", unit)),
        }
    } else {
        Err(anyhow!("Invalid duration format: {}", duration))
    }
}