use anyhow::{Result, Context};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};

#[derive(Serialize, Clone, Debug)]
pub struct WebLogEntry {
    pub timestamp: String,           // ISO 8601 format
    pub namespace: String,
    pub pod_name: String,
    pub container_name: String,
    pub message: String,
    pub level: Option<String>,       // Extracted if possible
    pub source: String,              // "kubernetes"
    pub cluster: Option<String>,     // From kube context
    pub metadata: HashMap<String, String>, // Additional fields
}

#[derive(Serialize, Clone, Debug)]
pub struct WebLogBatch {
    pub entries: Vec<WebLogEntry>,
    pub batch_info: BatchInfo,
}

#[derive(Serialize, Clone, Debug)]
pub struct BatchInfo {
    pub size: usize,
    pub timestamp: String,
    pub source: String,
}

#[derive(Debug)]
pub struct WebOutputHandler {
    client: Client,
    endpoint: String,
    batch_size: usize,
    timeout_duration: Duration,
    retry_attempts: u32,
    retry_delay: Duration,
    current_batch: Vec<WebLogEntry>,
}

impl WebOutputHandler {
    pub fn new(
        endpoint: String,
        batch_size: usize,
        timeout_seconds: u64,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        info!("🌐 Web output handler initialized");
        info!("   Endpoint: {}", endpoint);
        info!("   Batch size: {}", batch_size);
        info!("   Timeout: {}s", timeout_seconds);

        Ok(Self {
            client,
            endpoint,
            batch_size,
            timeout_duration: Duration::from_secs(timeout_seconds),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(1000),
            current_batch: Vec::new(),
        })
    }

    pub async fn send_log(&mut self, entry: WebLogEntry) -> Result<()> {
        self.current_batch.push(entry);

        if self.current_batch.len() >= self.batch_size {
            self.flush_batch().await?;
        }

        Ok(())
    }

    pub async fn flush_batch(&mut self) -> Result<()> {
        if self.current_batch.is_empty() {
            return Ok(());
        }

        let batch = WebLogBatch {
            entries: self.current_batch.clone(),
            batch_info: BatchInfo {
                size: self.current_batch.len(),
                timestamp: Utc::now().to_rfc3339(),
                source: "wake".to_string(),
            },
        };

        debug!("📤 Sending batch of {} log entries to {}", batch.batch_info.size, self.endpoint);

        match self.send_batch_with_retry(&batch).await {
            Ok(_) => {
                info!("✅ Successfully sent {} log entries", batch.batch_info.size);
                self.current_batch.clear();
            }
            Err(e) => {
                error!("❌ Failed to send batch after retries: {}", e);
                // Clear the batch to prevent memory buildup on persistent failures
                self.current_batch.clear();
                return Err(e);
            }
        }

        Ok(())
    }

    async fn send_batch_with_retry(&self, batch: &WebLogBatch) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=self.retry_attempts {
            match self.send_batch_once(batch).await {
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

    async fn send_batch_once(&self, batch: &WebLogBatch) -> Result<()> {
        let json_payload = serde_json::to_string(batch)
            .context("Failed to serialize batch to JSON")?;

        debug!("📡 POST {} (payload size: {} bytes)", self.endpoint, json_payload.len());

        let response = timeout(
            self.timeout_duration,
            self.client
                .post(&self.endpoint)
                .header("Content-Type", "application/json")
                .body(json_payload)
                .send()
        ).await
        .context("HTTP request timed out")?
        .context("Failed to send HTTP request")?;

        let status = response.status();
        debug!("📨 Response status: {}", status);

        if status.is_success() {
            debug!("✅ HTTP request successful: {}", status);
            Ok(())
        } else if status.is_client_error() {
            // 4xx errors - don't retry
            let error_text = response.text().await.unwrap_or_default();
            error!("🚫 Client error ({}): {}", status, error_text);
            Err(anyhow::anyhow!("Client error {}: {}", status, error_text))
        } else {
            // 5xx errors - retry
            let error_text = response.text().await.unwrap_or_default();
            warn!("🔄 Server error ({}): {}", status, error_text);
            Err(anyhow::anyhow!("Server error {}: {}", status, error_text))
        }
    }

    pub fn create_log_entry(
        timestamp: DateTime<Utc>,
        namespace: String,
        pod_name: String,
        container_name: String,
        message: String,
        cluster: Option<String>,
    ) -> WebLogEntry {
        // Try to extract log level from message
        let level = extract_log_level(&message);

        WebLogEntry {
            timestamp: timestamp.to_rfc3339(),
            namespace,
            pod_name,
            container_name,
            message,
            level,
            source: "kubernetes".to_string(),
            cluster,
            metadata: HashMap::new(),
        }
    }
}

impl Drop for WebOutputHandler {
    fn drop(&mut self) {
        if !self.current_batch.is_empty() {
            warn!("⚠️  Dropping WebOutputHandler with {} unsent log entries", self.current_batch.len());
        }
    }
}

fn extract_log_level(message: &str) -> Option<String> {
    let message_upper = message.to_uppercase();
    
    // Common log level patterns
    if message_upper.contains("ERROR") || message_upper.contains("ERR") {
        Some("ERROR".to_string())
    } else if message_upper.contains("WARN") || message_upper.contains("WARNING") {
        Some("WARN".to_string())
    } else if message_upper.contains("INFO") {
        Some("INFO".to_string())
    } else if message_upper.contains("DEBUG") {
        Some("DEBUG".to_string())
    } else if message_upper.contains("TRACE") {
        Some("TRACE".to_string())
    } else {
        None
    }
}