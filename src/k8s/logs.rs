use crate::cli::Args;
use crate::k8s::pod::{PodInfo, select_pods};
use anyhow::{Result, Context, anyhow};
use futures::Stream;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::LogParams;
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
        let container_regex = self.args.container_regex()
            .context("Invalid container selector regex")?;
        
        // Get pods matching our criteria
        let pods = select_pods(
            &self.client, 
            &self.args.namespace, 
            &pod_regex, 
            &container_regex, 
            self.args.all_namespaces,
        ).await?;
        
        if pods.is_empty() {
            return Err(anyhow!("No pods found matching the selection criteria"));
        }
        
        // Create a channel for receiving log entries
        let (tx, rx) = mpsc::channel::<LogEntry>(100);
        
        // Start streaming logs from each container
        for pod_info in pods {
            let client = self.client.clone();
            let tail_lines = self.args.tail;
            let follow = self.args.follow;
            let timestamps = self.args.timestamps;
            let tx = tx.clone();
            
            for container_name in &pod_info.containers {
                let pod_info = pod_info.clone();
                let container_name = container_name.clone();
                let tx = tx.clone();
                let client = client.clone();
                
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
                        tx
                    ).await {
                        error!("Error streaming logs from {}/{}/{}: {:?}", 
                               pod_info.namespace, pod_info.name, container_name, e);
                    }
                });
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
    ) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(client, namespace);
        
        // Create log params
        let mut log_params = LogParams::default();
        log_params.follow = follow;
        log_params.tail_lines = Some(tail_lines);
        log_params.timestamps = timestamps;
        log_params.container = Some(container_name.to_string());
        
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
                // Format is typically "2023-03-21T12:34:56.789012345Z "
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
        
        info!("Log stream ended for {}/{}/{}", namespace, pod_name, container_name);
        Ok(())
    }
}