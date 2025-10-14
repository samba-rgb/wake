use anyhow::{Result, Context};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};

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

pub struct WebOutput {
    client: Client,
    endpoint: String,
    stream_name: String,
    batch_size: usize,
    timeout_duration: Duration,
    retry_attempts: u32,
    retry_delay: Duration,
    current_batch: Vec<WebLogEntry>,
    base_url: String,
}

impl WebOutput {
    pub fn new(endpoint: String, batch_size: usize, timeout_seconds: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        // Extract base URL from endpoint
        let base_url = if let Some(pos) = endpoint.find("/api/") {
            endpoint[..pos].to_string()
        } else {
            "http://localhost:5080".to_string()
        };

        // Extract stream name from endpoint
        let stream_name = if let Some(api_pos) = endpoint.find("/api/default/") {
            let after_api = &endpoint[api_pos + 13..]; // "/api/default/".len() = 13
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
        
        // Show OpenObserve UI redirect information
        println!("🌐 Web mode started - sending logs to OpenObserve");
        println!("📊 Access OpenObserve dashboard at: {base_url}");
        println!("🔐 Login credentials: root@example.com / Complexpass#123");
        println!("📝 Stream name: {stream_name}");
        println!("🔗 Full endpoint: {endpoint}");
        println!();

        Ok(Self {
            client,
            endpoint,
            stream_name,
            batch_size,
            timeout_duration: Duration::from_secs(timeout_seconds),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(1000),
            current_batch: Vec::new(),
            base_url,
        })
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
                        warn!("⚠️  Attempt {}/{} failed, retrying in {:?}", attempt, self.retry_attempts, delay);
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
        debug!("📡 POST {} (payload size: {} bytes) [stream: {}]", self.endpoint, json_payload.len(), self.stream_name);

        let response = timeout(
            self.timeout_duration,
            self.client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .basic_auth("root@example.com", Some("Complexpass#123"))
                .body(json_payload.to_string())
                .send()
        ).await
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
        WebLogEntry {
            timestamp: entry.timestamp
                .map(|ts| ts.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            level: extract_log_level(&entry.message)
                .unwrap_or_else(|| "info".to_string()),
            message: entry.message.clone(),
            service: extract_service_name(&entry.pod_name),
            pod_name: entry.pod_name.clone(),
            namespace: entry.namespace.clone(),
            container: entry.container_name.clone(),
        }
    }
}

#[async_trait]
impl LogOutput for WebOutput {
    async fn send_log(&mut self, entry: &LogEntry) -> Result<()> {
        let web_entry = self.convert_log_entry(entry);
        self.current_batch.push(web_entry);

        if self.current_batch.len() >= self.batch_size {
            self.flush().await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        if self.current_batch.is_empty() {
            return Ok(());
        }

        // Send as array directly (matching your curl payload format)
        let json_payload = serde_json::to_string(&self.current_batch)
            .context("Failed to serialize logs to JSON")?;

        debug!("📤 Sending {} log entries to {}", self.current_batch.len(), self.endpoint);

        match self.send_logs_with_retry(&json_payload).await {
            Ok(_) => {
                info!("✅ Successfully sent {} log entries", self.current_batch.len());
                self.current_batch.clear();
            }
            Err(e) => {
                error!("❌ Failed to send logs after retries: {}", e);
                self.current_batch.clear();
                return Err(e);
            }
        }

        Ok(())
    }

    fn output_type(&self) -> &'static str {
        "web"
    }
}

fn extract_log_level(message: &str) -> Option<String> {
    let message_upper = message.to_uppercase();
    
    if message_upper.contains("ERROR") || message_upper.contains("ERR") {
        Some("error".to_string())
    } else if message_upper.contains("WARN") || message_upper.contains("WARNING") {
        Some("warn".to_string())
    } else if message_upper.contains("INFO") {
        Some("info".to_string())
    } else if message_upper.contains("DEBUG") {
        Some("debug".to_string())
    } else if message_upper.contains("TRACE") {
        Some("trace".to_string())
    } else {
        None
    }
}

fn extract_service_name(pod_name: &str) -> String {
    // Extract service name from pod name by removing deployment suffix
    // e.g., "web-api-deployment-7d4b8c9f6-x8k2m" -> "web-api"
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