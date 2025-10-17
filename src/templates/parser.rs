use crate::templates::*;
use anyhow::{anyhow, Result};
use regex::Regex;
use std::collections::HashMap;

/// Template parameter parser and validator
pub struct TemplateParser;

impl TemplateParser {
    /// Parse and validate template arguments against template parameters
    pub fn parse_arguments(
        template: &Template,
        arguments: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut parsed_args = HashMap::new();
        
        // Check if we have the right number of arguments
        if arguments.len() != template.parameters.len() {
            return Err(anyhow!(
                "Template '{}' expects {} arguments, got {}",
                template.name,
                template.parameters.len(),
                arguments.len()
            ));
        }

        // Parse and validate each argument
        for (i, param) in template.parameters.iter().enumerate() {
            let arg_value = &arguments[i];
            
            // Validate argument based on parameter type
            Self::validate_parameter_value(param, arg_value)?;

            parsed_args.insert(param.name.clone(), arg_value.clone());
        }

        Ok(parsed_args)
    }

    /// Validate a parameter value against its type and constraints
    pub fn validate_parameter_value(param: &TemplateParameter, value: &str) -> Result<()> {
        // Type-specific validation
        match param.param_type {
            ParameterType::Integer => {
                value.parse::<i64>().map_err(|_| {
                    anyhow!("Parameter '{}' must be an integer, got: '{}'", param.name, value)
                })?;
            }
            ParameterType::Duration => {
                Self::parse_duration(value)?;
            }
            ParameterType::Path => {
                if value.is_empty() {
                    return Err(anyhow!("Parameter '{}' cannot be empty", param.name));
                }
            }
            ParameterType::Boolean => {
                match value.to_lowercase().as_str() {
                    "true" | "false" | "yes" | "no" | "1" | "0" => {}
                    _ => return Err(anyhow!(
                        "Parameter '{}' must be a boolean value (true/false, yes/no, 1/0), got: '{}'", 
                        param.name, value
                    )),
                }
            }
            ParameterType::String => {
                // String parameters are always valid unless regex constraints apply
            }
        }

        // Validate against regex if provided
        if let Some(ref regex_pattern) = param.validation_regex {
            let regex = Regex::new(regex_pattern)
                .map_err(|e| anyhow!("Invalid regex pattern for parameter '{}': {}", param.name, e))?;
            
            if !regex.is_match(value) {
                return Err(anyhow!(
                    "Parameter '{}' value '{}' doesn't match required format: {}",
                    param.name, value, regex_pattern
                ));
            }
        }

        Ok(())
    }

    /// Parse duration string to seconds
    pub fn parse_duration(duration_str: &str) -> Result<u64> {
        let duration_str = duration_str.trim();
        
        if duration_str.is_empty() {
            return Err(anyhow!("Duration cannot be empty"));
        }

        let (number_part, unit_part) = if duration_str.ends_with("ms") {
            (&duration_str[..duration_str.len() - 2], "ms")
        } else if duration_str.ends_with('s') {
            (&duration_str[..duration_str.len() - 1], "s")
        } else if duration_str.ends_with('m') {
            (&duration_str[..duration_str.len() - 1], "m")
        } else if duration_str.ends_with('h') {
            (&duration_str[..duration_str.len() - 1], "h")
        } else if duration_str.ends_with('d') {
            (&duration_str[..duration_str.len() - 1], "d")
        } else {
            return Err(anyhow!("Duration must end with 'ms', 's', 'm', 'h', or 'd'"));
        };

        let number: u64 = number_part.parse()
            .map_err(|_| anyhow!("Invalid duration format: '{}'", duration_str))?;

        let seconds = match unit_part {
            "ms" => {
                if number < 1000 {
                    return Err(anyhow!("Duration in milliseconds must be at least 1000ms (1s)"));
                }
                number / 1000
            }
            "s" => number,
            "m" => number * 60,
            "h" => number * 3600,
            "d" => number * 86400,
            _ => return Err(anyhow!("Invalid duration unit: '{}'", unit_part)),
        };

        if seconds == 0 {
            return Err(anyhow!("Duration must be greater than 0"));
        }

        if seconds > 86400 * 7 {  // 1 week limit
            return Err(anyhow!("Duration cannot exceed 1 week (7d)"));
        }

        Ok(seconds)
    }

