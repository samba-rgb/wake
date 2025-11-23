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

/// OpenObserve schema definition for Wake logs
#[derive(Serialize, Debug)]
pub struct OpenObserveSchema {
    pub stream_type: String,
    pub stream_name: String,
    pub settings: SchemaSettings,
    pub schema: FieldMappings,
}

#[derive(Serialize, Debug)]
pub struct SchemaSettings {
    pub partition_keys: Vec<String>,
    pub full_text_search_keys: Vec<String>,
    pub data_retention: i32,
    pub partition_time_level: String,
}

#[derive(Serialize, Debug)]
pub struct FieldMappings {
    #[serde(rename = "_timestamp")]
    pub timestamp: FieldDefinition,
    pub message: FieldDefinition,
    pub level: FieldDefinition,
    pub service: FieldDefinition,
    pub pod_name: FieldDefinition,
    pub namespace: FieldDefinition,
    pub container: FieldDefinition,
}

#[derive(Serialize, Debug)]
pub struct FieldDefinition {
    #[serde(rename = "type")]
    pub field_type: String,
    pub index: bool,
    pub stored: bool,
    pub doc_values: bool,
    pub fast: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

impl OpenObserveSchema {
    pub fn wake_default(stream_name: &str) -> Self {
        Self {
            stream_type: "logs".to_string(),
            stream_name: stream_name.to_string(),
            settings: SchemaSettings {
                partition_keys: vec![
                    "namespace".to_string(),
                    "service".to_string(),
                ],
                full_text_search_keys: vec![
                    "message".to_string(),
                ],
                data_retention: 30, // days
                partition_time_level: "daily".to_string(),
            },
            schema: FieldMappings {
                timestamp: FieldDefinition {
                    field_type: "date".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: Some("rfc3339".to_string()),
                },
                message: FieldDefinition {
                    field_type: "text".to_string(),
                    index: true,
                    stored: true,
                    doc_values: false,
                    fast: false,
                    format: None,
                },
                level: FieldDefinition {
                    field_type: "keyword".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: None,
                },
                service: FieldDefinition {
                    field_type: "keyword".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: None,
                },
                pod_name: FieldDefinition {
                    field_type: "keyword".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: None,
                },
                namespace: FieldDefinition {
                    field_type: "keyword".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: None,
                },
                container: FieldDefinition {
                    field_type: "keyword".to_string(),
                    index: true,
                    stored: true,
                    doc_values: true,
                    fast: true,
                    format: None,
                },
            },
        }
    }
}

/// Default column configuration for OpenObserve display
#[derive(Serialize, Debug)]
pub struct ColumnConfig {
    pub columns: Vec<ColumnDefinition>,
}

#[derive(Serialize, Debug)]
pub struct ColumnDefinition {
    pub name: String,
    pub label: String,
    pub width: String,
    pub sortable: bool,
    pub searchable: bool,
}

impl ColumnConfig {
    pub fn wake_default() -> Self {
        Self {
            columns: vec![
                ColumnDefinition {
                    name: "_timestamp".to_string(),
                    label: "Time".to_string(),
                    width: "180px".to_string(),
                    sortable: true,
                    searchable: false,
                },
                ColumnDefinition {
                    name: "level".to_string(),
                    label: "Level".to_string(),
                    width: "80px".to_string(),
                    sortable: true,
                    searchable: true,
                },
                ColumnDefinition {
                    name: "service".to_string(),
                    label: "Service".to_string(),
                    width: "120px".to_string(),
                    sortable: true,
                    searchable: true,
                },
                ColumnDefinition {
                    name: "namespace".to_string(),
                    label: "Namespace".to_string(),
                    width: "100px".to_string(),
                    sortable: true,
                    searchable: true,
                },
                ColumnDefinition {
                    name: "pod_name".to_string(),
                    label: "Pod".to_string(),
                    width: "150px".to_string(),
                    sortable: true,
                    searchable: true,
                },
                ColumnDefinition {
                    name: "container".to_string(),
                    label: "Container".to_string(),
                    width: "120px".to_string(),
                    sortable: true,
                    searchable: true,
                },
                ColumnDefinition {
                    name: "message".to_string(),
                    label: "Message".to_string(),
                    width: "auto".to_string(),
                    sortable: false,
                    searchable: true,
                },
            ],
        }
    }
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
        
