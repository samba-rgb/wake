use crate::k8s::logs::LogEntry;
use anyhow::{Result, anyhow};
use regex::Regex;
use std::sync::Arc;
use chrono::DateTime;

/// Trait for formatting log entries
pub trait OutputFormatter: Send + Sync {
    /// Format a log entry according to the formatter's rules
    fn format(&self, entry: &LogEntry) -> Result<String>;

    /// Get the name of the formatter
    fn format_name(&self) -> Option<String> {
        None
    }
}

/// Text formatter for human-readable output
pub struct TextFormatter {
    show_timestamps: bool,
}

impl TextFormatter {
    /// Creates a new text formatter
    pub fn new(show_timestamps: bool) -> Self {
        Self { show_timestamps }
    }
}

impl OutputFormatter for TextFormatter {
    fn format(&self, entry: &LogEntry) -> Result<String> {
        let mut output = String::new();
        
        // Add timestamp if enabled
        if self.show_timestamps {
            if let Some(ts) = &entry.timestamp {
                output.push_str(&format!("{} ", ts.format("%Y-%m-%d %H:%M:%S")));
            }
        }
        
        // Add pod/container context and message
        output.push_str(&format!("[{}/{}/{}] {}", 
            entry.namespace,
            entry.pod_name,
            entry.container_name,
            entry.message
        ));
        
        Ok(output)
    }
    
    fn format_name(&self) -> Option<String> {
        Some("text".to_string())
    }
}

/// Formats logs as JSON
pub struct JsonFormatter {}

/// Formats logs in raw format (just the message)
pub struct RawFormatter {}

impl JsonFormatter {
    /// Creates a new JSON formatter
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputFormatter for JsonFormatter {
    fn format(&self, entry: &LogEntry) -> Result<String> {
        let timestamp = entry.timestamp.map(|ts| ts.to_rfc3339());
        
        let json = serde_json::json!({
            "namespace": entry.namespace,
            "pod": entry.pod_name,
            "container": entry.container_name,
            "message": entry.message,
            "timestamp": timestamp,
        });

        serde_json::to_string(&json).map_err(|e| anyhow!("Failed to serialize to JSON: {}", e))
    }
    
    fn format_name(&self) -> Option<String> {
        Some("json".to_string())
    }
}

impl RawFormatter {
    /// Creates a new raw formatter
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputFormatter for RawFormatter {
    fn format(&self, entry: &LogEntry) -> Result<String> {
        Ok(entry.message.clone())
    }
    
    fn format_name(&self) -> Option<String> {
        Some("raw".to_string())
    }
}

/// Create a formatter based on the output format string
pub fn create_formatter(format: &str, show_timestamps: bool) -> Result<Box<dyn OutputFormatter + Send + Sync>> {
    match format {
        "text" => Ok(Box::new(TextFormatter::new(show_timestamps))),
        "json" => Ok(Box::new(JsonFormatter::new())),
        "raw" => Ok(Box::new(RawFormatter::new())),
        _ => Err(anyhow!("Unsupported output format: {}", format)),
    }
}