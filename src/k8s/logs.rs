use crate::cli::Args;
use crate::k8s::pod::select_pods;
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

impl Default for LogEntry {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            pod_name: String::new(),
            container_name: String::new(),
            message: String::new(),
            timestamp: None,
        }
    }
}

/// Watches and streams logs from multiple pods/containers
pub struct LogWatcher {
    client: Client,
    pub args: Arc<Args>,
}

impl LogWatcher {
    /// Creates a new log watcher
    pub fn new(client: Client, args: &Args) -> Self {
        info!("LOG_WATCHER: Creating new LogWatcher with args: namespace={}, pod_selector={}, container={}", 
              args.namespace, args.pod_selector, args.container);
        Self {
            client,
            args: Arc::new(args.clone()),
        }
    }
    
    /// Starts streaming logs from all matching pods/containers
    pub async fn stream(&self) -> Result<Pin<Box<dyn Stream<Item = LogEntry> + Send>>> {
        info!("LOG_WATCHER: Starting to stream logs...");
        let pod_regex = self.args.pod_regex()
            .context("Invalid pod selector regex")?;
        
        info!("LOG_WATCHER: Pod regex: {:?}", pod_regex.as_str());
        
        // If no specific container is provided (i.e., default pattern ".*") and 
        // all_containers is false, we need to behave like kubectl logs and use the first container
        let use_first_container = self.args.container == ".*" && !self.args.all_containers;
        
        info!("LOG_WATCHER: Use first container only: {}", use_first_container);
        
        // If we're not using the first container, use the provided container regex
        let container_regex = self.args.container_regex()
            .context("Invalid container selector regex")?;
            
        info!("LOG_WATCHER: Container regex: {:?}", container_regex.as_str());
        
        // Get pods matching our criteria - always get all containers first
        info!("LOG_WATCHER: Selecting pods in namespace: {}", self.args.namespace);
        let pods = select_pods(
            &self.client, 
            &self.args.namespace, 
            &pod_regex, 
            &Regex::new(".*").unwrap(), // Get all containers first, we'll filter later
            self.args.all_namespaces,
            self.args.resource.as_deref(), // Pass the resource query if present
        ).await?;
        
        info!("LOG_WATCHER: Found {} matching pods", pods.len());
        for (i, pod) in pods.iter().enumerate() {
            info!("LOG_WATCHER: Pod #{}: {}/{} with {} containers", 
                  i+1, pod.namespace, pod.name, pod.containers.len());
        }
        
        if pods.is_empty() {
            error!("LOG_WATCHER: No pods found matching the selection criteria");
            return Err(anyhow!("No pods found matching the selection criteria"));
        }
        
        // Create a channel with larger buffer to handle high-volume logs
        // Buffer size is based on number of pods * estimated lines per second * buffer time
        let buffer_size = std::cmp::max(
            5000,  // Increased minimum buffer size from 1000 to 5000
            pods.len() * 500 * 5  // Increased to (pods * ~500 lines/s * 5s buffer)
        );
        info!("LOG_WATCHER: Creating log channel with buffer size: {}", buffer_size);
        let (tx, rx) = mpsc::channel::<LogEntry>(buffer_size);
        
        // Get the since parameter from args
        let since_param = self.args.since.clone();
        info!("LOG_WATCHER: Since parameter: {:?}", since_param);
        
        // Start streaming logs from each container
        info!("LOG_WATCHER: Starting log streams for {} pods", pods.len());
        for pod_info in pods {
            let client = self.client.clone();
            let tail_lines = self.args.tail;
            let follow = self.args.follow;
            let timestamps = self.args.timestamps;
            let tx = tx.clone();
            
            info!("LOG_WATCHER: Processing pod {}/{} with containers: {:?}", 
                  pod_info.namespace, pod_info.name, pod_info.containers);
            
            // If we should use the first container only (kubectl-like behavior)
            if use_first_container && !pod_info.containers.is_empty() {
                let first_container = pod_info.containers[0].clone();
                info!("LOG_WATCHER: Using only first container: {} in pod: {}/{}", 
                      first_container, pod_info.namespace, pod_info.name);
                
                let pod_info_clone = pod_info.clone();
                let tx_clone = tx.clone();
                let client_clone = client.clone();
                let since_clone = since_param.clone();
                
                tokio::spawn(async move {
                    info!("CONTAINER_TASK: Starting log stream for {}/{}/{}", 
                          pod_info_clone.namespace, pod_info_clone.name, first_container);
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
                        error!("CONTAINER_TASK: Error streaming logs from {}/{}/{}: {:?}", 
                               pod_info_clone.namespace, pod_info_clone.name, first_container, e);
                    } else {
                        info!("CONTAINER_TASK: Completed streaming logs from {}/{}/{}", 
                              pod_info_clone.namespace, pod_info_clone.name, first_container);
                    }
                });
            } else {
                // Use all containers that match the regex (or all if all_containers is true)
                info!("LOG_WATCHER: Processing all containers for pod {}/{}", 
                      pod_info.namespace, pod_info.name);
                for container_name in &pod_info.containers {
                    // Skip containers that don't match the regex (unless all_containers is true)
                    if !self.args.all_containers && !container_regex.is_match(container_name) {
                        info!("LOG_WATCHER: Skipping container {} (doesn't match regex)", container_name);
                        continue;
                    }
                    
                    info!("LOG_WATCHER: Including container {} for streaming", container_name);
                    
                    let pod_info = pod_info.clone();
                    let container_name = container_name.clone();
                    let tx = tx.clone();
                    let client = client.clone();
                    let since_clone = since_param.clone();
                    
                    // Spawn a task for each container
                    tokio::spawn(async move {
                        info!("CONTAINER_TASK: Starting log stream for {}/{}/{}", 
                              pod_info.namespace, pod_info.name, container_name);
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
                            error!("CONTAINER_TASK: Error streaming logs from {}/{}/{}: {:?}", 
                                   pod_info.namespace, pod_info.name, container_name, e);
                        } else {
                            info!("CONTAINER_TASK: Completed streaming logs from {}/{}/{}", 
                                  pod_info.namespace, pod_info.name, container_name);
                        }
                    });
                }
            }
        }
        
        info!("LOG_WATCHER: All container streaming tasks spawned, returning stream");
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
        info!("CONTAINER_LOGS: Starting log stream for {}/{}/{}", namespace, pod_name, container_name);
        info!("CONTAINER_LOGS: Params - follow: {}, tail_lines: {}, timestamps: {}, since: {:?}", 
              follow, tail_lines, timestamps, since);
        
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
            info!("CONTAINER_LOGS: Applied since parameter: {} -> {:?} seconds", 
                  since_val, log_params.since_seconds);
        }
        
        info!("CONTAINER_LOGS: Attempting to get logs for {}/{}/{}", namespace, pod_name, container_name);
        
        // Use the streaming API instead of getting logs all at once
        if follow {
            info!("CONTAINER_LOGS: Using streaming mode (follow=true)");
            // For streaming logs continuously
            use futures::AsyncBufReadExt;
            use futures::StreamExt;
            
            match pods.log_stream(pod_name, &log_params).await {
                Ok(logs) => {
                    info!("CONTAINER_LOGS: Successfully got log stream for {}/{}/{}", namespace, pod_name, container_name);
                    let mut lines = logs.lines();
                    let mut line_count = 0;
                    
                    while let Some(line_result) = lines.next().await {
                        match line_result {
                            Ok(line) => {
                                let line = line.trim();
                                if line.is_empty() { 
                                    debug!("CONTAINER_LOGS: Skipping empty line");
                                    continue; 
                                }
                                
                                line_count += 1;
                                if line_count <= 5 || line_count % 100 == 0 {
                                    info!("CONTAINER_LOGS: Processing line #{} from {}/{}/{}: {}", 
                                          line_count, namespace, pod_name, container_name, 
                                          line.chars().take(50).collect::<String>());
                                }
                                
                                // Extract timestamp if present
                                let (timestamp, message) = if timestamps {
                                    match line.find(' ') {
                                        Some(space_idx) => {
                                            let time_str = &line[0..space_idx];
                                            match chrono::DateTime::parse_from_rfc3339(time_str) {
                                                Ok(dt) => (Some(dt.with_timezone(&chrono::Utc)), line[space_idx+1..].to_string()),
                                                Err(_) => {
                                                    debug!("CONTAINER_LOGS: Failed to parse timestamp: {}", time_str);
                                                    (None, line.to_string())
                                                }
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
                                
                                if let Err(_) = tx.send(entry).await {
                                    debug!("CONTAINER_LOGS: Channel closed, stopping log stream for {}/{}/{}", 
                                           namespace, pod_name, container_name);
                                    return Ok(());
                                } else if line_count <= 5 {
                                    debug!("CONTAINER_LOGS: Successfully sent log entry #{} to channel", line_count);
                                }
                            },
                            Err(e) => {
                                error!("CONTAINER_LOGS: Error reading log line from {}/{}/{}: {:?}", 
                                       namespace, pod_name, container_name, e);
                            }
                        }
                    }
                    
                    info!("CONTAINER_LOGS: Completed streaming {} lines from {}/{}/{}", 
                          line_count, namespace, pod_name, container_name);
                },
                Err(e) => {
                    error!("CONTAINER_LOGS: Failed to get log stream for {}/{}/{}: {:?}", 
                           namespace, pod_name, container_name, e);
                    return Err(e.into());
                }
            }
        } else {
            info!("CONTAINER_LOGS: Using one-time fetch mode (follow=false)");
            // For one-time fetch of logs
            match pods.logs(pod_name, &log_params).await {
                Ok(logs) => {
                    info!("CONTAINER_LOGS: Successfully fetched logs for {}/{}/{}, processing lines...", 
                          namespace, pod_name, container_name);
                    
                    let lines: Vec<&str> = logs.lines().collect();
                    info!("CONTAINER_LOGS: Fetched {} lines from {}/{}/{}", 
                          lines.len(), namespace, pod_name, container_name);
                    
                    // Process each log line
                    for (i, line) in lines.iter().enumerate() {
                        let message = line.trim().to_string();
                        
                        // Skip empty lines
                        if message.is_empty() {
                            debug!("CONTAINER_LOGS: Skipping empty line #{}", i+1);
                            continue;
                        }
                        
                        if i < 5 || i % 100 == 0 {
                            info!("CONTAINER_LOGS: Processing line #{} from {}/{}/{}: {}", 
                                  i+1, namespace, pod_name, container_name, 
                                  message.chars().take(50).collect::<String>());
                        }
                        
                        // Extract timestamp if present
                        let (timestamp, message) = if timestamps {
                            // Parse timestamp from the beginning of the message
                            match message.find(' ') {
                                Some(space_idx) => {
                                    let time_str = &message[0..space_idx];
                                    match chrono::DateTime::parse_from_rfc3339(time_str) {
                                        Ok(dt) => (Some(dt.with_timezone(&chrono::Utc)), message[space_idx+1..].to_string()),
                                        Err(_) => {
                                            debug!("CONTAINER_LOGS: Failed to parse timestamp: {}", time_str);
                                            (None, message)
                                        }
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
                        
                        if let Err(_) = tx.send(entry).await {
                            debug!("CONTAINER_LOGS: Channel closed, stopping log processing for {}/{}/{}", 
                                   namespace, pod_name, container_name);
                            break;
                        } else if i < 5 {
                            debug!("CONTAINER_LOGS: Successfully sent log entry #{} to channel", i+1);
                        }
                    }
                    
                    info!("CONTAINER_LOGS: Completed processing {} lines from {}/{}/{}", 
                          lines.len(), namespace, pod_name, container_name);
                },
                Err(e) => {
                    error!("CONTAINER_LOGS: Failed to fetch logs for {}/{}/{}: {:?}", 
                           namespace, pod_name, container_name, e);
                    return Err(e.into());
                }
            }
        }
        
        info!("CONTAINER_LOGS: Log stream ended for {}/{}/{}", namespace, pod_name, container_name);
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