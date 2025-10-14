use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use anyhow::{Result, Context, anyhow};
use directories::ProjectDirs;
use std::collections::VecDeque;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub autosave: AutosaveConfig,
    pub ui: UiConfig,
    pub history: HistoryConfig,
    pub web: WebConfig,
    // Args defaultable fields
    pub pod_selector: Option<String>,
    pub container: Option<String>,
    pub namespace: Option<String>,
    pub tail: Option<i64>,
    pub follow: Option<bool>,
    pub output: Option<String>,
    pub buffer_size: Option<usize>,
    // Add more as needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutosaveConfig {
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub buffer_expansion: f64, // Multiplier for buffer expansion in pause mode (e.g., 10.0 for 10x)
    pub theme: String, // UI theme (dark, light, auto)
    pub show_timestamps: bool, // Default timestamp display
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistoryEntry {
    pub command: String,
    pub timestamp: DateTime<Utc>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    pub enabled: bool,
    pub max_entries: usize,
    pub commands: VecDeque<CommandHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub endpoint: Option<String>,
    pub batch_size: usize,
    pub timeout_seconds: u64,
}

impl Default for AutosaveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: None,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            buffer_expansion: 10.0, // Default to 10x expansion
            theme: "auto".to_string(), // Auto-detect theme
            show_timestamps: false, // Don't show timestamps by default
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 150,
            commands: VecDeque::new(),
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            endpoint: Some("http://localhost:5080".to_string()),
            batch_size: 10,
            timeout_seconds: 30,
        }
    }
}

