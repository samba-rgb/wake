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
        let namespace_arg = if self.namespace == "*" {
            "--all-namespaces".to_string()
        } else {
            format!("--namespace={}", self.namespace)
        };

        // Run kubectl top pod command
        let output = Command::new("kubectl")
            .args(["top", "pod", &namespace_arg, "--no-headers"])
            .output()
            .map_err(|e| anyhow!("Failed to execute kubectl top: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("kubectl top failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pod_selector_regex = Regex::new(pod_selector)
            .map_err(|e| anyhow!("Invalid pod selector regex: {}", e))?;
        
        let mut result = HashMap::new();
        let timestamp = Utc::now();

        // Parse the output of kubectl top pod
        for line in stdout.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            
            if fields.len() < 3 {
                continue; // Skip lines with insufficient data
            }
            
            let pod_name = if self.namespace == "*" {
                // When --all-namespaces is used, format is "namespace name cpu memory"
                if fields.len() < 4 {
                    continue;
                }
                fields[1]
            } else {
                // Format is "name cpu memory"
                fields[0]
            };

            // Apply pod selector filter
            if !pod_selector_regex.is_match(pod_name) {
                continue;
            }

            // Get CPU usage (2nd or 3rd field depending on namespace)
            let cpu_idx = if self.namespace == "*" { 2 } else { 1 };
            let cpu_str = fields[cpu_idx];
            
            // Get Memory usage (3rd or 4th field depending on namespace)
            let mem_idx = if self.namespace == "*" { 3 } else { 2 };
            let mem_str = fields[mem_idx];
            
            // Find the pod in our list to get resource limits/requests
            let pod = pods.iter().find(|p| {
                p.metadata.name.as_ref().map_or(false, |name| name == pod_name)
            });
            
            // Create resource metrics
            let cpu = self.create_resource_metrics_from_kubectl("cpu", cpu_str, pod);
            let memory = self.create_resource_metrics_from_kubectl("memory", mem_str, pod);
            
            // Create pod metrics entry
            let pod_metrics_entry = PodMetrics {
                timestamp,
                window: Some(Duration::from_secs(60)), // kubectl top typically uses a ~60s window
                cpu,
                memory,
            };
            
            result.insert(pod_name.to_string(), pod_metrics_entry);
        }
        
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
                format!("{}m", (total_usage * 1000.0) as u64)
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
}

/// Parse a Kubernetes quantity string into a float value
/// Handles formats like "100m" for CPU and "256Mi" for memory
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
        // Apply unit multiplier
        match unit_part.as_str() {
            // CPU units
            "m" => Some(value / 1000.0), // millicores to cores
            
            // Memory units
            "Ki" | "KiB" => Some(value * 1024.0),
            "Mi" | "MiB" => Some(value * 1024.0 * 1024.0),
            "Gi" | "GiB" => Some(value * 1024.0 * 1024.0 * 1024.0),
            "Ti" | "TiB" => Some(value * 1024.0 * 1024.0 * 1024.0 * 1024.0),
            
            // Decimal memory units
            "K" | "k" | "KB" | "kB" => Some(value * 1000.0),
            "M" | "MB" => Some(value * 1000.0 * 1000.0),
            "G" | "GB" => Some(value * 1000.0 * 1000.0 * 1000.0),
            "T" | "TB" => Some(value * 1000.0 * 1000.0 * 1000.0 * 1000.0),
            
            // No unit or unknown unit
            "" => Some(value),
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