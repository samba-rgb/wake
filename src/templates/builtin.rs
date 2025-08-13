use crate::templates::*;
use std::collections::HashMap;
use std::time::Duration;

/// Get all built-in template definitions
pub fn get_builtin_templates() -> HashMap<String, Template> {
    let mut templates = HashMap::new();
    
    // JFR Template - Java Flight Recorder
    templates.insert("jfr".to_string(), Template {
        name: "jfr".to_string(),
        description: "Generate Java Flight Recorder dump for performance analysis".to_string(),
        parameters: vec![
            TemplateParameter {
                name: "pid".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                description: "Process ID to profile".to_string(),
                default_value: None,
                validation_regex: Some(r"^\d+$".to_string()),
            },
            TemplateParameter {
                name: "time".to_string(),
                param_type: ParameterType::Duration,
                required: true,
                description: "Recording duration (e.g., 30s, 5m, 1h)".to_string(),
                default_value: Some("30s".to_string()),
                validation_regex: Some(r"^\d+[smh]$".to_string()),
            },
        ],
        commands: vec![
            TemplateCommand {
                description: "Check if jcmd is available".to_string(),
                command: vec!["which".to_string(), "jcmd".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Verify process exists".to_string(),
                command: vec!["kill".to_string(), "-0".to_string(), "{{pid}}".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Start JFR recording".to_string(),
                command: vec![
                    "jcmd".to_string(),
                    "{{pid}}".to_string(),
                    "JFR.start".to_string(),
                    "name=wake-recording".to_string(),
                    "settings=profile".to_string(),
                    "duration={{time}}".to_string(),
                    "filename=/tmp/jfr_{{pid}}_wake.jfr".to_string(),
                ],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Verify JFR recording started successfully".to_string(),
                command: vec![
                    "jcmd".to_string(),
                    "{{pid}}".to_string(),
                    "JFR.check".to_string(),
                ],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: true,  // Don't fail the template if JFR.check fails
                capture_output: true,
            },
            TemplateCommand {
                description: "Wait for recording to complete".to_string(),
                command: vec!["wait".to_string(), "{{time}}".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: false,
            },
            TemplateCommand {
                description: "List generated JFR files".to_string(),
                command: vec!["ls".to_string(), "-la".to_string(), "/tmp/jfr_*.jfr".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: true,
                capture_output: true,
            },
        ],
        output_files: vec![
            OutputFilePattern {
                pattern: "/tmp/jfr_*.jfr".to_string(),
                file_type: FileType::Binary,
                description: "Java Flight Recorder dump".to_string(),
                required: true,
            },
        ],
        required_tools: vec!["jcmd".to_string()],
        timeout: Some(Duration::from_secs(1800)), // 30 minutes max
    });
    
    // Heap Dump Template
    templates.insert("heap-dump".to_string(), Template {
        name: "heap-dump".to_string(),
        description: "Generate Java heap dump for memory analysis".to_string(),
        parameters: vec![
            TemplateParameter {
                name: "pid".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                description: "Process ID to dump".to_string(),
                default_value: None,
                validation_regex: Some(r"^\d+$".to_string()),
            },
        ],
        commands: vec![
            TemplateCommand {
                description: "Check if jmap is available".to_string(),
                command: vec!["which".to_string(), "jmap".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Verify process exists".to_string(),
                command: vec!["kill".to_string(), "-0".to_string(), "{{pid}}".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Generate heap dump using jmap".to_string(),
                command: vec![
                    "jmap".to_string(),
                    "-dump:live,format=b,file=/tmp/heap_dump_{{pid}}_wake.hprof".to_string(),
                    "{{pid}}".to_string(),
                ],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "List generated heap dump files".to_string(),
                command: vec!["ls".to_string(), "-la".to_string(), "/tmp/heap_dump_*.hprof".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: true,
                capture_output: true,
            },
        ],
        output_files: vec![
            OutputFilePattern {
                pattern: "/tmp/heap_dump_*.hprof".to_string(),
                file_type: FileType::Binary,
                description: "Java heap dump".to_string(),
                required: true,
            },
        ],
        required_tools: vec!["jmap".to_string()],
        timeout: Some(Duration::from_secs(600)), // 10 minutes max
    });
    
    // Thread Dump Template
    templates.insert("thread-dump".to_string(), Template {
        name: "thread-dump".to_string(),
        description: "Generate Java thread dump for deadlock and performance analysis".to_string(),
        parameters: vec![
            TemplateParameter {
                name: "pid".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                description: "Process ID to dump".to_string(),
                default_value: None,
                validation_regex: Some(r"^\d+$".to_string()),
            },
        ],
        commands: vec![
            TemplateCommand {
                description: "Verify process exists".to_string(),
                command: vec!["kill".to_string(), "-0".to_string(), "{{pid}}".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "Generate thread dump using jstack".to_string(),
                command: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "if command -v jstack > /dev/null; then jstack {{pid}} > /tmp/thread_dump_{{pid}}_$(date +%Y%m%d_%H%M%S).txt; else jcmd {{pid}} Thread.print > /tmp/thread_dump_{{pid}}_$(date +%Y%m%d_%H%M%S).txt; fi".to_string(),
                ],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
                capture_output: true,
            },
            TemplateCommand {
                description: "List generated thread dump files".to_string(),
                command: vec!["ls".to_string(), "-la".to_string(), "/tmp/thread_dump_*.txt".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: true,
                capture_output: true,
            },
        ],
        output_files: vec![
            OutputFilePattern {
                pattern: "/tmp/thread_dump_*.txt".to_string(),
                file_type: FileType::Text,
                description: "Java thread dump".to_string(),
                required: true,
            },
        ],
        required_tools: vec!["jstack".to_string(), "jcmd".to_string()],
        timeout: Some(Duration::from_secs(300)), // 5 minutes max
    });
    
    templates
}