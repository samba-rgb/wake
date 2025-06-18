use crate::cli::Args;
use crate::k8s::pod::{select_pods, PodInfo};
use anyhow::{Result, Context, anyhow};
use futures::Stream;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::LogParams;
use regex::Regex;
use std::pin::Pin;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
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
    
    /// Intelligently determines the default container for a pod with fallback to first container
    async fn determine_default_container(
        client: &Client,
        namespace: &str,
        pod_info: &PodInfo,
        all_pods: &[PodInfo]
    ) -> String {
        // Strategy 1: Check for Kubernetes default container annotation
        if let Some(annotated_default) = Self::get_annotated_default_container(client, namespace, &pod_info.name).await {
            if pod_info.containers.contains(&annotated_default) {
                info!("LOG_WATCHER: Using annotated default container: {}", annotated_default);
                return annotated_default;
            }
        }
        
        // Strategy 2: Use smart name-based heuristics
        if let Some(smart_default) = Self::get_smart_default_container(&pod_info.containers) {
            info!("LOG_WATCHER: Using smart name-based default: {}", smart_default);
            return smart_default;
        }
        
        // Strategy 3: Use namespace-wide container frequency analysis (only if multiple pods)
        if all_pods.len() > 1 {
            if let Some(frequent_default) = Self::get_most_common_container(all_pods) {
                if pod_info.containers.contains(&frequent_default) {
                    info!("LOG_WATCHER: Using most common container in namespace: {}", frequent_default);
                    return frequent_default;
                }
            }
        }
        
        // Fallback: Use first container (current behavior - always works)
        let first_container = pod_info.containers[0].clone();
        info!("LOG_WATCHER: No clear default found, falling back to first container: {}", first_container);
        first_container
    }
    
    /// Check for kubectl.kubernetes.io/default-container annotation
    async fn get_annotated_default_container(
        client: &Client,
        namespace: &str,
        pod_name: &str
    ) -> Option<String> {
        let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
        
        if let Ok(pod) = pods.get(pod_name).await {
            if let Some(annotations) = &pod.metadata.annotations {
                if let Some(default_container) = annotations.get("kubectl.kubernetes.io/default-container") {
                    debug!("Found annotated default container: {}", default_container);
                    return Some(default_container.clone());
                }
            }
        }
        None
    }
    
    /// Use smart heuristics to find the main application container
    fn get_smart_default_container(containers: &[String]) -> Option<String> {
        // Priority order for common container names (higher score = higher priority)
        let priority_patterns = [
            // Main application containers
            ("app", 100),
            ("main", 95),
            ("application", 90),
            ("server", 85),
            ("service", 80),
            
            // Web/API containers
            ("web", 75),
            ("api", 70),
            ("backend", 65),
            ("frontend", 60),
            
            // Common web servers
            ("nginx", 55),
            ("apache", 50),
            ("httpd", 50),
        ];
        
        let mut best_match = None;
        let mut best_score = 0;
        
        for container in containers {
            let container_lower = container.to_lowercase();
            
            for (pattern, score) in &priority_patterns {
                let current_score = if container_lower == *pattern {
                    *score + 20 // Bonus for exact match
                } else if container_lower.contains(pattern) {
                    *score // Partial match
                } else {
                    continue;
                };
                
                if current_score > best_score {
                    best_score = current_score;
                    best_match = Some(container.clone());
                }
            }
        }
        
        // Only return if we found a meaningful match (score > 50)
        if best_score > 50 {
            debug!("Found smart default container with score {}: {:?}", best_score, best_match);
            best_match
        } else {
            debug!("No smart default found, scores too low");
            None
        }
    }
    
    /// Find the most common container name across all pods in namespace
    fn get_most_common_container(all_pods: &[PodInfo]) -> Option<String> {
        let mut container_counts = HashMap::new();
        
        for pod in all_pods {
            for container in &pod.containers {
                *container_counts.entry(container.clone()).or_insert(0) += 1;
            }
        }
        
        // Only consider containers that appear in multiple pods
        let most_common = container_counts.into_iter()
            .filter(|(_, count)| *count > 1) // Must appear in at least 2 pods
            .max_by_key(|(_, count)| *count)
            .map(|(name, count)| {
                debug!("Most common container: {} (appears {} times)", name, count);
                name
            });
            
        most_common
    }

    /// Starts streaming logs from all matching pods/containers with dynamic pod discovery
    pub async fn stream(&self) -> Result<Pin<Box<dyn Stream<Item = LogEntry> + Send>>> {
        info!("LOG_WATCHER: Starting to stream logs with dynamic pod discovery...");
        let pod_regex = self.args.pod_regex()
            .context("Invalid pod selector regex")?;
        
        info!("LOG_WATCHER: Pod regex: {:?}", pod_regex.as_str());
        
        // If no specific container is provided (i.e., default pattern ".*") and 
        // all_containers is false, we need to behave like kubectl logs and use intelligent default
        let use_smart_default = self.args.container == ".*" && !self.args.all_containers;
        
        info!("LOG_WATCHER: Use smart default container selection: {}", use_smart_default);
        
        // If we're not using smart defaults, use the provided container regex
        let container_regex = self.args.container_regex()
            .context("Invalid container selector regex")?;
            
        info!("LOG_WATCHER: Container regex: {:?}", container_regex.as_str());
        
        // Create a channel with larger buffer to handle high-volume logs
        let buffer_size = 10000; // Increased for dynamic discovery
        info!("LOG_WATCHER: Creating log channel with buffer size: {}", buffer_size);
        let (tx, rx) = mpsc::channel::<LogEntry>(buffer_size);
        
        // Start dynamic pod discovery and monitoring
        let client_clone = self.client.clone();
        let args_clone = self.args.clone();
        let tx_clone = tx.clone();
        
        tokio::spawn(async move {
            if let Err(e) = Self::dynamic_pod_discovery(
                client_clone,
                args_clone,
                tx_clone,
            ).await {
                error!("Dynamic pod discovery failed: {:?}", e);
            }
        });
        
        info!("LOG_WATCHER: Dynamic pod discovery started, returning stream");
        Ok(Box::pin(ReceiverStream::new(rx)))
    }
    
    /// Continuously discovers and monitors pods dynamically
    async fn dynamic_pod_discovery(
        client: Client,
        args: Arc<Args>,
        tx: mpsc::Sender<LogEntry>,
    ) -> Result<()> {
        use tokio::time::{Duration, interval};
        
        let mut known_pods: HashSet<String> = HashSet::new();
        let mut pod_tasks: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
        let mut discovery_interval = interval(Duration::from_secs(5)); // Check every 5 seconds
        
        // Initial pod discovery
        info!("DYNAMIC_DISCOVERY: Starting initial pod discovery");
        Self::discover_and_start_pods(&client, &args, &tx, &mut known_pods, &mut pod_tasks).await?;
        
        loop {
            discovery_interval.tick().await;
            
            // Periodic pod discovery
            match Self::discover_and_start_pods(&client, &args, &tx, &mut known_pods, &mut pod_tasks).await {
                Ok(new_count) => {
                    if new_count > 0 {
                        info!("DYNAMIC_DISCOVERY: Found {} new pods", new_count);
                    }
                }
                Err(e) => {
                    error!("DYNAMIC_DISCOVERY: Failed to discover pods: {:?}", e);
                    // Continue despite errors - might be temporary
                }
            }
            
            // Clean up completed tasks
            pod_tasks.retain(|pod_key, handle| {
                if handle.is_finished() {
                    info!("DYNAMIC_DISCOVERY: Cleaned up completed task for pod: {}", pod_key);
                    false
                } else {
                    true
                }
            });
        }
    }
    
    /// Discovers pods and starts streaming for new ones
    async fn discover_and_start_pods(
        client: &Client,
        args: &Arc<Args>,
        tx: &mpsc::Sender<LogEntry>,
        known_pods: &mut HashSet<String>,
        pod_tasks: &mut HashMap<String, tokio::task::JoinHandle<()>>,
    ) -> Result<usize> {
        let pod_regex = args.pod_regex().context("Invalid pod selector regex")?;
        let container_regex = args.container_regex().context("Invalid container selector regex")?;
        
        // Get current pods
        let pods = select_pods(
            client, 
            &args.namespace, 
            &pod_regex, 
            &Regex::new(".*").unwrap(),
            args.all_namespaces,
            args.resource.as_deref(),
        ).await?;
        
        let mut new_pods_count = 0;
        
        for pod_info in pods {
            let pod_key = format!("{}/{}", pod_info.namespace, pod_info.name);
            
            // Skip if we're already monitoring this pod
            if known_pods.contains(&pod_key) {
                continue;
            }
            
            info!("DYNAMIC_DISCOVERY: New pod detected: {}", pod_key);
            known_pods.insert(pod_key.clone());
            new_pods_count += 1;
            
            // Send notification about new pod
            let discovery_entry = LogEntry {
                namespace: pod_info.namespace.clone(),
                pod_name: pod_info.name.clone(),
                container_name: "system".to_string(),
                message: format!("🆕 New pod discovered and added to monitoring: {}/{}", 
                               pod_info.namespace, pod_info.name),
                timestamp: Some(chrono::Utc::now()),
            };
            let _ = tx.send(discovery_entry).await;
            
            // Determine which containers to monitor
            let use_smart_default = args.container == ".*" && !args.all_containers;
            
            if use_smart_default && !pod_info.containers.is_empty() {
                // Use smart default container selection for new pod
                let default_container = Self::determine_default_container(
                    client,
                    &args.namespace,
                    &pod_info,
                    &[pod_info.clone()], // Single pod for now
                ).await;
                
                let task = Self::spawn_container_task(
                    client.clone(),
                    pod_info.clone(),
                    default_container.clone(),
                    args.clone(),
                    tx.clone(),
                ).await;
                
                pod_tasks.insert(format!("{}/{}", pod_key, default_container), task);
            } else {
                // Monitor all matching containers
                for container_name in &pod_info.containers {
                    if !args.all_containers && !container_regex.is_match(container_name) {
                        continue;
                    }
                    
                    let task = Self::spawn_container_task(
                        client.clone(),
                        pod_info.clone(),
                        container_name.clone(),
                        args.clone(),
                        tx.clone(),
                    ).await;
                    
                    pod_tasks.insert(format!("{}/{}", pod_key, container_name), task);
                }
            }
        }
        
        Ok(new_pods_count)
    }
    
    /// Spawns a task to monitor a specific container
    async fn spawn_container_task(
        client: Client,
        pod_info: PodInfo,
        container_name: String,
        args: Arc<Args>,
        tx: mpsc::Sender<LogEntry>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("CONTAINER_TASK: Starting log stream for {}/{}/{}", 
                  pod_info.namespace, pod_info.name, container_name);
            
            if let Err(e) = Self::stream_container_logs(
                client,
                &pod_info.namespace,
                &pod_info.name,
                &container_name,
                args.follow,
                args.tail,
                args.timestamps,
                tx,
                args.since.clone(),
            ).await {
                error!("CONTAINER_TASK: Error streaming logs from {}/{}/{}: {:?}", 
                       pod_info.namespace, pod_info.name, container_name, e);
            } else {
                info!("CONTAINER_TASK: Completed streaming logs from {}/{}/{}", 
                      pod_info.namespace, pod_info.name, container_name);
            }
        })
    }
    
    /// Stream logs from a single container with retry logic
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
        const MAX_RETRIES: u32 = 3;
        const INITIAL_RETRY_DELAY: u64 = 1000; // 1 second
        const MAX_RETRY_DELAY: u64 = 30000;    // 30 seconds
        
        let mut retry_count = 0;
        let mut retry_delay = INITIAL_RETRY_DELAY;
        
        loop {
            let result = Self::stream_container_logs_once(
                client.clone(),
                namespace,
                pod_name,
                container_name,
                follow,
                tail_lines,
                timestamps,
                tx.clone(),
                since.clone(),
            ).await;
            
            match result {
                Ok(()) => {
                    info!("CONTAINER_LOGS: Successfully completed streaming for {}/{}/{}", 
                          namespace, pod_name, container_name);
                    return Ok(());
                }
                Err(e) => {
                    retry_count += 1;
                    
                    if retry_count > MAX_RETRIES {
                        error!("CONTAINER_LOGS: Max retries ({}) exceeded for {}/{}/{}: {:?}", 
                               MAX_RETRIES, namespace, pod_name, container_name, e);
                        return Err(e);
                    }
                    
                    // Determine if error is retryable
                    if Self::is_retryable_error(&e) {
                        info!("CONTAINER_LOGS: Retryable error for {}/{}/{} (attempt {}/{}): {:?}", 
                              namespace, pod_name, container_name, retry_count, MAX_RETRIES, e);
                        
                        // Send a system notification about the retry
                        let retry_entry = LogEntry {
                            namespace: namespace.to_string(),
                            pod_name: pod_name.to_string(),
                            container_name: container_name.to_string(),
                            message: format!("🔄 Connection lost, retrying... (attempt {}/{}) - {}", 
                                           retry_count, MAX_RETRIES, e),
                            timestamp: Some(chrono::Utc::now()),
                        };
                        
                        // Send retry notification (ignore if channel closed)
                        let _ = tx.send(retry_entry).await;
                        
                        // Exponential backoff with jitter
                        let jitter = fastrand::u64(0..retry_delay / 4); // Up to 25% jitter
                        tokio::time::sleep(std::time::Duration::from_millis(retry_delay + jitter)).await;
                        
                        // Increase delay for next retry, cap at max
                        retry_delay = std::cmp::min(retry_delay * 2, MAX_RETRY_DELAY);
                    } else {
                        error!("CONTAINER_LOGS: Non-retryable error for {}/{}/{}: {:?}", 
                               namespace, pod_name, container_name, e);
                        return Err(e);
                    }
                }
            }
        }
    }
    
    /// Determine if an error is worth retrying
    fn is_retryable_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        
        // Retryable errors
        if error_str.contains("connection") || 
           error_str.contains("timeout") ||
           error_str.contains("network") ||
           error_str.contains("temporary") ||
           error_str.contains("rate limit") ||
           error_str.contains("service unavailable") ||
           error_str.contains("too many requests") {
            return true;
        }
        
        // Non-retryable errors
        if error_str.contains("not found") ||
           error_str.contains("forbidden") ||
           error_str.contains("unauthorized") ||
           error_str.contains("invalid") ||
           error_str.contains("malformed") {
            return false;
        }
        
        // Default to retryable for unknown errors
        true
    }
    
    /// Single attempt at streaming container logs (original implementation)
    async fn stream_container_logs_once(
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
        
        log_params.timestamps = timestamps;
        log_params.container = Some(container_name.to_string());

        // if since is not load tail line
        if since.is_none() {
            log_params.tail_lines = Some(tail_lines); // Set to 0 to load all logs if no since parameter
        }
        
        // Add since parameter if provided
        if let Some(since_val) = since {
            match parse_duration_to_seconds(&since_val) {
                Ok(seconds) => {
                    // Calculate the timestamp by subtracting seconds from now
                    log_params.since_seconds = Some(seconds);
                    info!("CONTAINER_LOGS: Using since parameter: {} seconds ago", seconds);
                },
                Err(e) => {
                    error!("CONTAINER_LOGS: Failed to parse since parameter '{}': {}", since_val, e);
                    return Err(anyhow!("Invalid since parameter '{}': {}", since_val, e));
                }
            }
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
                                // Don't return error for individual line failures, continue streaming
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