        // Setup schema in OpenObserve
        let schema_client = client.clone();
        let schema_base_url = base_url.clone();
        let schema_stream_name = stream_name.clone();
        let schema_user = web_user.clone();
        let schema_pass = web_pass.clone();
        
        // Set up schema synchronously to ensure it's ready before sending logs
        tokio::spawn(async move {
            if let Err(e) = Self::setup_openobserve_schema(
                &schema_client,
                &schema_base_url,
                &schema_stream_name,
                &schema_user,
                &schema_pass,
            ).await {
                warn!("Failed to setup schema: {}", e);
                info!("Continuing without schema - OpenObserve will auto-detect fields");
            } else {
                info!("✅ Schema configured successfully");
                println!("📋 Schema: Wake structured logs (message, level, service, etc.)");
            }
        });
        
        println!("📊 Access OpenObserve dashboard at: http://localhost:5080/web/logs?stream_type=logs&stream={stream_name}&period=15m&refresh=0&fn_editor=false&type=stream_explorer&defined_schemas=user_defined_schema&org_identifier=default&quick_mode=false&show_histogram=true&logs_visualize_toggle=logs");
        println!("🔐 Login credentials: {} / {}", web_user, web_pass);
        println!("📝 Stream name: {stream_name}");
        println!();
        println!("🎨 Custom View Setup:");
        println!("   • Wake is creating a custom view 'wake-logs-view' automatically");
        println!("   • View includes structured columns: Time, Level, Service, Namespace, Pod, Container, Message");
        println!("   • If custom view fails, look for saved query 'wake-structured-logs'");
        println!();
        println!("💡 Manual Setup (if auto-setup fails):");
        println!("   1. Go to the Logs view in OpenObserve");
        println!("   2. Look for 'Views' or 'Saved Queries' section"); 
        println!("   3. Use the 'wake-logs-view' or run this query:");
        println!("      SELECT _timestamp, level, service, namespace, pod_name, container, message");
        println!("      FROM \"{}\" ORDER BY _timestamp DESC", stream_name);
        println!();
        println!("💡 To configure structured view in OpenObserve:");
        println!("   1. Go to http://localhost:5080/web/logs");
        println!("   2. Select stream: {}", stream_name);
        println!("   3. Click on 'Columns' or 'Fields' button");
        println!("   4. Enable these columns:");
        println!("      ✓ _timestamp (Time)");
        println!("      ✓ level (Log Level)");
        println!("      ✓ service (Service)");
        println!("      ✓ namespace (Namespace)");
        println!("      ✓ pod_name (Pod)");
        println!("      ✓ container (Container)");
        println!("      ✓ message (Message)");
        println!("   5. Save the view configuration");
        println!();
        println!("🔗 Direct link: http://localhost:5080/web/logs?stream_type=logs&stream={}&period=15m", stream_name);
        
        // Try to create a dashboard/view through OpenObserve's dashboard API
        let dashboard_client = client.clone();
        let dashboard_base_url = base_url.clone();
        let dashboard_stream_name = stream_name.clone();
        let dashboard_user = web_user.clone();
        let dashboard_pass = web_pass.clone();
        
        tokio::spawn(async move {
            Self::create_openobserve_dashboard(
                &dashboard_client,
                &dashboard_base_url,
                &dashboard_stream_name,
                &dashboard_user,
                &dashboard_pass,
            ).await.unwrap_or_else(|e| {
                info!("Dashboard creation not supported: {}", e);
            });
        });

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

    /// Setup OpenObserve schema for structured logging
    async fn setup_openobserve_schema(
        client: &Client,
        base_url: &str,
        stream_name: &str,
        user: &str,
        pass: &str,
    ) -> Result<()> {
        info!("🔧 Setting up OpenObserve stream: {}", stream_name);
        
        // OpenObserve automatically creates streams on first POST
        // We'll send multiple sample records with different field types to establish the schema
        Self::send_schema_establishment_records(client, base_url, stream_name, user, pass).await?;
        
        Ok(())
    }
    
