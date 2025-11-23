use anyhow::{Result, Context};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};
use strip_ansi_escapes;

use crate::k8s::logs::LogEntry;
use super::LogOutput;

#[derive(Serialize, Clone, Debug)]
pub struct WebLogEntry {
    #[serde(rename = "_timestamp")]
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub service: String,
    pub pod_name: String,
    pub namespace: String,
    pub container: String,
}

#[derive(Debug)]
pub struct WebOutput {
    sender: mpsc::Sender<LogEntry>,
    flush_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WebOutput {
    pub fn new(
        endpoint: String,
        batch_size: usize,
        timeout_seconds: u64,
        web_user: String,
        web_pass: String,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(1000); // Channel with a buffer of 1000

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        let base_url = if let Some(pos) = endpoint.find("/api/") {
            endpoint[..pos].to_string()
        } else {
            "http://localhost:5080".to_string()
        };

        let stream_name = if let Some(api_pos) = endpoint.find("/api/default/") {
            let after_api = &endpoint[api_pos + 13..];
            if let Some(json_pos) = after_api.find("/_json") {
                after_api[..json_pos].to_string()
            } else {
                format!("logs_wake_{}", chrono::Local::now().format("%Y_%m_%d"))
            }
        } else {
            format!("logs_wake_{}", chrono::Local::now().format("%Y_%m_%d"))
        };

        info!("🌐 Web output handler initialized");
        info!("   Endpoint: {}", endpoint);
        info!("   Stream: {}", stream_name);
        info!("   Base URL: {}", base_url);
        info!("   Batch size: {}", batch_size);
        info!("   Timeout: {}s", timeout_seconds);

        println!("🌐 Web mode started - sending logs to OpenObserve");
        println!("📊 Access OpenObserve dashboard at: http://localhost:5080/web/logs?stream_type=logs&stream={stream_name}&period=15m&refresh=0&fn_editor=false&type=stream_explorer&defined_schemas=user_defined_schema&org_identifier=default&quick_mode=false&show_histogram=true&logs_visualize_toggle=logs");
        println!("🔐 Login credentials: {} / {}", web_user, web_pass);
        println!("📝 Stream name: {stream_name}");
        //println!("🔗 Full endpoint: {endpoint}");
        println!();

        let mut handler = BatchingWebOutput {
            client,
            endpoint: endpoint.clone(),
            stream_name: stream_name.clone(),
            batch_size,
            timeout_duration: Duration::from_secs(timeout_seconds),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(1000),
            current_batch: Vec::new(),
            receiver,
            web_user,
            web_pass,
        };

        let flush_handle = tokio::spawn(async move {
            handler.run().await;
        });

        Ok(Self {
            sender,
            flush_handle: Some(flush_handle),
        })
    }
}

struct BatchingWebOutput {
    client: Client,
    endpoint: String,
    stream_name: String,
    batch_size: usize,
    timeout_duration: Duration,
    retry_attempts: u32,
    retry_delay: Duration,
    current_batch: Vec<WebLogEntry>,
    receiver: mpsc::Receiver<LogEntry>,
    web_user: String,
    web_pass: String,
}

impl BatchingWebOutput {
    async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                Some(entry) = self.receiver.recv() => {
                    let web_entry = self.convert_log_entry(&entry);
                    self.current_batch.push(web_entry);
                    if self.current_batch.len() >= self.batch_size {
                        if let Err(e) = self.flush().await {
                            error!("Failed to flush logs: {}", e);
                        }
                    }
                }
                _ = interval.tick() => {
                    if !self.current_batch.is_empty() {
                        if let Err(e) = self.flush().await {
                            error!("Failed to flush logs on interval: {}", e);
                        }
                    }
                }
                else => {
                    // Channel closed, flush remaining logs and exit
                    if !self.current_batch.is_empty() {
                        if let Err(e) = self.flush().await {
                            error!("Failed to flush logs on shutdown: {}", e);
                        }
                    }
                    break;
                }
            }
        }
    }

    async fn flush(&mut self) -> Result<()> {
        if self.current_batch.is_empty() {
            return Ok(());
        }

        let json_payload = serde_json::to_string(&self.current_batch)
            .context("Failed to serialize logs to JSON")?;

        debug!(
            "📤 Sending {} log entries to {}",
            self.current_batch.len(),
            self.endpoint
        );

        match self.send_logs_with_retry(&json_payload).await {
            Ok(_) => {
                info!(
                    "✅ Successfully sent {} log entries",
                    self.current_batch.len()
                );
                self.current_batch.clear();
            }
            Err(e) => {
                error!("❌ Failed to send logs after retries: {}", e);
                // Decide on error handling: clear the batch anyway or retry later
                self.current_batch.clear();
                return Err(e);
            }
        }

        Ok(())
    }

    async fn send_logs_with_retry(&self, json_payload: &str) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=self.retry_attempts {
            match self.send_logs_once(json_payload).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_error = Some(e);

                    if attempt < self.retry_attempts {
                        let delay = self.retry_delay * attempt;
                        warn!(
                            "⚠️  Attempt {}/{} failed, retrying in {:?}",
                            attempt, self.retry_attempts, delay
                        );
                        sleep(delay).await;
                    } else {
                        error!("❌ All {} attempts failed", self.retry_attempts);
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    async fn send_logs_once(&self, json_payload: &str) -> Result<()> {
        debug!(
            "📡 POST {} (payload size: {} bytes) [stream: {}]",
            self.endpoint,
            json_payload.len(),
            self.stream_name
        );

        let response = timeout(
            self.timeout_duration,
            self.client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .basic_auth(&self.web_user, Some(&self.web_pass))
                .body(json_payload.to_string())
                .send(),
        )
        .await
        .context("HTTP request timed out")?
        .context("Failed to send HTTP request")?;

        let status = response.status();
        debug!("📨 Response status: {} [stream: {}]", status, self.stream_name);

        if status.is_success() {
            debug!("✅ HTTP request successful: {} [stream: {}]", status, self.stream_name);
            Ok(())
        } else if status.is_client_error() {
            let error_text = response.text().await.unwrap_or_default();
            error!("🚫 Client error ({}) [stream: {}]: {}", status, self.stream_name, error_text);
            Err(anyhow::anyhow!("Client error {}: {}", status, error_text))
        } else {
            let error_text = response.text().await.unwrap_or_default();
            warn!("🔄 Server error ({}) [stream: {}]: {}", status, self.stream_name, error_text);
            Err(anyhow::anyhow!("Server error {}: {}", status, error_text))
        }
    }

    fn convert_log_entry(&self, entry: &LogEntry) -> WebLogEntry {
        let cleaned_message_bytes = strip_ansi_escapes::strip(&entry.message);
        let cleaned_message = String::from_utf8_lossy(&cleaned_message_bytes).to_string();
        
        // Strip timestamp prefix if present since we already have timestamp in separate field
        let message_without_timestamp = strip_timestamp_prefix(&cleaned_message);

        WebLogEntry {
            timestamp: entry.timestamp
                .map(|ts| ts.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            level: extract_log_level(&message_without_timestamp).unwrap_or_else(|| "info".to_string()),
            message: message_without_timestamp,
            service: extract_service_name(&entry.pod_name),
            pod_name: entry.pod_name.clone(),
            namespace: entry.namespace.clone(),
            container: entry.container_name.clone(),
        }
    }
}

/// Strip timestamp prefix from log message if present
/// Handles formats like: "[2023-11-23T10:30:45Z] message" -> "message"
/// Also handles: "2023-11-23T10:30:45.123Z message" -> "message"
fn strip_timestamp_prefix(message: &str) -> String {
    // Pattern 1: [timestamp] format
    if let Some(bracket_end) = message.find("] ") {
        if message.starts_with('[') {
            return message[bracket_end + 2..].to_string();
        }
    }
    
    // Pattern 2: ISO timestamp at start of line
    if let Some(space_pos) = message.find(' ') {
        let potential_timestamp = &message[..space_pos];
        // Check if it looks like an ISO timestamp (contains 'T' and either 'Z' or '+'/'-')
        if potential_timestamp.contains('T') && 
           (potential_timestamp.contains('Z') || 
            potential_timestamp.contains('+') || 
            potential_timestamp.contains('-')) {
            return message[space_pos + 1..].to_string();
        }
    }
    
    // Pattern 3: Simple timestamp format like "2023-11-23 10:30:45 message"
    // Look for date-like pattern at start
    if message.len() > 19 {
        let potential_date = &message[..19];
        if potential_date.matches('-').count() >= 2 && 
           potential_date.matches(':').count() >= 2 {
            return message[19..].trim_start().to_string();
        }
    }
    
    // If no timestamp pattern found, return original message
    message.to_string()
}

#[async_trait]
impl LogOutput for WebOutput {
    async fn send_log(&mut self, entry: &LogEntry) -> Result<()> {
        self.sender
            .send(entry.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send log to web output channel: {}", e))
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn output_type(&self) -> &'static str {
        "web"
    }
}

impl Drop for WebOutput {
    fn drop(&mut self) {
        if let Some(handle) = self.flush_handle.take() {
            info!("Shutting down web output. Waiting for final log flush...");
            drop(self.sender.clone());
            let _ = tokio::runtime::Handle::current().block_on(handle);
            info!("Web output shutdown complete.");
        }
    }
}

fn extract_log_level(message: &str) -> Option<String> {
    if message.to_lowercase().contains("error") {
        Some("error".to_string())
    } else if message.to_lowercase().contains("warn") || message.to_lowercase().contains("warning") {
        Some("warn".to_string())
    } else if message.to_lowercase().contains("info") {
        Some("info".to_string())
    } else if message.to_lowercase().contains("debug") {
        Some("debug".to_string())
    } else if message.to_lowercase().contains("trace") {
        Some("trace".to_string())
    } else {
        None
    }
}

fn extract_service_name(pod_name: &str) -> String {
    if let Some(deployment_pos) = pod_name.find("-deployment-") {
        pod_name[..deployment_pos].to_string()
    } else if let Some(dash_pos) = pod_name.rfind('-') {
        if let Some(second_dash_pos) = pod_name[..dash_pos].rfind('-') {
            pod_name[..second_dash_pos].to_string()
        } else {
            pod_name.to_string()
        }
    } else {
        pod_name.to_string()
    }
}