pub mod formatter;

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use colored::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// Formatter for log entries
pub struct Formatter {
    output_format: OutputFormat,
    show_timestamps: bool,
    pod_colors: Mutex<HashMap<String, Color>>,
    container_colors: Mutex<HashMap<String, Color>>,
    colors_enabled: bool,
}

/// Different output formats
enum OutputFormat {
    Text,
    Json,
    Raw,
}

/// Available colors for pods and containers - using more compatible colors
static COLORS: [Color; 8] = [
    Color::BrightGreen,
    Color::BrightYellow, 
    Color::BrightBlue,
    Color::BrightMagenta,
    Color::BrightCyan,
    Color::Green,
    Color::Yellow,
    Color::Blue,
];

impl Formatter {
    /// Creates a new formatter
    pub fn new(args: &Args) -> Self {
        let output_format = match args.output.as_str() {
            "json" => OutputFormat::Json,
            "raw" => OutputFormat::Raw,
            _ => OutputFormat::Text,
        };

        // Enhanced color detection
        let colors_enabled = Self::detect_color_support();
        
        if colors_enabled {
            colored::control::set_override(true);
        } else {
            colored::control::set_override(false);
        }

        Self {
            output_format,
            show_timestamps: args.timestamps,
            pod_colors: Mutex::new(HashMap::new()),
            container_colors: Mutex::new(HashMap::new()),
            colors_enabled,
        }
    }

    /// Detect if the terminal supports colors
    fn detect_color_support() -> bool {
        // Check for explicit color control
        if let Ok(no_color) = std::env::var("NO_COLOR") {
            if !no_color.is_empty() {
                return false;
            }
        }

        // Check for force color
        if let Ok(force_color) = std::env::var("FORCE_COLOR") {
            if !force_color.is_empty() && force_color != "0" {
                return true;
            }
        }

        // Check terminal capabilities
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("color") || term.contains("256") || term.contains("truecolor") {
                return true;
            }
            if term == "dumb" || term.is_empty() {
                return false;
            }
        }

        // Check if we're in a known good terminal
        if std::env::var("COLORTERM").is_ok() {
            return true;
        }

        // Check if stdout is a terminal
        #[cfg(unix)]
        {
            unsafe {
                libc::isatty(libc::STDOUT_FILENO) != 0
            }
        }
        #[cfg(not(unix))]
        {
            true // Default to true on non-Unix systems
        }
    }

    /// Formats a log entry based on the selected output format
    /// This method no longer includes filtering - filtering is handled by the filter manager
    #[allow(dead_code)]
    pub fn format(&self, entry: &LogEntry) -> Option<String> {
        self.format_without_filtering(entry)
    }
    
    /// Formats a log entry without applying filtering
    /// This is used by the new threaded filtering architecture
    pub fn format_without_filtering(&self, entry: &LogEntry) -> Option<String> {
        match &self.output_format {
            OutputFormat::Text => Some(self.format_text(entry)),
            OutputFormat::Json => Some(self.format_json(entry)),
            OutputFormat::Raw => Some(entry.message.clone()),
        }
    }

    /// Formats a log entry as colored text
    fn format_text(&self, entry: &LogEntry) -> String {
        if !self.colors_enabled {
            // Plain text format without colors
            let time_part = if self.show_timestamps {
                if let Some(ts) = entry.timestamp {
                    format!("{} ", ts.format("%Y-%m-%d %H:%M:%S%.3f"))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            
            return format!("{}{}/{} {}", time_part, entry.pod_name, entry.container_name, entry.message);
        }

        let pod_color = self.get_color_for_pod(&entry.pod_name);
        let container_color = self.get_color_for_container(&entry.container_name);

        let pod_part = entry.pod_name.color(pod_color).bold().to_string();
        let container_part = entry.container_name.color(container_color).to_string();

        let time_part = if self.show_timestamps {
            if let Some(ts) = entry.timestamp {
                format!("{} ", ts.format("%Y-%m-%d %H:%M:%S%.3f").to_string().bright_black())
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Enhanced color coding for log levels
        let message_with_level_color = if entry.message.contains("FATAL") || entry.message.contains("CRITICAL") {
            entry.message.bright_red().bold().to_string()
        } else if entry.message.contains("ERROR") || entry.message.contains("ERR") {
            entry.message.bright_red().to_string()
        } else if entry.message.contains("WARN") || entry.message.contains("WARNING") {
            entry.message.bright_yellow().to_string()
        } else if entry.message.contains("INFO") {
            entry.message.bright_white().to_string()
        } else if entry.message.contains("DEBUG") || entry.message.contains("TRACE") {
            entry.message.bright_cyan().to_string()
        } else {
            // Default color - use bright white for better visibility
            entry.message.bright_white().to_string()
        };

        // Format the complete log entry
        format!("{}{}/{} {}", time_part, pod_part, container_part, message_with_level_color)
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