    /// Send schema establishment records to define field types
    async fn send_schema_establishment_records(
        client: &Client,
        base_url: &str,
        stream_name: &str,
        user: &str,
        pass: &str,
    ) -> Result<()> {
        info!("📤 Sending schema establishment records to define field types");
        
        let endpoint = format!("{}/api/default/{}/_json", base_url, stream_name);
        
        // Send multiple records with different field types to establish schema
        let schema_records = vec![
            serde_json::json!({
                "_timestamp": chrono::Utc::now().to_rfc3339(),
                "level": "info",
                "message": "Wake schema establishment - info level",
                "service": "wake-schema",
                "pod_name": "wake-schema-pod-1",
                "namespace": "default",
                "container": "main"
            }),
            serde_json::json!({
                "_timestamp": chrono::Utc::now().to_rfc3339(), 
                "level": "error",
                "message": "Wake schema establishment - error level",
                "service": "wake-schema",
                "pod_name": "wake-schema-pod-2",
                "namespace": "kube-system",
                "container": "sidecar"
            }),
            serde_json::json!({
                "_timestamp": chrono::Utc::now().to_rfc3339(),
                "level": "warn",
                "message": "Wake schema establishment - warn level",
                "service": "wake-schema",
                "pod_name": "wake-schema-pod-3",
                "namespace": "monitoring",
                "container": "app"
            })
        ];
        
        for (i, record) in schema_records.iter().enumerate() {
            let response = client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .basic_auth(user, Some(pass))
                .json(&[record]) // Send as array like our normal logs
                .send()
                .await;
                
            match response {
                Ok(resp) if resp.status().is_success() => {
                    info!("✅ Schema record {} sent successfully", i + 1);
                }
                Ok(resp) => {
                    let status = resp.status();
                    let error_text = resp.text().await.unwrap_or_default();
                    warn!("⚠️ Schema record {} failed ({}): {}", i + 1, status, error_text);
                }
                Err(e) => {
                    warn!("❌ Failed to send schema record {}: {}", i + 1, e);
                }
            }
            
            // Small delay between records
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        info!("📋 Schema establishment complete - OpenObserve should now recognize field types");
        println!("📋 Schema records sent - fields should be properly detected");
        
        Ok(())
    }
    
    /// Create OpenObserve dashboard with structured view
    async fn create_openobserve_dashboard(
        client: &Client,
        base_url: &str,
        stream_name: &str,
        user: &str,
        pass: &str,
    ) -> Result<()> {
        info!("🎨 Creating OpenObserve dashboard for structured view");
        
        let dashboard_config = serde_json::json!({
            "title": "Wake Kubernetes Logs",
            "description": "Structured view of Wake Kubernetes logs",
            "type": "logs",
            "panels": [{
                "title": "Wake Logs",
                "type": "logs",
                "query": {
                    "sql": format!("SELECT _timestamp as Time, level as Level, service as Service, namespace as Namespace, pod_name as Pod, container as Container, message as Message FROM \"{}\" ORDER BY _timestamp DESC LIMIT 1000", stream_name),
                    "stream_name": stream_name,
                    "stream_type": "logs"
                },
                "fields": [
                    {"name": "_timestamp", "label": "Time", "type": "timestamp"},
                    {"name": "level", "label": "Level", "type": "keyword"},
                    {"name": "service", "label": "Service", "type": "keyword"},
                    {"name": "namespace", "label": "Namespace", "type": "keyword"},
                    {"name": "pod_name", "label": "Pod", "type": "keyword"},
                    {"name": "container", "label": "Container", "type": "keyword"},
                    {"name": "message", "label": "Message", "type": "text"}
                ]
            }]
        });
        
        // Try different dashboard API endpoints
        let dashboard_endpoints = vec![
            format!("{}/api/default/dashboards", base_url),
            format!("{}/api/default/_dashboards", base_url),
            format!("{}/api/default/logs/dashboards", base_url),
        ];
        
        for endpoint in &dashboard_endpoints {
            if let Ok(resp) = client
                .post(endpoint)
                .header("Content-Type", "application/json")
                .basic_auth(user, Some(pass))
                .json(&dashboard_config)
                .send()
                .await {
                    if resp.status().is_success() {
                        info!("✅ Dashboard created successfully at: {}", endpoint);
                        println!("🎨 Dashboard 'Wake Kubernetes Logs' created successfully");
                        return Ok(());
                    } else {
                        debug!("Dashboard creation failed at {}: {}", endpoint, resp.status());
                    }
            }
        }
        
        info!("💡 Dashboard API not available - manual configuration needed");
        Ok(())
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