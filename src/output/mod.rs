pub mod formatter;

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use colored::*;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// Formatter for log entries
pub struct Formatter {
    output_format: OutputFormat,
    include_pattern: Option<Regex>,
    exclude_pattern: Option<Regex>,
    show_timestamps: bool,
    pod_colors: Mutex<HashMap<String, Color>>,
    container_colors: Mutex<HashMap<String, Color>>,
}

/// Different output formats
enum OutputFormat {
    Text,
    Json,
    Raw,
    Template(String),
}

/// Available colors for pods and containers
static COLORS: [Color; 7] = [
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::BrightGreen,
    Color::BrightBlue,
];

impl Formatter {
    /// Creates a new formatter
    pub fn new(args: &Args) -> Self {
        let output_format = match args.output.as_str() {
            "json" => OutputFormat::Json,
            "raw" => OutputFormat::Raw,
            _ => match &args.template {
                Some(template) => OutputFormat::Template(template.clone()),
                None => OutputFormat::Text,
            },
        };

        // Parse regex patterns
        let include_pattern = args.include_regex()
            .map(|r| r.unwrap_or_else(|_| Regex::new(".*").unwrap()));
        let exclude_pattern = args.exclude_regex()
            .map(|r| r.unwrap_or_else(|_| Regex::new(".*").unwrap()));

        Self {
            output_format,
            include_pattern,
            exclude_pattern,
            show_timestamps: args.timestamps,
            pod_colors: Mutex::new(HashMap::new()),
            container_colors: Mutex::new(HashMap::new()),
        }
    }

    /// Formats a log entry based on the selected output format
    pub fn format(&self, entry: &LogEntry) -> Option<String> {
        // Filter logs based on include/exclude patterns
        if let Some(ref pattern) = self.include_pattern {
            if !pattern.is_match(&entry.message) {
                return None;
            }
        }

        if let Some(ref pattern) = self.exclude_pattern {
            if pattern.is_match(&entry.message) {
                return None;
            }
        }

        match &self.output_format {
            OutputFormat::Text => Some(self.format_text(entry)),
            OutputFormat::Json => Some(self.format_json(entry)),
            OutputFormat::Raw => Some(entry.message.clone()),
            OutputFormat::Template(_) => Some(self.format_text(entry)), // Simplified for now
        }
    }

    /// Formats a log entry as colored text
    fn format_text(&self, entry: &LogEntry) -> String {
        let pod_color = self.get_color_for_pod(&entry.pod_name);
        let container_color = self.get_color_for_container(&entry.container_name);

        let pod_part = entry.pod_name.color(pod_color).to_string();
        let container_part = entry.container_name.color(container_color).to_string();

        let time_part = if self.show_timestamps {
            if let Some(ts) = entry.timestamp {
                format!("{} ", ts.format("%Y-%m-%d %H:%M:%S%.3f").to_string().dimmed())
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Check if this is an error log
        let is_error_log = entry.message.to_lowercase().contains("error") || 
                          entry.message.contains("ERROR") ||
                          entry.message.contains("ERR") ||
                          entry.message.contains("Exception") ||
                          entry.message.contains("FATAL");

        // Format the base log entry text
        let log_text = format!("{}{}/{} {}", time_part, pod_part, container_part, entry.message);
        
        // If it's an error log, color the entire log text red
        if is_error_log {
            log_text.red().to_string()
        } else {
            log_text
        }
    }

    /// Formats a log entry as JSON
    fn format_json(&self, entry: &LogEntry) -> String {
        let timestamp = entry.timestamp.map(|ts| ts.to_rfc3339());
        
        let json = serde_json::json!({
            "namespace": entry.namespace,
            "pod": entry.pod_name,
            "container": entry.container_name,
            "message": entry.message,
            "timestamp": timestamp,
        });

        serde_json::to_string(&json).unwrap_or_else(|_| entry.message.clone())
    }

    /// Gets a consistent color for a pod
    fn get_color_for_pod(&self, pod_name: &str) -> Color {
        self.get_or_assign_color(&self.pod_colors, pod_name)
    }

    /// Gets a consistent color for a container
    fn get_color_for_container(&self, container_name: &str) -> Color {
        self.get_or_assign_color(&self.container_colors, container_name)
    }

    /// Gets or assigns a color for a name
    fn get_or_assign_color(&self, map: &Mutex<HashMap<String, Color>>, name: &str) -> Color {
        let mut map = map.lock().unwrap();
        
        if let Some(color) = map.get(name) {
            return *color;
        }
        
        // Assign a new color
        let color = COLORS[map.len() % COLORS.len()];
        map.insert(name.to_string(), color);
        color
    }
}