impl Config {
    /// Get the configuration file path
    pub fn config_file_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "wake", "wake")
            .context("Unable to determine project directories")?;
        
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)
            .context("Failed to create config directory")?;
        
        Ok(config_dir.join("config.toml"))
    }
    
    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file_path()?;
        
        if (!config_path.exists()) {
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        match toml::from_str::<Config>(&content) {
            Ok(config) => Ok(config),
            Err(e) => {
                eprintln!("⚠️  Warning: Failed to parse config file ({}). Using defaults.", e);
                Ok(Self::default())
            }
        }
    }
    
    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_file_path()?;
        
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&config_path, content)
            .context("Failed to write config file")?;
        
        println!("Configuration saved to: {}", config_path.display());
        Ok(())
    }
    
    /// Save configuration to file silently (without printing message)
    pub fn save_silent(&self) -> Result<()> {
        let config_path = Self::config_file_path()?;
        
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&config_path, content)
            .context("Failed to write config file")?;
        
        Ok(())
    }
    
    /// Set autosave configuration
    pub fn set_autosave(&mut self, enabled: bool, path: Option<String>) {
        self.autosave.enabled = enabled;
        self.autosave.path = path;
    }
    
    /// Get autosave file path (either configured path or generate timestamp-based path)
    #[allow(dead_code)]
    pub fn get_autosave_path(&self, write_file: Option<&str>) -> Option<String> {
        if (!self.autosave.enabled) {
            return None;
        }
        
        // Priority: -w flag > configured path > auto-generated path
        if let Some(write_path) = write_file {
            Some(write_path.to_string())
        } else if let Some(ref configured_path) = self.autosave.path {
            Some(configured_path.clone())
        } else {
            // Generate timestamp-based filename
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            Some(format!("wake_{}.log", timestamp))
        }
    }
    
    /// Automatically set any configuration value using dot notation
    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "autosave.enabled" => {
                self.autosave.enabled = match value.to_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" | "enable" | "enabled" => true,
                    "false" | "0" | "no" | "off" | "disable" | "disabled" => false,
                    _ => return Err(anyhow!("Invalid boolean value: '{}'. Use 'true' or 'false'", value)),
                };
            }
            "autosave.path" => {
                self.autosave.path = if value.is_empty() || value == "<auto-generated>" {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            "ui.buffer_expansion" => {
                let expansion = value.parse::<f64>()
                    .map_err(|_| anyhow!("Invalid buffer expansion value: '{}'. Must be a number (e.g., 10, 5.5)", value))?;
                
                if (expansion < 1.0) {
                    return Err(anyhow!("Buffer expansion must be at least 1.0x (got: {})", expansion));
                }
                
                if (expansion > 50.0) {
                    return Err(anyhow!("Buffer expansion too large: {}x (maximum: 50x)", expansion));
                }
                
                self.ui.buffer_expansion = expansion;
            }
            "ui.theme" => {
                match value.to_lowercase().as_str() {
                    "dark" | "light" | "auto" => {
                        self.ui.theme = value.to_lowercase();
                    }
                    _ => return Err(anyhow!("Invalid theme: '{}'. Valid themes: dark, light, auto", value)),
                }
            }
            "ui.show_timestamps" => {
                self.ui.show_timestamps = match value.to_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" | "enable" | "enabled" => true,
                    "false" | "0" | "no" | "off" | "disable" | "disabled" => false,
                    _ => return Err(anyhow!("Invalid boolean value: '{}'. Use 'true' or 'false'", value)),
                };
            }
            "pod_selector" => {
                self.pod_selector = Some(value.to_string());
            }
            "container" => {
                self.container = Some(value.to_string());
            }
            "namespace" => {
                self.namespace = Some(value.to_string());
            }
            "tail" => {
                self.tail = Some(value.parse::<i64>().map_err(|_| anyhow!("Invalid tail value: '{}'. Must be an integer.", value))?);
            }
            "follow" => {
                self.follow = Some(match value.to_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" | "enable" | "enabled" => true,
                    "false" | "0" | "no" | "off" | "disable" | "disabled" => false,
                    _ => return Err(anyhow!("Invalid boolean value: '{}'. Use 'true' or 'false'", value)),
                });
            }
            "output" => {
                self.output = Some(value.to_string());
            }
            "buffer_size" => {
                self.buffer_size = Some(value.parse::<usize>().map_err(|_| anyhow!("Invalid buffer_size value: '{}'. Must be an integer.", value))?);
            }
            "web.endpoint" => {
                self.web.endpoint = if value.is_empty() || value == "<not-set>" || value == "reset" {
                    // Reset to default
                    Some("http://localhost:5080".to_string())
                } else {
                    // Validate URL format
                    if !value.starts_with("http://") && !value.starts_with("https://") {
                        return Err(anyhow!("Invalid web endpoint: '{}'. Must be a valid HTTP/HTTPS URL", value));
                    }
                    Some(value.to_string())
                };
            }
            "web.batch_size" => {
                let batch_size = value.parse::<usize>()
                    .map_err(|_| anyhow!("Invalid web batch size: '{}'. Must be a positive integer", value))?;
                
                if (batch_size == 0) {
                    return Err(anyhow!("Web batch size must be at least 1 (got: {})", batch_size));
                }
                
                if (batch_size > 1000) {
                    return Err(anyhow!("Web batch size too large: {} (maximum: 1000)", batch_size));
                }
                
                self.web.batch_size = batch_size;
            }
            "web.timeout_seconds" => {
                let timeout = value.parse::<u64>()
                    .map_err(|_| anyhow!("Invalid web timeout: '{}'. Must be a positive integer (seconds)", value))?;
                
                if (timeout == 0) {
                    return Err(anyhow!("Web timeout must be at least 1 second (got: {})", timeout));
                }
                
                if (timeout > 300) {
                    return Err(anyhow!("Web timeout too large: {}s (maximum: 300s)", timeout));
                }
                
                self.web.timeout_seconds = timeout;
            }
            _ => return Err(anyhow!("Unknown configuration key: '{}'. Available keys: autosave.enabled, autosave.path, ui.buffer_expansion, ui.theme, ui.show_timestamps, web.endpoint, web.batch_size, web.timeout_seconds", key))
        }
        
        Ok(())
    }
    
    /// Get all available configuration keys automatically
    pub fn get_all_keys(&self) -> Vec<String> {
        // Always include all possible keys, regardless of whether they are set
        vec![
            "autosave.enabled".to_string(),
            "autosave.path".to_string(),
            "ui.buffer_expansion".to_string(),
            "ui.theme".to_string(),
            "ui.show_timestamps".to_string(),
            "pod_selector".to_string(),
            "container".to_string(),
            "namespace".to_string(),
            "tail".to_string(),
            "follow".to_string(),
            "output".to_string(),
            "buffer_size".to_string(),
            "web.endpoint".to_string(),
            "web.batch_size".to_string(),
            "web.timeout_seconds".to_string(),
            // Add new config keys here as needed
        ]
    }
    
    /// Get any configuration value using dot notation
    pub fn get_value(&self, key: &str) -> Result<String> {
        match key {
            "autosave.enabled" => Ok(self.autosave.enabled.to_string()),
            "autosave.path" => {
                if let Some(ref path) = self.autosave.path {
                    Ok(path.clone())
                } else {
                    Ok("<auto-generated>".to_string())
                }
            }
            "ui.buffer_expansion" => Ok(self.ui.buffer_expansion.to_string()),
            "ui.theme" => Ok(self.ui.theme.clone()),
            "ui.show_timestamps" => Ok(self.ui.show_timestamps.to_string()),
            "pod_selector" => {
                if let Some(pod) = self.pod_selector.clone() {
                    Ok(pod)
                } else {
                    Ok(".*".to_string())
                }
            }
            "container" => {
                if let Some(container) = self.container.clone() {
                    Ok(container)
                } else {
                    Ok(".*".to_string())
                }
            }
            "namespace" => {
                if let Some(ns) = self.namespace.clone() {
                    Ok(ns)
                } else {
                    // Try kube context
                    if let Some(ctx_ns) = crate::k8s::client::get_current_context_namespace() {
                        Ok(ctx_ns)
                    } else {
                        Ok("default".to_string())
                    }
                }
            }
            "tail" => Ok(self.tail.unwrap_or(10).to_string()),
            "follow" => Ok(self.follow.unwrap_or(true).to_string()),
            "output" => Ok(self.output.clone().unwrap_or_else(|| "text".to_string())),
            "buffer_size" => Ok(self.buffer_size.unwrap_or(20000).to_string()),
            "web.endpoint" => {
                if let Some(ref endpoint) = self.web.endpoint {
                    Ok(endpoint.clone())
                } else {
                    Ok("http://localhost:5080".to_string())
                }
            }
            "web.batch_size" => Ok(self.web.batch_size.to_string()),
            "web.timeout_seconds" => Ok(self.web.timeout_seconds.to_string()),
            _ => Err(anyhow!("Configuration key not found: {}", key))
        }
    }
    
    /// Display current configuration in tabular format
    pub fn display(&self) -> String {
        let mut output = String::new();
        
        // Header
        output.push_str("┌─────────────────────────────────────────────────────────────────────┐\n");
        output.push_str("│                           Wake Configuration                        │\n");
        output.push_str("├─────────────────────────┬───────────────────────────────────────────┤\n");
        output.push_str("│ Setting                 │ Value                                     │\n");
        output.push_str("├─────────────────────────┼───────────────────────────────────────────┤\n");
        
        // Get all keys and display them
        let keys = self.get_all_keys();
        for key in keys {
            if let Ok(value) = self.get_value(&key) {
                let display_value = if value.len() > 37 {
                    format!("{}...", &value[..34])
                } else {
                    value
                };
                output.push_str(&format!("│ {:<23} │ {:<41} │\n", key, display_value));
            }
        }
        
        // Footer
        output.push_str("└─────────────────────────┴───────────────────────────────────────────┘\n");
        
        // Configuration file location
        if let Ok(config_path) = Self::config_file_path() {
            let path_str = config_path.display().to_string();
            let path_display = if path_str.len() > 65 {
                format!("...{}", &path_str[path_str.len()-62..])
            } else {
                path_str
            };
            output.push_str(&format!("\nConfig file: {}\n", path_display));
        }
        
        output
    }
    
    /// Display specific configuration key in tabular format
    pub fn display_key(&self, key: &str) -> Result<String> {
        let mut output = String::new();
        
        // Check if it's a section (contains multiple sub-keys)
        let all_keys = self.get_all_keys();
        let matching_keys: Vec<String> = all_keys.into_iter()
            .filter(|k| k.starts_with(key) && (k == key || k.starts_with(&format!("{}.", key))))
            .collect();
        
        if (matching_keys.is_empty()) {
            return Err(anyhow!("Configuration key not found: {}", key));
        }
        
        // Header
        output.push_str("┌─────────────────────────┬───────────────────────────────────────────┐\n");
        if (matching_keys.len() == 1 && matching_keys[0] == key) {
            output.push_str("│ Setting                 │ Value                                     │\n");
        } else {
            output.push_str(&format!("│ {:<23} │ Value                                     │\n", 
                if key.is_empty() { "Configuration" } else { key }));
        }
        output.push_str("├─────────────────────────┼───────────────────────────────────────────┤\n");
        
        // Display matching keys
        for matching_key in matching_keys {
            if let Ok(value) = self.get_value(&matching_key) {
                let display_key = if matching_key == key {
                    key.to_string()
                } else if matching_key.starts_with(&format!("{}.", key)) {
                    matching_key[key.len()+1..].to_string() // Remove prefix and dot
                } else {
                    matching_key
                };
                
                let display_value = if value.len() > 37 {
                    format!("{}...", &value[..34])
                } else {
                    value
                };
                output.push_str(&format!("│ {:<23} │ {:<41} │\n", display_key, display_value));
            }
        }
        
        output.push_str("└─────────────────────────┴───────────────────────────────────────────┘\n");
        Ok(output)
    }
    
    /// Add a command to history
    pub fn add_command_to_history(&mut self, command: String) {
        if (!self.history.enabled) {
            return;
        }
        
        let entry = CommandHistoryEntry {
            command,
            timestamp: Utc::now(),
            working_directory: std::env::current_dir().ok().map(|p| p.display().to_string()),
        };
        
        self.history.commands.push_back(entry);
        
        // Maintain max entries limit
        while (self.history.commands.len() > self.history.max_entries) {
            self.history.commands.pop_front();
        }
    }
    
    /// Get command history (oldest first, newest last)
    pub fn get_command_history(&self) -> Vec<&CommandHistoryEntry> {
        self.history.commands.iter().collect()
    }
    
    /// Get command history count
    pub fn get_history_count(&self) -> usize {
        self.history.commands.len()
    }
}