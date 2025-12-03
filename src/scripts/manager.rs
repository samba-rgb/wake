//! Script Manager - handles storage, retrieval, and management of user scripts

use anyhow::{Result, Context, anyhow};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use directories::ProjectDirs;
use chrono::{DateTime, Utc};

/// A script argument definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptArg {
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub required: bool,
}

/// A saved script with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub arguments: Vec<ScriptArg>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Script {
    /// Create a new script
    pub fn new(name: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            description: None,
            content,
            arguments: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add an argument to the script
    pub fn add_argument(&mut self, arg: ScriptArg) {
        self.arguments.push(arg);
        self.updated_at = Utc::now();
    }

    /// Update the script content
    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.updated_at = Utc::now();
    }
}

/// Script Manager - handles all script CRUD operations
pub struct ScriptManager {
    scripts_dir: PathBuf,
}

impl ScriptManager {
    /// Create a new ScriptManager
    pub fn new() -> Result<Self> {
        let proj_dirs = ProjectDirs::from("com", "wake", "wake")
            .context("Unable to determine project directories")?;
        
        let scripts_dir = proj_dirs.config_dir().join("scripts");
        fs::create_dir_all(&scripts_dir)
            .context("Failed to create scripts directory")?;
        
        Ok(Self { scripts_dir })
    }

    /// Get the path for a script file
    fn script_path(&self, name: &str) -> PathBuf {
        self.scripts_dir.join(format!("{}.toml", name))
    }

    /// Save a script
    pub fn save(&self, script: &Script) -> Result<()> {
        let path = self.script_path(&script.name);
        let content = toml::to_string_pretty(script)
            .context("Failed to serialize script")?;
        fs::write(&path, content)
            .context("Failed to write script file")?;
        Ok(())
    }

    /// Load a script by name
    pub fn load(&self, name: &str) -> Result<Script> {
        let path = self.script_path(name);
        if !path.exists() {
            return Err(anyhow!("Script '{}' not found", name));
        }
        let content = fs::read_to_string(&path)
            .context("Failed to read script file")?;
        let script: Script = toml::from_str(&content)
            .context("Failed to parse script file")?;
        Ok(script)
    }

    /// Delete a script
    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.script_path(name);
        if !path.exists() {
            return Err(anyhow!("Script '{}' not found", name));
        }
        fs::remove_file(&path)
            .context("Failed to delete script file")?;
        Ok(())
    }

    /// List all saved scripts
    pub fn list(&self) -> Result<Vec<Script>> {
        let mut scripts = Vec::new();
        
        if !self.scripts_dir.exists() {
            return Ok(scripts);
        }

        for entry in fs::read_dir(&self.scripts_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(script) = toml::from_str::<Script>(&content) {
                        scripts.push(script);
                    }
                }
            }
        }

        // Sort by name
        scripts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(scripts)
    }

    /// List script names only (for autocomplete)
    pub fn list_names(&self) -> Result<Vec<String>> {
        self.list().map(|scripts| scripts.into_iter().map(|s| s.name).collect())
    }

    /// Check if a script exists
    pub fn exists(&self, name: &str) -> bool {
        self.script_path(name).exists()
    }

    /// Filter scripts by prefix (for autocomplete)
    pub fn filter_by_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let prefix_lower = prefix.to_lowercase();
        self.list_names().map(|names| {
            names.into_iter()
                .filter(|name| name.to_lowercase().starts_with(&prefix_lower))
                .collect()
        })
    }

    /// Validate script name
    pub fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow!("Script name cannot be empty"));
        }
        if name.len() > 50 {
            return Err(anyhow!("Script name too long (max 50 characters)"));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(anyhow!("Script name can only contain letters, numbers, underscores, and hyphens"));
        }
        Ok(())
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new().expect("Failed to create ScriptManager")
    }
}
