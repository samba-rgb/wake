// Templates module for Wake - predefined diagnostic command sequences
pub mod builtin;
pub mod executor;
pub mod parser;
pub mod registry;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Template definition for diagnostic commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub parameters: Vec<TemplateParameter>,
    pub commands: Vec<TemplateCommand>,
    pub output_files: Vec<OutputFilePattern>,
    pub required_tools: Vec<String>,
    pub timeout: Option<Duration>,
}

/// Individual command within a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCommand {
    pub description: String,
    pub command: Vec<String>,  // Command and arguments
    pub working_dir: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub ignore_failure: bool,  // Continue on failure
    pub capture_output: bool,  // Capture stdout/stderr
}

/// Output file pattern to collect after execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFilePattern {
    pub pattern: String,       // Glob pattern like "/tmp/heap_dump_*.hprof"
    pub file_type: FileType,
    pub description: String,
    pub required: bool,        // Fail if file not found
}

/// File type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    Binary,    // For heap dumps, JFR files
    Text,      // For thread dumps, logs
    Archive,   // For tar/zip files
}

/// Template parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    pub name: String,
    pub param_type: ParameterType,
    pub required: bool,
    pub description: String,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
}

/// Parameter type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    Integer,
    String,
    Duration,  // For time values like "30s", "5m"
    Path,      // For file paths
    Boolean,   // For flags
}

/// Template execution context
#[derive(Debug, Clone)]
pub struct TemplateExecution {
    pub template_name: String,
    pub arguments: HashMap<String, String>,
    pub execution_id: String,
    pub timestamp: DateTime<Utc>,
    pub output_dir: PathBuf,   // Local output directory
}

/// Result of template execution across all pods
#[derive(Debug, Clone)]
pub struct TemplateExecutionResult {
    pub execution_id: String,
    pub template_name: String,
    pub pod_results: Vec<PodExecutionResult>,
    pub output_dir: PathBuf,
}

/// Result of template execution on a single pod
#[derive(Debug, Clone)]
pub struct PodExecutionResult {
    pub pod_name: String,
    pub pod_namespace: String,
    pub command_results: Vec<CommandResult>,
    pub downloaded_files: Vec<DownloadedFile>,
    pub success: bool,
}

/// Result of a single command execution
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Downloaded file information
#[derive(Debug, Clone)]
pub struct DownloadedFile {
    pub remote_path: String,
    pub local_path: PathBuf,
    pub file_type: FileType,
    pub size_bytes: u64,
}