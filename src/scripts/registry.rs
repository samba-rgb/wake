use super::manager::{SavedScript, ScriptManager, ParameterDef, ParameterType};
use anyhow::Result;

/// Registry for managing and querying scripts
pub struct ScriptRegistry {
    manager: ScriptManager,
}

impl ScriptRegistry {
    /// Create a new script registry
    pub fn new() -> Result<Self> {
        let manager = ScriptManager::new()?;
        Ok(Self { manager })
    }

    /// Get all scripts
    pub fn get_all_scripts(&self) -> Result<Vec<SavedScript>> {
        self.manager.list_scripts()
    }

    /// Get a script by name
    pub fn get_script(&self, name: &str) -> Result<SavedScript> {
        self.manager.load_script(name)
    }

    /// Search scripts by partial name match
    pub fn search_scripts(&self, query: &str) -> Result<Vec<SavedScript>> {
        if query.is_empty() {
            self.manager.list_scripts()
        } else {
            self.manager.filter_scripts(query)
        }
    }

    /// Save a new script
    pub fn save_script(&self, script: &SavedScript) -> Result<()> {
        ScriptManager::validate_script_name(&script.name)?;
        self.manager.save_script(script)
    }

    /// Delete a script
    pub fn delete_script(&self, name: &str) -> Result<()> {
        self.manager.delete_script(name)
    }

    /// Check if script exists
    pub fn script_exists(&self, name: &str) -> bool {
        self.manager.script_exists(name)
    }

    /// Get script names only
    pub fn get_script_names(&self) -> Result<Vec<String>> {
        self.manager
            .list_scripts()
            .map(|scripts| scripts.into_iter().map(|s| s.name).collect())
    }
}

impl Default for ScriptRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create script registry")
    }
}
