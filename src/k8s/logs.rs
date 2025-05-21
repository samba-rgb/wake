use crate::cli::Args;
use crate::k8s::pod::{PodInfo, select_pods};
use anyhow::{Result, Context, anyhow};
use futures::Stream;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::LogParams;
use regex::Regex;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, debug, error};

/// Represents a single log entry from a container
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub namespace: String,
    pub pod_name: String,
    pub container_name: String,
    pub message: String,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Watches and streams logs from multiple pods/containers
pub struct LogWatcher {
    client: Client,
    pub args: Arc<Args>,
}

impl LogWatcher {
    /// Creates a new log watcher
    pub fn new(client: Client, args: &Args) -> Self {
        Self {
            client,
            args: Arc::new(args.clone()),
        }
    }
    
    /// Starts streaming logs from all matching pods/containers
    pub async fn stream(&self) -> Result<Pin<Box<dyn Stream<Item = LogEntry> + Send>>> {
        let pod_regex = self.args.pod_regex()
            .context("Invalid pod selector regex")?;
        
        // If no specific container is provided (i.e., default pattern ".*") and 
        // all_containers is false, we need to behave like kubectl logs and use the first container
        let use_first_container = self.args.container == ".*" && !self.args.all_containers;
        
        // If we're not using the first container, use the provided container regex
        let container_regex = self.args.container_regex()
            .context("Invalid container selector regex")?;
        
        // Get pods matching our criteria - always get all containers first
        let pods = select_pods(
            &self.client, 
            &self.args.namespace, 
            &pod_regex, 
            &Regex::new(".*").unwrap(), // Get all containers first, we'll filter later
            self.args.all_namespaces,
            self.args.resource.as_deref(), // Pass the resource query if present
        ).await?;
        
        if pods.is_empty() {
            return Err(anyhow!("No pods found matching the selection criteria"));
        }
        
        // Create a channel with larger buffer to handle high-volume logs
        // Buffer size is based on number of pods * estimated lines per second * buffer time
        let buffer_size = std::cmp::max(
            1000,  // Minimum buffer size
            pods.len() * 100 * 2  // (pods * ~100 lines/s * 2s buffer)
        );
        
        let (tx, rx) = mpsc::channel::<LogEntry>(buffer_size);
        
        // Get the since parameter from args
        let since_param = self.args.since.clone();
        
        // Start streaming logs from each container
        for pod_info in pods {
            let client = self.client.clone();
            let tail_lines = self.args.tail;
            let follow = self.args.follow;
            let timestamps = self.args.timestamps;
            let tx = tx.clone();
            
            // If we should use the first container only (kubectl-like behavior)
            if use_first_container && !pod_info.containers.is_empty() {
                let first_container = pod_info.containers[0].clone();
                debug!("Using only first container: {} in pod: {}/{}", 
                      first_container, pod_info.namespace, pod_info.name);
                
                let pod_info_clone = pod_info.clone();
                let tx_clone = tx.clone();
                let client_clone = client.clone();
                let since_clone = since_param.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = Self::stream_container_logs(
                        client_clone, 
                        &pod_info_clone.namespace, 
                        &pod_info_clone.name, 
                        &first_container,
                        follow,
                        tail_lines,
                        timestamps,
                        tx_clone,
                        since_clone
                    ).await {
                        error!("Error streaming logs from {}/{}/{}: {:?}", 
                               pod_info_clone.namespace, pod_info_clone.name, first_container, e);
                    }
                });
            } else {
                // Use all containers that match the regex (or all if all_containers is true)
                for container_name in &pod_info.containers {
                    // Skip containers that don't match the regex (unless all_containers is true)
                    if !self.args.all_containers && !container_regex.is_match(container_name) {
                        continue;
                    }
                    
                    let pod_info = pod_info.clone();
                    let container_name = container_name.clone();
                    let tx = tx.clone();
                    let client = client.clone();
                    let since_clone = since_param.clone();
                    
                    debug!("Starting log stream for {}/{}/{}", 
                           pod_info.namespace, pod_info.name, container_name);
                    
                    // Spawn a task for each container
                    tokio::spawn(async move {
                        if let Err(e) = Self::stream_container_logs(
                            client, 
                            &pod_info.namespace, 
                            &pod_info.name, 
                            &container_name,
                            follow,
                            tail_lines,
                            timestamps,
                            tx,
                            since_clone
                        ).await {
                            error!("Error streaming logs from {}/{}/{}: {:?}", 
                                   pod_info.namespace, pod_info.name, container_name, e);
                        }
                    });
                }
            }
        }
        
        // Return the receiving stream
        Ok(Box::pin(ReceiverStream::new(rx)))
    }
    
    /// Stream logs from a single container
    async fn stream_container_logs(
        client: Client,
        namespace: &str,
        pod_name: &str,
        container_name: &str,
        follow: bool,
        tail_lines: i64,
        timestamps: bool,
        tx: mpsc::Sender<LogEntry>,
        since: Option<String>,
    ) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(client, namespace);
        
        // Create log params
        let mut log_params = LogParams::default();
        log_params.follow = follow;
        log_params.tail_lines = Some(tail_lines);
        log_params.timestamps = timestamps;
        log_params.container = Some(container_name.to_string());
        
        // Add since parameter if provided
        if let Some(since_val) = since {
            log_params.since_seconds = parse_duration_to_seconds(&since_val).ok();
        }
        
        // Use the streaming API instead of getting logs all at once
        if follow {
            // For streaming logs continuously
            use futures::AsyncBufReadExt;
            use futures::StreamExt;
            
            let logs = pods.log_stream(pod_name, &log_params).await?;
            let mut lines = logs.lines();
            
            while let Some(line_result) = lines.next().await {
                match line_result {
                    Ok(line) => {
                        let line = line.trim();
                        if line.is_empty() { continue; }
                        
                        // Extract timestamp if present
                        let (timestamp, message) = if timestamps {
                            match line.find(' ') {
                                Some(space_idx) => {
                                    let time_str = &line[0..space_idx];
                                    match chrono::DateTime::parse_from_rfc3339(time_str) {
                                        Ok(dt) => (Some(dt.with_timezone(&chrono::Utc)), line[space_idx+1..].to_string()),
                                        Err(_) => (None, line.to_string()),
                                    }
                                },
                                None => (None, line.to_string()),
                            }
                        } else {
                            (None, line.to_string())
                        };
                        
                        // Create and send log entry
                        let entry = LogEntry {
                            namespace: namespace.to_string(),
                            pod_name: pod_name.to_string(),
                            container_name: container_name.to_string(),
                            message,
                            timestamp,
                        };
                        
                        if let Err(e) = tx.send(entry).await {
                            error!("Failed to send log entry: {:?}", e);
                            return Ok(());
                        }
                    },
                    Err(e) => {
                        error!("Error reading log line from {}/{}/{}: {:?}", 
                               namespace, pod_name, container_name, e);
                    }
                }
            }
        } else {
            // For one-time fetch of logs
            let logs = pods.logs(pod_name, &log_params).await?;
            
            // Process each log line
            for line in logs.lines() {
                let message = line.trim().to_string();
                
                // Skip empty lines
                if message.is_empty() {
                    continue;
                }
                
                // Extract timestamp if present
                let (timestamp, message) = if timestamps {
                    // Parse timestamp from the beginning of the message
                    match message.find(' ') {
                        Some(space_idx) => {
                            let time_str = &message[0..space_idx];
                            match chrono::DateTime::parse_from_rfc3339(time_str) {
                                Ok(dt) => (Some(dt.with_timezone(&chrono::Utc)), message[space_idx+1..].to_string()),
                                Err(_) => (None, message),
                            }
                        },
                        None => (None, message),
                    }
                } else {
                    (None, message)
                };
                
                // Create and send log entry
                let entry = LogEntry {
                    namespace: namespace.to_string(),
                    pod_name: pod_name.to_string(),
                    container_name: container_name.to_string(),
                    message,
                    timestamp,
                };
                
                if let Err(e) = tx.send(entry).await {
                    error!("Failed to send log entry: {:?}", e);
                    break;
                }
            }
        }
        
        info!("Log stream ended for {}/{}/{}", namespace, pod_name, container_name);
        Ok(())
    }
}

/// Parse a duration string like "5s", "2m", "3h" into seconds
fn parse_duration_to_seconds(duration: &str) -> Result<i64> {
    // Handle empty string
    if duration.is_empty() {
        return Err(anyhow!("Duration string cannot be empty"));
    }

    // Capture the numeric part and the unit
    let mut numeric_part = String::new();
    let mut unit_part = String::new();
    
    for c in duration.chars() {
        if c.is_ascii_digit() {
            numeric_part.push(c);
        } else {
            unit_part.push(c);
        }
    }
    
    // Check if we have a valid numeric part
    if numeric_part.is_empty() {
        return Err(anyhow!("Missing numeric value in duration string"));
    }
    
    // Parse the numeric part
    let number = numeric_part.parse::<i64>()
        .map_err(|_| anyhow!("Invalid number in duration string"))?;
    
    // Convert to seconds based on unit
    match unit_part.as_str() {
        "s" => Ok(number),
        "m" => Ok(number * 60),
        "h" => Ok(number * 3600),
        "d" => Ok(number * 86400),
        "" => Err(anyhow!("Missing time unit. Expected format like '5s', '2m', '3h'")),
        _ => Err(anyhow!("Invalid time unit '{}'. Supported units: s, m, h, d", unit_part)),
    }
}