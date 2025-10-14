use crate::k8s::logs::LogEntry;
use anyhow::{Result, anyhow};

/// Normalize multiline log messages into single lines
/// Replaces newlines with spaces and collapses multiple whitespaces
fn normalize_message(message: &str) -> String {
    // Replace all types of newlines and line separators with spaces
    let normalized = message
        .replace(['\n', '\r', '\t'], " ");
    
    // Collapse multiple consecutive spaces into single spaces
    let mut result = String::new();
    let mut prev_was_space = false;
    
    for ch in normalized.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    
    // Trim leading and trailing whitespace
    result.trim().to_string()
}

/// Trait for formatting log entries
pub trait OutputFormatter: Send + Sync {
    /// Format a log entry according to the formatter's rules
    #[allow(dead_code)]
    fn format(&self, entry: &LogEntry) -> Result<String>;

    /// Get the name of the formatter
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        
        // Normalize the message to convert multiline logs to single lines
        let normalized_message = normalize_message(&entry.message);
        
        // Add pod/container context and message
        output.push_str(&format!("[{}/{}/{}] {}", 
            entry.namespace,
            entry.pod_name,
            entry.container_name,
            normalized_message
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

impl Default for JsonFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonFormatter {
    /// Creates a new JSON formatter
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputFormatter for JsonFormatter {
    fn format(&self, entry: &LogEntry) -> Result<String> {
        let timestamp = entry.timestamp.map(|ts| ts.to_rfc3339());
        
        // Normalize the message to convert multiline logs to single lines
        let normalized_message = normalize_message(&entry.message);
        
        let json = serde_json::json!({
            "namespace": entry.namespace,
            "pod": entry.pod_name,
            "container": entry.container_name,
            "message": normalized_message,
            "timestamp": timestamp,
        });

        serde_json::to_string(&json).map_err(|e| anyhow!("Failed to serialize to JSON: {}", e))
    }
    
    fn format_name(&self) -> Option<String> {
        Some("json".to_string())
    }
}

impl Default for RawFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl RawFormatter {
    /// Creates a new raw formatter
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputFormatter for RawFormatter {
    fn format(&self, entry: &LogEntry) -> Result<String> {
        // Normalize the message to convert multiline logs to single lines
        Ok(normalize_message(&entry.message))
    }
    
    fn format_name(&self) -> Option<String> {
        Some("raw".to_string())
    }
}

/// Create a formatter based on the output format string
#[allow(dead_code)]
pub fn create_formatter(format: &str, show_timestamps: bool) -> Result<Box<dyn OutputFormatter + Send + Sync>> {
    match format {
        "text" => Ok(Box::new(TextFormatter::new(show_timestamps))),
        "json" => Ok(Box::new(JsonFormatter::new())),
        "raw" => Ok(Box::new(RawFormatter::new())),
        _ => Err(anyhow!("Unsupported output format: {}", format)),
    }
}