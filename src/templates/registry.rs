use crate::templates::*;
use std::collections::HashMap;

/// Template registry for managing and accessing templates
pub struct TemplateRegistry {
    templates: HashMap<String, Template>,
}

impl TemplateRegistry {
    /// Create a new template registry with built-in templates
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Load built-in templates into the registry
    pub fn load_builtin_templates(&mut self) {
        let builtin_templates = crate::templates::builtin::get_builtin_templates();
        for (name, template) in builtin_templates {
            self.templates.insert(name, template);
        }
    }

    /// Create a new registry with built-in templates already loaded
    pub fn with_builtins() -> Self {
        Self {
            templates: crate::templates::builtin::get_builtin_templates(),
        }
    }

    /// Get a template by name
    pub fn get_template(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    /// List all available template names
    pub fn list_templates(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// Get all templates
    pub fn get_all_templates(&self) -> &HashMap<String, Template> {
        &self.templates
    }

    /// Add a custom template
    pub fn add_template(&mut self, template: Template) {
        self.templates.insert(template.name.clone(), template);
    }

    /// Remove a template
    pub fn remove_template(&mut self, name: &str) -> Option<Template> {
        self.templates.remove(name)
    }

    /// Check if template exists
    pub fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}