    /// Format duration seconds back to human-readable format
    pub fn format_duration(seconds: u64) -> String {
        if seconds >= 86400 {
            format!("{}d", seconds / 86400)
        } else if seconds >= 3600 {
            format!("{}h", seconds / 3600)
        } else if seconds >= 60 {
            format!("{}m", seconds / 60)
        } else {
            format!("{seconds}s")
        }
    }

    /// Generate help text for a template
    pub fn generate_template_help(template: &Template) -> String {
        let mut help = String::new();
        
        help.push_str(&format!("Template: {}\n", template.name));
        help.push_str(&format!("Description: {}\n\n", template.description));
        
        if !template.parameters.is_empty() {
            help.push_str("Parameters:\n");
            for param in &template.parameters {
                help.push_str(&format!(
                    "  {} ({}): {}\n",
                    param.name,
                    Self::format_parameter_type(&param.param_type),
                    param.description
                ));
                
                if let Some(ref default) = param.default_value {
                    help.push_str(&format!("    Default: {default}\n"));
                }
                
                if let Some(ref regex) = param.validation_regex {
                    help.push_str(&format!("    Format: {regex}\n"));
                }
            }
            help.push('\n');
        }
        
        if !template.required_tools.is_empty() {
            help.push_str("Required tools in pods:\n");
            for tool in &template.required_tools {
                help.push_str(&format!("  - {tool}\n"));
            }
            help.push('\n');
        }
        
        help.push_str("Usage examples:\n");
        help.push_str(&Self::generate_usage_examples(template));
        
        help
    }

    /// Format parameter type for help display
    fn format_parameter_type(param_type: &ParameterType) -> &'static str {
        match param_type {
            ParameterType::Integer => "integer",
            ParameterType::String => "string", 
            ParameterType::Duration => "duration",
            ParameterType::Path => "path",
            ParameterType::Boolean => "boolean",
        }
    }

    /// Generate usage examples for a template
    fn generate_usage_examples(template: &Template) -> String {
        let mut examples = String::new();
        
        // Basic example
        let example_args: Vec<String> = template.parameters.iter().map(|param| {
            match param.param_type {
                ParameterType::Integer => "1234".to_string(),
                ParameterType::Duration => param.default_value.clone().unwrap_or_else(|| "30s".to_string()),
                ParameterType::Boolean => "true".to_string(),
                ParameterType::Path => "/tmp/output".to_string(),
                ParameterType::String => param.default_value.clone().unwrap_or_else(|| "value".to_string()),
            }
        }).collect();
        
        examples.push_str(&format!(
            "  wake -t {} {}\n",
            template.name,
            example_args.join(" ")
        ));
        
        // With namespace selection
        examples.push_str(&format!(
            "  wake -n production -t {} {}\n",
            template.name,
            example_args.join(" ")
        ));
        
        // With pod selector
        examples.push_str(&format!(
            "  wake -p \"java-app.*\" -t {} {}\n",
            template.name,
            example_args.join(" ")
        ));
        
        // With custom output directory
        examples.push_str(&format!(
            "  wake -t {} {} --template-outdir ./diagnostics\n",
            template.name,
            example_args.join(" ")
        ));
        
        examples
    }

    /// Validate template definition
    pub fn validate_template(template: &Template) -> Result<()> {
        if template.name.is_empty() {
            return Err(anyhow!("Template name cannot be empty"));
        }

        if template.description.is_empty() {
            return Err(anyhow!("Template description cannot be empty"));
        }

        if template.commands.is_empty() {
            return Err(anyhow!("Template must have at least one command"));
        }

        // Validate parameter names are unique
        let mut param_names = std::collections::HashSet::new();
        for param in &template.parameters {
            if !param_names.insert(&param.name) {
                return Err(anyhow!("Duplicate parameter name: {}", param.name));
            }

            // Validate regex if provided
            if let Some(ref regex) = param.validation_regex {
                Regex::new(regex)
                    .map_err(|e| anyhow!("Invalid regex for parameter '{}': {}", param.name, e))?;
            }
        }

        // Validate commands
        for (i, cmd) in template.commands.iter().enumerate() {
            if cmd.command.is_empty() {
                return Err(anyhow!("Command {} cannot be empty", i + 1));
            }
            
            if cmd.description.is_empty() {
                return Err(anyhow!("Command {} must have a description", i + 1));
            }
        }

        Ok(())
    }
}