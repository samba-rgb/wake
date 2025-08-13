# Wake Template Feature Design Document - Revised

## Overview

The template feature extends Wake with predefined command sequences that execute via `kubectl exec` and automatically retrieve output files to your local system. Templates are defined as a series of commands rather than scripts, providing maximum flexibility for users to create their own diagnostic workflows.

**NEW: Interactive Terminal UI** - Templates now feature a real-time terminal UI showing parallel execution progress across all pods, with clickable pod status and live log viewing capabilities.

## Key Design Changes

### 1. Template as Command Sequences
Templates are now defined as a series of `kubectl exec` commands rather than scripts, allowing:
- More granular control over execution
- Better error handling per command
- Easier customization and debugging
- No need to copy scripts to pods

### 2. Automatic File Retrieval
After command execution, Wake automatically:
- Detects output files created by the template
- Copies them to local system using `kubectl cp`
- Organizes files by pod and timestamp
- Provides download summary

### 3. Built-in Wait Command
Wake includes a special `wait` command that can be used in any template:
- `wait <duration>` - Wait for specified time (e.g., `wait 30s`, `wait 5m`, `wait 1h`)
- Shows progress indication for waits longer than 30 seconds
- Supports human-readable duration formats
- Can be used by anyone in custom templates

### 4. Interactive Terminal UI
**NEW**: Real-time terminal interface for template execution:
- Live progress bars for each pod
- Pod status indicators (Starting → Running → Completed/Failed)
- Clickable pods to view execution logs
- Parallel execution visualization
- Command-by-command progress tracking
- File download status

## Feature Requirements

### Core Templates
1. **jfr** (Java Flight Recorder) - Args: `pid`, `time`
2. **heap-dump** - Args: `pid`  
3. **thread-dump** - Args: `pid`

### Usage Examples
```bash
# Execute heap dump on process 7 with UI
wake -t heap-dump 7

# Execute JFR recording with UI
wake -t jfr 1234 30s

# Execute thread dump with custom output directory
wake -t thread-dump 456 --template-outdir ./diagnostics

# With namespace and pod selection
wake -n production -p "java-app.*" -t heap-dump 7

# Disable UI for headless environments
wake -t jfr 1234 5m --no-template-ui
```

## Interactive Terminal UI Design

### Terminal Layout
```
┌─ Wake Template Execution ─────────────────────────────────────────────────────┐
│ Template: jfr - Java Flight Recorder dump for performance analysis           │
│ Arguments: pid=1234, time=5m                                                 │
│ Execution ID: 550e8400-e29b-41d4-a716-446655440000                          │
│ Output Directory: ./wake-templates-20250812_143022                           │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│ Pod Execution Status (3/3 pods)                                              │
│                                                                               │
│ ► production/java-app-1     ●●●●●○○○○○  RUNNING    [Command 3/5] Wait 5m     │
│   production/java-app-2     ●●●●●●●●●●  COMPLETED  [✓] All commands done     │
│   production/java-app-3     ●●●○○○○○○○  RUNNING    [Command 2/5] Start JFR    │
│                                                                               │
│ Overall Progress: 2/3 pods completed                                         │
│ Files Downloaded: 1/3 pods                                                   │
│                                                                               │
├─ Pod Logs (production/java-app-1) ───────────────────────────────────────────┤
│ [14:30:22] ⚡ Command 1/5: Check if jcmd is available                        │
│ [14:30:22]    Output: /usr/bin/jcmd                                          │
│ [14:30:23] ⚡ Command 2/5: Verify process exists                             │
│ [14:30:23]    Output: (no output - success)                                  │
│ [14:30:24] ⚡ Command 3/5: Start JFR recording                               │
│ [14:30:24]    Output: Recording started for PID 1234                        │
│ [14:30:25] ⚡ Command 4/5: Wait for recording to complete                    │
│ [14:30:25]    ⏳ Waiting 5m (300 seconds) locally...                         │
│ [14:30:25]    ⏳ [████████████████████░░░░░░░░░░░░░░░░░░░░] 60% (03:00)       │
│                                                                               │
├───────────────────────────────────────────────────────────────────────────────┤
│ Controls: ↑↓ Select Pod | Enter View Logs | Esc Back | q Quit                │
└───────────────────────────────────────────────────────────────────────────────┘
```

### UI Components

#### 1. Header Section
- Template name and description
- Execution arguments and ID
- Output directory path
- Start time and elapsed time

#### 2. Pod Status Grid
- **Pod List**: Scrollable list of all selected pods
- **Status Indicators**: 
  - 🔄 Starting (gray)
  - ⚡ Running (yellow) 
  - ✅ Completed (green)
  - ❌ Failed (red)
- **Progress Bars**: Visual command progress (●●●○○○○○○○)
- **Current Action**: What the pod is currently doing
- **Selection**: Highlighted pod shows logs in bottom panel

#### 3. Log Viewer Panel
- **Real-time logs** from selected pod
- **Command-by-command output** with timestamps
- **Progress indicators** for wait commands
- **Error highlighting** for failed commands
- **Scrollable history** of all executed commands

#### 4. Footer Controls
- **Navigation**: Arrow keys to select pods
- **Log viewing**: Enter to focus on logs, Esc to return
- **Quit**: 'q' to exit (with confirmation if still running)

### UI State Management

```rust
// src/ui/template_ui.rs
#[derive(Debug, Clone)]
pub struct TemplateUIState {
    pub execution: TemplateExecution,
    pub pods: Vec<PodExecutionState>,
    pub selected_pod_index: usize,
    pub log_scroll_offset: usize,
    pub show_logs: bool,
    pub start_time: Instant,
}

#[derive(Debug, Clone)]
pub struct PodExecutionState {
    pub pod_info: PodInfo,
    pub status: PodStatus,
    pub current_command_index: usize,
    pub total_commands: usize,
    pub command_logs: Vec<CommandLog>,
    pub downloaded_files: Vec<DownloadedFile>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PodStatus {
    Starting,
    Running { command_index: usize },
    WaitingLocal { duration: String, progress: f64 },
    DownloadingFiles { current: usize, total: usize },
    Completed,
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct CommandLog {
    pub timestamp: DateTime<Local>,
    pub command_index: usize,
    pub description: String,
    pub output: Option<String>,
    pub status: CommandStatus,
}
```

## Architecture Design

### 1. Template System Components

#### A. Core Data Structures

```rust
// src/templates/mod.rs
#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub parameters: Vec<TemplateParameter>,
    pub commands: Vec<TemplateCommand>,
    pub output_files: Vec<OutputFilePattern>,
    pub required_tools: Vec<String>,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct TemplateCommand {
    pub description: String,
    pub command: Vec<String>,  // Command and arguments
    pub working_dir: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub ignore_failure: bool,  // Continue on failure
    pub capture_output: bool,  // Capture stdout/stderr
}

#[derive(Debug, Clone)]
pub struct OutputFilePattern {
    pub pattern: String,       // Glob pattern like "/tmp/heap_dump_*.hprof"
    pub file_type: FileType,
    pub description: String,
    pub required: bool,        // Fail if file not found
}

#[derive(Debug, Clone)]
pub enum FileType {
    Binary,    // For heap dumps, JFR files
    Text,      // For thread dumps, logs
    Archive,   // For tar/zip files
}

#[derive(Debug, Clone)]
pub struct TemplateParameter {
    pub name: String,
    pub param_type: ParameterType,
    pub required: bool,
    pub description: String,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ParameterType {
    Integer,
    String,
    Duration,  // For time values like "30s", "5m"
    Path,      // For file paths
    Boolean,   // For flags
}

#[derive(Debug, Clone)]
pub struct TemplateExecution {
    pub template_name: String,
    pub arguments: HashMap<String, String>,
    pub execution_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub output_dir: PathBuf,   // Local output directory
}
```

### 2. Built-in Templates

#### A. Template Definitions

```rust
// src/templates/builtin.rs
pub fn get_builtin_templates() -> HashMap<String, Template> {
    let mut templates = HashMap::new();
    
    // JFR Template - Simplified with wait command
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
                    "duration={{time}}".to_string(),
                    "filename=/tmp/jfr_{{pid}}_$(date +%Y%m%d_%H%M%S).jfr".to_string(),
                ],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: false,
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
                description: "Force garbage collection".to_string(),
                command: vec!["jcmd".to_string(), "{{pid}}".to_string(), "VM.gc".to_string()],
                working_dir: None,
                env_vars: HashMap::new(),
                ignore_failure: true,  // Don't fail if GC command fails
                capture_output: true,
            },
            TemplateCommand {
                description: "Generate heap dump".to_string(),
                command: vec![
                    "jcmd".to_string(),
                    "{{pid}}".to_string(),
                    "GC.dump_heap".to_string(),
                    "/tmp/heap_dump_{{pid}}_$(date +%Y%m%d_%H%M%S).hprof".to_string(),
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
        required_tools: vec!["jcmd".to_string()],
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
```

#### B. Template Scripts

##### JFR Script (`src/templates/scripts/jfr.sh`)
```bash
#!/bin/bash
set -e

# Template parameters
PID={{pid}}
DURATION={{time}}

# Script metadata
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
POD_NAME=${HOSTNAME:-unknown}
NAMESPACE=${NAMESPACE:-default}
OUTPUT_FILE="/tmp/jfr_${POD_NAME}_${PID}_${TIMESTAMP}.jfr"

echo "=== Java Flight Recorder Collection ==="
echo "Pod: $POD_NAME"
echo "Namespace: $NAMESPACE"
echo "PID: $PID"
echo "Duration: $DURATION"
echo "Timestamp: $TIMESTAMP"
echo "Output: $OUTPUT_FILE"
echo "======================================="

# Validate prerequisites
if ! command -v jcmd &> /dev/null; then
    echo "ERROR: jcmd not found. Please ensure Java JDK is installed."
    echo "Required: Java JDK 8+ with jcmd utility"
    exit 1
fi

# Check if process exists and is a Java process
if ! kill -0 $PID 2>/dev/null; then
    echo "ERROR: Process $PID not found or not accessible"
    ps aux | grep java | head -5
    exit 1
fi

# Verify it's a Java process
PROCESS_INFO=$(ps -p $PID -o comm= 2>/dev/null || echo "")
if [[ ! "$PROCESS_INFO" =~ java ]]; then
    echo "WARNING: Process $PID might not be a Java process"
    echo "Process info: $PROCESS_INFO"
fi

# Start JFR recording
echo "Starting JFR recording..."
START_OUTPUT=$(jcmd $PID JFR.start duration=$DURATION filename=$OUTPUT_FILE 2>&1)
if [[ $? -ne 0 ]]; then
    echo "ERROR: Failed to start JFR recording"
    echo "Output: $START_OUTPUT"
    exit 1
fi

echo "JFR recording started successfully"
echo "Recording will complete in $DURATION"
echo "Monitoring progress..."

# Convert duration to seconds for sleep
SLEEP_DURATION=$(echo $DURATION | sed 's/s$//' | sed 's/m$//')
if [[ "$DURATION" =~ m$ ]]; then
    SLEEP_DURATION=$((SLEEP_DURATION * 60))
elif [[ "$DURATION" =~ h$ ]]; then
    SLEEP_DURATION=$((SLEEP_DURATION * 3600))
fi

# Wait for recording to complete
sleep $SLEEP_DURATION

# Verify recording completed
if [[ -f "$OUTPUT_FILE" ]]; then
    echo "JFR recording completed successfully!"
    echo "File: $OUTPUT_FILE"
    echo "Size: $(ls -lh $OUTPUT_FILE | awk '{print $5}')"
    
    # Display file info
    echo ""
    echo "=== Recording Summary ==="
    echo "Duration: $DURATION"
    echo "Output file: $OUTPUT_FILE"
    echo "File size: $(du -h $OUTPUT_FILE | cut -f1)"
    echo "========================="
else
    echo "ERROR: JFR recording file not found: $OUTPUT_FILE"
    exit 1
fi
```

##### Heap Dump Script (`src/templates/scripts/heap-dump.sh`)
```bash
#!/bin/bash
set -e

# Template parameters
PID={{pid}}

# Script metadata
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
POD_NAME=${HOSTNAME:-unknown}
NAMESPACE=${NAMESPACE:-default}
OUTPUT_FILE="/tmp/heap_dump_${POD_NAME}_${PID}_${TIMESTAMP}.hprof"

echo "=== Java Heap Dump Collection ==="
echo "Pod: $POD_NAME"
echo "Namespace: $NAMESPACE"
echo "PID: $PID"
echo "Timestamp: $TIMESTAMP"
echo "Output: $OUTPUT_FILE"
echo "=================================="

# Validate prerequisites
if ! command -v jcmd &> /dev/null; then
    echo "ERROR: jcmd not found. Please ensure Java JDK is installed."
    echo "Required: Java JDK 8+ with jcmd utility"
    exit 1
fi

# Check if process exists
if ! kill -0 $PID 2>/dev/null; then
    echo "ERROR: Process $PID not found or not accessible"
    ps aux | grep java | head -5
    exit 1
fi

# Verify it's a Java process
PROCESS_INFO=$(ps -p $PID -o comm= 2>/dev/null || echo "")
if [[ ! "$PROCESS_INFO" =~ java ]]; then
    echo "WARNING: Process $PID might not be a Java process"
    echo "Process info: $PROCESS_INFO"
fi

# Show memory info before dump
echo "Memory information before dump:"
jcmd $PID VM.info | grep -E "(heap|memory)" || true

# Force garbage collection before heap dump (optional but recommended)
echo "Running garbage collection..."
jcmd $PID GC.run_finalization 2>/dev/null || true
jcmd $PID VM.gc 2>/dev/null || true

# Generate heap dump
echo "Generating heap dump..."
DUMP_OUTPUT=$(jcmd $PID GC.dump_heap $OUTPUT_FILE 2>&1)
if [[ $? -ne 0 ]]; then
    echo "ERROR: Failed to generate heap dump"
    echo "Output: $DUMP_OUTPUT"
    exit 1
fi

# Verify heap dump was created
if [[ -f "$OUTPUT_FILE" ]]; then
    echo "Heap dump completed successfully!"
    echo "File: $OUTPUT_FILE"
    echo "Size: $(ls -lh $OUTPUT_FILE | awk '{print $5}')"
    
    # Display file info
    echo ""
    echo "=== Heap Dump Summary ==="
    echo "Process PID: $PID"
    echo "Output file: $OUTPUT_FILE"
    echo "File size: $(du -h $OUTPUT_FILE | cut -f1)"
    echo "Created: $(date)"
    echo "========================="
    
    # Show process memory usage
    echo ""
    echo "Process memory usage:"
    ps -p $PID -o pid,ppid,rss,vsz,pcpu,pmem,comm || true
else
    echo "ERROR: Heap dump file not found: $OUTPUT_FILE"
    exit 1
fi
```

##### Thread Dump Script (`src/templates/scripts/thread-dump.sh`)
```bash
#!/bin/bash
set -e

# Template parameters
PID={{pid}}

# Script metadata
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
POD_NAME=${HOSTNAME:-unknown}
NAMESPACE=${NAMESPACE:-default}
OUTPUT_FILE="/tmp/thread_dump_${POD_NAME}_${PID}_${TIMESTAMP}.txt"

echo "=== Java Thread Dump Collection ==="
echo "Pod: $POD_NAME"
echo "Namespace: $NAMESPACE"
echo "PID: $PID"
echo "Timestamp: $TIMESTAMP"
echo "Output: $OUTPUT_FILE"
echo "===================================="

# Check if process exists
if ! kill -0 $PID 2>/dev/null; then
    echo "ERROR: Process $PID not found or not accessible"
    ps aux | grep java | head -5
    exit 1
fi

# Verify it's a Java process
PROCESS_INFO=$(ps -p $PID -o comm= 2>/dev/null || echo "")
if [[ ! "$PROCESS_INFO" =~ java ]]; then
    echo "WARNING: Process $PID might not be a Java process"
    echo "Process info: $PROCESS_INFO"
fi

# Generate thread dump using available tools
echo "Generating thread dump..."

if command -v jstack &> /dev/null; then
    echo "Using jstack for thread dump..."
    jstack $PID > $OUTPUT_FILE 2>&1
    DUMP_STATUS=$?
elif command -v jcmd &> /dev/null; then
    echo "Using jcmd for thread dump..."
    jcmd $PID Thread.print > $OUTPUT_FILE 2>&1
    DUMP_STATUS=$?
else
    echo "ERROR: Neither jstack nor jcmd found."
    echo "Please ensure Java JDK is installed."
    exit 1
fi

# Check if dump was successful
if [[ $DUMP_STATUS -ne 0 ]]; then
    echo "ERROR: Failed to generate thread dump"
    if [[ -f "$OUTPUT_FILE" ]]; then
        echo "Error output:"
        cat $OUTPUT_FILE
    fi
    exit 1
fi

# Verify thread dump was created and has content
if [[ -f "$OUTPUT_FILE" && -s "$OUTPUT_FILE" ]]; then
    echo "Thread dump completed successfully!"
    echo "File: $OUTPUT_FILE"
    echo "Size: $(ls -lh $OUTPUT_FILE | awk '{print $5}')"
    
    # Display summary information
    THREAD_COUNT=$(grep -c "^\"" $OUTPUT_FILE 2>/dev/null || echo "0")
    DEADLOCK_COUNT=$(grep -c "Found Java-level deadlock" $OUTPUT_FILE 2>/dev/null || echo "0")
    
    echo ""
    echo "=== Thread Dump Summary ==="
    echo "Total threads: $THREAD_COUNT"
    echo "Deadlocks found: $DEADLOCK_COUNT"
    echo "File size: $(du -h $OUTPUT_FILE | cut -f1)"
    echo "Created: $(date)"
    echo "=========================="
    
    # Show first few lines of the dump
    echo ""
    echo "Thread dump preview (first 20 lines):"
    echo "-------------------------------------"
    head -20 $OUTPUT_FILE
    echo "-------------------------------------"
    echo "Full thread dump saved to: $OUTPUT_FILE"
else
    echo "ERROR: Thread dump file not found or empty: $OUTPUT_FILE"
    exit 1
fi
```

### 3. CLI Integration

#### A. Modified Args Structure
```rust
// src/cli/args.rs - Add to existing Args struct
pub struct Args {
    // ...existing fields...
    
    /// Template name and arguments for execution
    #[arg(short = 't', long = "template", value_names = &["TEMPLATE", "ARGS"], num_args = 1.., help = "Execute predefined template with arguments (e.g., -t heap-dump 7, -t jfr 1234 30s)")]
    pub template: Option<Vec<String>>,
    
    /// Output directory for template results (overrides default)
    #[arg(long = "template-outdir", value_name = "DIR", help = "Directory to save template output files (default: ./wake-templates-<timestamp>)")]
    pub template_outdir: Option<PathBuf>,
    
    /// Disable template execution UI (use simple console output)
    #[arg(long = "no-template-ui", help = "Disable interactive UI for template execution")]
    pub no_template_ui: bool,
}
```

#### B. Modified Template Execution Function

```rust
// src/cli/mod.rs - Updated template execution function
async fn run_template_in_pods(args: &Args, template_args: &[String]) -> Result<()> {
    println!("🚀 Wake Template Execution");
    println!("==========================");
    
    // Initialize kubernetes client
    let client = crate::k8s::create_client(args).await?;
    
    // Initialize template system
    let registry = TemplateRegistry::new();
    let parser = TemplateParser::new(registry);
    let executor = TemplateExecutor::new(client.clone(), !args.no_template_ui);
    
    // Parse template arguments
    let mut execution = parser.parse_template_args(template_args)
        .context("Failed to parse template arguments")?;
    
    // Set output directory
    execution.output_dir = if let Some(outdir) = &args.template_outdir {
        outdir.clone()
    } else {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        std::env::current_dir()?.join(format!("wake-templates-{}", timestamp))
    };
    
    // Get template for validation and info display
    let template = executor.get_template(&execution.template_name)
        .ok_or_else(|| anyhow!("Template not found: {}", execution.template_name))?;
    
    // Display execution info
    println!("Template: {} - {}", template.name, template.description);
    println!("Execution ID: {}", execution.execution_id);
    println!("Arguments:");
    for (key, value) in &execution.arguments {
        println!("  {}: {}", key, value);
    }
    println!("Required tools: {}", template.required_tools.join(", "));
    println!("Commands to execute: {}", template.commands.len());
    println!("Output directory: {}", execution.output_dir.display());
    println!("");
    
    // Select pods
    let pod_regex = args.pod_regex().context("Invalid pod selector regex")?;
    let container_regex = args.container_regex().context("Invalid container selector regex")?;
    
    let pods = crate::k8s::pod::select_pods(
        &client,
        &args.namespace,
        &pod_regex,
        &container_regex,
        args.all_namespaces,
        args.resource.as_deref(),
    ).await?;
    
    if pods.is_empty() {
        return Err(anyhow!("No pods found matching the selection criteria"));
    }
    
    println!("Selected {} pods for template execution:", pods.len());
    for pod in &pods {
        println!("  - {}/{}", pod.namespace, pod.name);
    }
    println!("");
    
    // Execute template
    let result = executor.execute_template(&execution, &pods).await;
    
    // Display execution summary
    match result {
        Ok(exec_result) => {
            println!("");
            println!("✅ Template execution completed successfully!");
            println!("==================================================");
            println!("Template: {}", exec_result.template_name);
            println!("Execution ID: {}", exec_result.execution_id);
            println!("Output directory: {}", exec_result.output_dir.display());
            println!("");
            
            // Summary of results
            let successful_pods = exec_result.pod_results.iter().filter(|r| r.success).count();
            let total_files = exec_result.pod_results.iter()
                .map(|r| r.downloaded_files.len())
                .sum::<usize>();
            
            println!("Results Summary:");
            println!("  Successful pods: {}/{}", successful_pods, exec_result.pod_results.len());
            println!("  Total files downloaded: {}", total_files);
            
            // Per-pod summary
            println!("");
            println!("Per-pod results:");
            for pod_result in &exec_result.pod_results {
                let status = if pod_result.success { "✅" } else { "❌" };
                println!("  {} {}/{} - {} files", 
                    status, 
                    pod_result.pod_namespace, 
                    pod_result.pod_name,
                    pod_result.downloaded_files.len()
                );
                
                for file in &pod_result.downloaded_files {
                    println!("    📁 {} ({} bytes)", 
                        file.local_path.display(), 
                        file.size_bytes
                    );
                }
            }
            
            println!("");
            println!("All files are available in: {}", exec_result.output_dir.display());
        },
        Err(e) => {
            println!("");
            println!("❌ Template execution failed");
            println!("Template: {}", execution.template_name);
            println!("Execution ID: {}", execution.execution_id);
            println!("Error: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}
```

### 5. Template Execution Engine with Parallel Processing and UI

```rust
// src/templates/executor.rs
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use kube::Client;
use tokio::process::Command as AsyncCommand;
use tokio::time::{Duration, sleep};
use tokio::sync::{mpsc, Mutex};
use std::io::{self, Write};

pub struct TemplateExecutor {
    registry: TemplateRegistry,
    client: Client,
    ui_enabled: bool,
}

impl TemplateExecutor {
    pub fn new(client: Client, ui_enabled: bool) -> Self {
        Self {
            registry: TemplateRegistry::new(),
            client,
            ui_enabled,
        }
    }
    
    pub async fn execute_template(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
    ) -> Result<TemplateExecutionResult> {
        let template = self.registry.get_template(&execution.template_name)
            .ok_or_else(|| anyhow!("Template not found: {}", execution.template_name))?;
        
        if self.ui_enabled {
            // Use interactive UI for execution
            self.execute_template_with_ui(execution, pods, template).await
        } else {
            // Use simple console output
            self.execute_template_console(execution, pods, template).await
        }
    }
    
    async fn execute_template_with_ui(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
        template: &Template,
    ) -> Result<TemplateExecutionResult> {
        // Create UI state
        let ui_state = Arc::new(Mutex::new(TemplateUIState::new(
            execution.clone(),
            pods.to_vec(),
            template.clone(),
        )));
        
        // Create channels for UI updates
        let (ui_tx, ui_rx) = mpsc::channel::<UIUpdate>(1000);
        
        // Start UI in separate task
        let ui_state_clone = ui_state.clone();
        let ui_task = tokio::spawn(async move {
            crate::ui::template_ui::run_template_ui(ui_state_clone, ui_rx).await
        });
        
        // Execute template with UI updates
        let result = self.execute_template_parallel_with_ui(
            execution, 
            pods, 
            template, 
            ui_tx
        ).await;
        
        // Wait for UI to finish
        ui_task.await??;
        
        result
    }
    
    async fn execute_template_parallel_with_ui(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
        template: &Template,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<TemplateExecutionResult> {
        // Create output directory
        std::fs::create_dir_all(&execution.output_dir)?;
        
        // **PARALLEL EXECUTION**: Create futures for all pods with UI updates
        let pod_futures: Vec<_> = pods.iter().enumerate().map(|(index, pod)| {
            let template = template.clone();
            let execution = execution.clone();
            let pod = pod.clone();
            let ui_tx = ui_tx.clone();
            
            async move {
                // Send starting status
                let _ = ui_tx.send(UIUpdate::PodStatusChanged {
                    pod_index: index,
                    status: PodStatus::Starting,
                }).await;
                
                let result = self.execute_template_on_pod_with_ui(
                    &template,
                    &execution,
                    &pod,
                    index,
                    ui_tx.clone(),
                ).await;
                
                // Send completion status
                let status = match &result {
                    Ok(_) => PodStatus::Completed,
                    Err(e) => PodStatus::Failed { error: e.to_string() },
                };
                
                let _ = ui_tx.send(UIUpdate::PodStatusChanged {
                    pod_index: index,
                    status,
                }).await;
                
                result
            }
        }).collect();
        
        // **AWAIT ALL PODS**: Execute all pods concurrently
        let pod_results = futures::future::try_join_all(pod_futures).await?;
        
        // Send completion signal to UI
        let _ = ui_tx.send(UIUpdate::ExecutionCompleted).await;
        
        Ok(TemplateExecutionResult {
            execution_id: execution.execution_id.clone(),
            template_name: execution.template_name.clone(),
            pod_results,
            output_dir: execution.output_dir.clone(),
        })
    }
    
    async fn execute_template_on_pod_with_ui(
        &self,
        template: &Template,
        execution: &TemplateExecution,
        pod: &PodInfo,
        pod_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<PodExecutionResult> {
        let mut command_results = Vec::new();
        let mut downloaded_files = Vec::new();
        
        // Execute each command in sequence with UI updates
        for (cmd_index, template_cmd) in template.commands.iter().enumerate() {
            // Update UI with current command
            let _ = ui_tx.send(UIUpdate::PodStatusChanged {
                pod_index,
                status: PodStatus::Running { command_index: cmd_index },
            }).await;
            
            let _ = ui_tx.send(UIUpdate::CommandStarted {
                pod_index,
                command_index: cmd_index,
                description: template_cmd.description.clone(),
            }).await;
            
            let result = self.execute_command_on_pod_with_ui(
                template_cmd,
                execution,
                pod,
                pod_index,
                cmd_index,
                ui_tx.clone(),
            ).await;
            
            match result {
                Ok(cmd_result) => {
                    // Send command output to UI
                    if template_cmd.capture_output && !cmd_result.stdout.is_empty() {
                        let _ = ui_tx.send(UIUpdate::CommandOutput {
                            pod_index,
                            command_index: cmd_index,
                            output: cmd_result.stdout.clone(),
                        }).await;
                    }
                    
                    let _ = ui_tx.send(UIUpdate::CommandCompleted {
                        pod_index,
                        command_index: cmd_index,
                        success: true,
                    }).await;
                    
                    command_results.push(cmd_result);
                },
                Err(e) => {
                    let _ = ui_tx.send(UIUpdate::CommandCompleted {
                        pod_index,
                        command_index: cmd_index,
                        success: false,
                    }).await;
                    
                    if template_cmd.ignore_failure {
                        let _ = ui_tx.send(UIUpdate::CommandOutput {
                            pod_index,
                            command_index: cmd_index,
                            output: format!("⚠️ Command failed (ignored): {}", e),
                        }).await;
                        
                        command_results.push(CommandResult {
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                            exit_code: -1,
                        });
                    } else {
                        return Err(anyhow!("Command failed on pod {}: {}", pod.name, e));
                    }
                }
            }
        }
        
        // Download output files with UI updates
        let _ = ui_tx.send(UIUpdate::PodStatusChanged {
            pod_index,
            status: PodStatus::DownloadingFiles { current: 0, total: template.output_files.len() },
        }).await;
        
        for (file_index, output_pattern) in template.output_files.iter().enumerate() {
            let _ = ui_tx.send(UIUpdate::PodStatusChanged {
                pod_index,
                status: PodStatus::DownloadingFiles { 
                    current: file_index + 1, 
                    total: template.output_files.len() 
                },
            }).await;
            
            let files = self.download_files_from_pod(
                output_pattern,
                execution,
                pod,
            ).await?;
            
            // Update UI with downloaded files
            for file in &files {
                let _ = ui_tx.send(UIUpdate::FileDownloaded {
                    pod_index,
                    file: file.clone(),
                }).await;
            }
            
            downloaded_files.extend(files);
        }
        
        if downloaded_files.is_empty() && template.output_files.iter().any(|f| f.required) {
            return Err(anyhow!("No required output files found on pod {}", pod.name));
        }
        
        Ok(PodExecutionResult {
            pod_name: pod.name.clone(),
            pod_namespace: pod.namespace.clone(),
            command_results,
            downloaded_files,
            success: true,
        })
    }
    
    async fn execute_command_on_pod_with_ui(
        &self,
        template_cmd: &TemplateCommand,
        execution: &TemplateExecution,
        pod: &PodInfo,
        pod_index: usize,
        command_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<CommandResult> {
        // Substitute template variables in command
        let mut resolved_command = Vec::new();
        for arg in &template_cmd.command {
            let mut resolved_arg = arg.clone();
            
            // Replace template variables
            for (param_name, value) in &execution.arguments {
                let placeholder = format!("{{{{{}}}}}", param_name);
                resolved_arg = resolved_arg.replace(&placeholder, value);
            }
            
            resolved_command.push(resolved_arg);
        }
        
        // Handle built-in wait command with UI progress updates
        if resolved_command[0] == "wait" && resolved_command.len() == 2 {
            return self.handle_wait_command_with_ui(
                &resolved_command[1],
                pod_index,
                command_index,
                ui_tx,
            ).await;
        }
        
        // Execute command using kubectl exec
        let mut kubectl_cmd = AsyncCommand::new("kubectl");
        kubectl_cmd
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("--");
        
        kubectl_cmd.args(&resolved_command);
        
        let output = kubectl_cmd.output().await?;
        
        Ok(CommandResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
    
    async fn handle_wait_command_with_ui(
        &self,
        duration_str: &str,
        pod_index: usize,
        command_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<CommandResult> {
        let seconds = self.parse_duration(duration_str)?;
        
        // Update UI to show waiting status
        let _ = ui_tx.send(UIUpdate::PodStatusChanged {
            pod_index,
            status: PodStatus::WaitingLocal { 
                duration: duration_str.to_string(), 
                progress: 0.0 
            },
        }).await;
        
        let _ = ui_tx.send(UIUpdate::CommandOutput {
            pod_index,
            command_index,
            output: format!("⏳ Waiting {} ({} seconds) locally...", duration_str, seconds),
        }).await;
        
        // Progress updates during wait
        for i in 0..seconds {
            sleep(Duration::from_secs(1)).await;
            
            let progress = (i + 1) as f64 / seconds as f64;
            
            // Update progress every second for UI
            let _ = ui_tx.send(UIUpdate::PodStatusChanged {
                pod_index,
                status: PodStatus::WaitingLocal { 
                    duration: duration_str.to_string(), 
                    progress 
                },
            }).await;
            
            // Update progress bar in command output every 10 seconds
            if i % 10 == 0 || i == seconds - 1 {
                let progress_percent = (progress * 100.0) as u32;
                let elapsed_min = (i + 1) / 60;
                let elapsed_sec = (i + 1) % 60;
                
                let bar_width = 40;
                let filled = (progress * bar_width as f64) as usize;
                let empty = bar_width - filled;
                let bar = format!("{}{}",
                    "█".repeat(filled),
                    "░".repeat(empty)
                );
                
                let _ = ui_tx.send(UIUpdate::CommandOutput {
                    pod_index,
                    command_index,
                    output: format!("⏳ [{}] {}% ({:02}:{:02} elapsed)", 
                                   bar, progress_percent, elapsed_min, elapsed_sec),
                }).await;
            }
        }
        
        Ok(CommandResult {
            success: true,
            stdout: format!("Waited {} seconds locally", seconds),
            stderr: String::new(),
            exit_code: 0,
        })
    }
    
    async fn download_files_from_pod(
        &self,
        output_pattern: &OutputFilePattern,
        execution: &TemplateExecution,
        pod: &PodInfo,
    ) -> Result<Vec<DownloadedFile>> {
        // First, list files matching the pattern on the pod
        let list_cmd = AsyncCommand::new("kubectl")
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(format!("ls -1 {} 2>/dev/null || true", output_pattern.pattern))
            .output()
            .await?;
        
        let file_list = String::from_utf8_lossy(&list_cmd.stdout);
        let files: Vec<&str> = file_list
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        
        if files.is_empty() {
            if output_pattern.required {
                return Err(anyhow!(
                    "Required output files not found on pod {}: {}",
                    pod.name,
                    output_pattern.pattern
                ));
            } else {
                return Ok(Vec::new());
            }
        }
        
        println!("     Found {} files matching pattern: {}", files.len(), output_pattern.pattern);
        
        let mut downloaded_files = Vec::new();
        
        // Create pod-specific output directory
        let pod_output_dir = execution.output_dir
            .join(&pod.namespace)
            .join(&pod.name);
        std::fs::create_dir_all(&pod_output_dir)?;
        
        // Download each file
        for remote_file in files {
            let file_name = Path::new(remote_file)
                .file_name()
                .ok_or_else(|| anyhow!("Invalid file path: {}", remote_file))?
                .to_string_lossy();
            
            let local_file_path = pod_output_dir.join(file_name.as_ref());
            
            println!("       📥 Downloading: {} -> {}", remote_file, local_file_path.display());
            
            // Use kubectl cp to download file
            let cp_result = AsyncCommand::new("kubectl")
                .arg("cp")
                .arg("-n")
                .arg(&pod.namespace)
                .arg(format!("{}:{}", pod.name, remote_file))
                .arg(&local_file_path)
                .output()
                .await?;
            
            if !cp_result.status.success() {
                let error = String::from_utf8_lossy(&cp_result.stderr);
                return Err(anyhow!("Failed to download file {}: {}", remote_file, error));
            }
            
            // Verify file was downloaded
            if local_file_path.exists() {
                let file_size = std::fs::metadata(&local_file_path)?.len();
                println!("       ✅ Downloaded: {} ({} bytes)", local_file_path.display(), file_size);
                
                downloaded_files.push(DownloadedFile {
                    remote_path: remote_file.to_string(),
                    local_path: local_file_path,
                    file_type: output_pattern.file_type.clone(),
                    size_bytes: file_size,
                });
            } else {
                return Err(anyhow!("File download failed: {}", local_file_path.display()));
            }
        }
        
        Ok(downloaded_files)
    }
}

#[derive(Debug, Clone)]
pub struct TemplateExecutionResult {
    pub execution_id: String,
    pub template_name: String,
    pub pod_results: Vec<PodExecutionResult>,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PodExecutionResult {
    pub pod_name: String,
    pub pod_namespace: String,
    pub command_results: Vec<CommandResult>,
    pub downloaded_files: Vec<DownloadedFile>,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct DownloadedFile {
    pub remote_path: String,
    pub local_path: PathBuf,
    pub file_type: FileType,
    pub size_bytes: u64,
}
```

### 6. Interactive Terminal UI Implementation

```rust
// src/ui/template_ui.rs
use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame, Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};

pub async fn run_template_ui(
    ui_state: Arc<Mutex<TemplateUIState>>,
    mut ui_rx: mpsc::Receiver<UIUpdate>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let result = run_ui_loop(&mut terminal, ui_state, &mut ui_rx, &mut list_state).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_ui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ui_state: Arc<Mutex<TemplateUIState>>,
    ui_rx: &mut mpsc::Receiver<UIUpdate>,
    list_state: &mut ListState,
) -> Result<()> {
    let mut last_render = Instant::now();
    let render_interval = Duration::from_millis(100); // 10 FPS

    loop {
        // Handle input events
        if let Ok(true) = event::poll(Duration::from_millis(50)) {
            match event::read()? {
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            // Check if execution is still running
                            let state = ui_state.lock().await;
                            if state.is_execution_complete() {
                                break;
                            } else {
                                // Show confirmation dialog
                                drop(state);
                                if show_quit_confirmation(terminal).await? {
                                    break;
                                }
                            }
                        }
                        KeyCode::Up => {
                            let state = ui_state.lock().await;
                            let current = list_state.selected().unwrap_or(0);
                            if current > 0 {
                                list_state.select(Some(current - 1));
                            }
                        }
                        KeyCode::Down => {
                            let state = ui_state.lock().await;
                            let current = list_state.selected().unwrap_or(0);
                            if current < state.pods.len().saturating_sub(1) {
                                list_state.select(Some(current + 1));
                            }
                        }
                        KeyCode::Enter => {
                            // Toggle log view for selected pod
                            let mut state = ui_state.lock().await;
                            state.show_logs = !state.show_logs;
                            if let Some(selected) = list_state.selected() {
                                state.selected_pod_index = selected;
                            }
                        }
                        KeyCode::PageUp => {
                            let mut state = ui_state.lock().await;
                            if state.show_logs && state.log_scroll_offset > 0 {
                                state.log_scroll_offset = state.log_scroll_offset.saturating_sub(10);
                            }
                        }
                        KeyCode::PageDown => {
                            let mut state = ui_state.lock().await;
                            if state.show_logs {
                                state.log_scroll_offset += 10;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Down(_) => {
                            // Handle pod selection by mouse click
                            let state = ui_state.lock().await;
                            let pod_list_area = calculate_pod_list_area(terminal.size()?);
                            
                            if mouse.column >= pod_list_area.x 
                                && mouse.column < pod_list_area.x + pod_list_area.width
                                && mouse.row >= pod_list_area.y + 1 // Account for border
                                && mouse.row < pod_list_area.y + pod_list_area.height - 1 {
                                
                                let clicked_index = (mouse.row - pod_list_area.y - 1) as usize;
                                if clicked_index < state.pods.len() {
                                    list_state.select(Some(clicked_index));
                                    drop(state);
                                    let mut state = ui_state.lock().await;
                                    state.show_logs = true;
                                    state.selected_pod_index = clicked_index;
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            let mut state = ui_state.lock().await;
                            if state.show_logs && state.log_scroll_offset > 0 {
                                state.log_scroll_offset = state.log_scroll_offset.saturating_sub(3);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            let mut state = ui_state.lock().await;
                            if state.show_logs {
                                state.log_scroll_offset += 3;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Handle UI updates from template execution
        while let Ok(update) = ui_rx.try_recv() {
            let mut state = ui_state.lock().await;
            state.handle_update(update);
        }

        // Render UI
        if last_render.elapsed() >= render_interval {
            let state = ui_state.lock().await;
            terminal.draw(|f| render_template_ui(f, &state, list_state))?;
            last_render = Instant::now();
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    Ok(())
}

fn render_template_ui(
    f: &mut Frame,
    state: &TemplateUIState,
    list_state: &mut ListState,
) {
    let size = f.size();

    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Header
            Constraint::Min(10),    // Pod list
            Constraint::Length(if state.show_logs { 15 } else { 0 }), // Log panel
            Constraint::Length(2),  // Footer
        ])
        .split(size);

    // Render header
    render_header(f, chunks[0], state);

    // Render pod list
    render_pod_list(f, chunks[1], state, list_state);

    // Render log panel if enabled
    if state.show_logs {
        render_log_panel(f, chunks[2], state);
    }

    // Render footer
    render_footer(f, chunks[3], state);
}

fn render_header(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    let header_text = vec![
        Line::from(vec![
            Span::styled("Template: ", Style::default().fg(Color::Cyan)),
            Span::styled(&state.execution.template_name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" - ", Style::default().fg(Color::Gray)),
            Span::styled(&format!("Arguments: {:?}", state.execution.arguments), Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Execution ID: ", Style::default().fg(Color::Cyan)),
            Span::styled(&state.execution.execution_id, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Output Directory: ", Style::default().fg(Color::Cyan)),
            Span::styled(state.execution.output_dir.display().to_string(), Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Elapsed: ", Style::default().fg(Color::Cyan)),
            Span::styled(format_duration(state.start_time.elapsed()), Style::default().fg(Color::Yellow)),
            Span::styled(" | Status: ", Style::default().fg(Color::Gray)),
            Span::styled(format_overall_status(&state.pods), Style::default().fg(Color::Green)),
        ]),
    ];

    let header = Paragraph::new(header_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Wake Template Execution")
            .title_alignment(Alignment::Center))
        .wrap(Wrap { trim: true });

    f.render_widget(header, area);
}

fn render_pod_list(f: &mut Frame, area: Rect, state: &TemplateUIState, list_state: &mut ListState) {
    let items: Vec<ListItem> = state.pods.iter().enumerate().map(|(i, pod)| {
        let status_icon = match &pod.status {
            PodStatus::Starting => "🔄",
            PodStatus::Running { .. } => "⚡",
            PodStatus::WaitingLocal { .. } => "⏳", 
            PodStatus::DownloadingFiles { .. } => "📥",
            PodStatus::Completed => "✅",
            PodStatus::Failed { .. } => "❌",
        };

        let progress_bar = create_progress_bar(pod);
        let status_text = format_pod_status(&pod.status);

        let line = Line::from(vec![
            Span::styled(status_icon, Style::default().fg(Color::White)),
            Span::styled(format!(" {}/{} ", pod.pod_info.namespace, pod.pod_info.name), 
                        Style::default().fg(Color::Cyan)),
            Span::styled(progress_bar, Style::default().fg(Color::Green)),
            Span::styled(format!(" {}", status_text), Style::default().fg(Color::Gray)),
        ]);

        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!("Pod Execution Status ({} pods)", state.pods.len())))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, list_state);
}

fn render_log_panel(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    if let Some(pod) = state.pods.get(state.selected_pod_index) {
        let logs = pod.command_logs.iter()
            .skip(state.log_scroll_offset)
            .take(area.height.saturating_sub(2) as usize)
            .map(|log| {
                let timestamp = log.timestamp.format("%H:%M:%S");
                let status_icon = match log.status {
                    CommandStatus::Running => "⚡",
                    CommandStatus::Completed => "✓",
                    CommandStatus::Failed => "✗",
                    CommandStatus::Waiting => "⏳",
                };
                
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(format!("[{}] ", timestamp), Style::default().fg(Color::Gray)),
                        Span::styled(format!("{} ", status_icon), Style::default().fg(Color::White)),
                        Span::styled(&log.description, Style::default().fg(Color::Cyan)),
                    ])
                ];
                
                if let Some(ref output) = log.output {
                    for output_line in output.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default()),
                            Span::styled(output_line, Style::default().fg(Color::White)),
                        ]));
                    }
                }
                
                lines
            })
            .flatten()
            .collect::<Vec<_>>();

        let log_text = Text::from(logs);
        let log_panel = Paragraph::new(log_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .title(format!("Pod Logs ({})", pod.pod_info.name)))
            .wrap(Wrap { trim: true });

        f.render_widget(log_panel, area);
    }
}

fn render_footer(f: &mut Frame, area: Rect, state: &TemplateUIState) {
    let controls = if state.show_logs {
        "↑↓ Select Pod | Enter Toggle Logs | PgUp/PgDn Scroll | Esc/q Quit"
    } else {
        "↑↓ Select Pod | Enter View Logs | Click Pod | q Quit"
    };

    let footer = Paragraph::new(controls)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(footer, area);
}

// Helper functions
fn create_progress_bar(pod: &PodExecutionState) -> String {
    let total_steps = pod.total_commands + 1; // +1 for file download
    let current_step = match &pod.status {
        PodStatus::Starting => 0,
        PodStatus::Running { command_index } => *command_index,
        PodStatus::WaitingLocal { .. } => pod.current_command_index,
        PodStatus::DownloadingFiles { .. } => pod.total_commands,
        PodStatus::Completed => total_steps,
        PodStatus::Failed { .. } => pod.current_command_index,
    };

    let filled = current_step;
    let empty = total_steps.saturating_sub(current_step);
    
    format!("{}{}",
        "●".repeat(filled),
        "○".repeat(empty)
    )
}

fn format_pod_status(status: &PodStatus) -> String {
    match status {
        PodStatus::Starting => "STARTING".to_string(),
        PodStatus::Running { command_index } => format!("RUNNING [Command {}/{}]", command_index + 1, command_index + 1),
        PodStatus::WaitingLocal { duration, progress } => {
            format!("WAITING {} ({:.0}%)", duration, progress * 100.0)
        },
        PodStatus::DownloadingFiles { current, total } => {
            format!("DOWNLOADING [{}/{}]", current, total)
        },
        PodStatus::Completed => "COMPLETED".to_string(),
        PodStatus::Failed { error } => format!("FAILED: {}", error),
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let hours = mins / 60;
    
    if hours > 0 {
        format!("{}h{}m{}s", hours, mins % 60, secs % 60)
    } else if mins > 0 {
        format!("{}m{}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

fn format_overall_status(pods: &[PodExecutionState]) -> String {
    let completed = pods.iter().filter(|p| matches!(p.status, PodStatus::Completed)).count();
    let failed = pods.iter().filter(|p| matches!(p.status, PodStatus::Failed { .. })).count();
    let running = pods.len() - completed - failed;
    
    if failed > 0 {
        format!("{} completed, {} failed, {} running", completed, failed, running)
    } else if running > 0 {
        format!("{}/{} completed, {} running", completed, pods.len(), running)
    } else {
        "All pods completed".to_string()
    }
}

async fn show_quit_confirmation(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<bool> {
    loop {
        terminal.draw(|f| {
            let area = centered_rect(50, 20, f.size());
            
            f.render_widget(Clear, area);
            
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Confirm Quit")
                .title_alignment(Alignment::Center);
            
            let text = Paragraph::new("Template execution is still running.\n\nAre you sure you want to quit?\n\n[Y]es / [N]o")
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            
            f.render_widget(text, area);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => return Ok(false),
                _ => {}
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn calculate_pod_list_area(terminal_size: Rect) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Header
            Constraint::Min(10),    // Pod list  
            Constraint::Length(0),  // Log panel (collapsed)
            Constraint::Length(2),  // Footer
        ])
        .split(terminal_size);
    
    chunks[1]
}

// Additional state management types
#[derive(Debug, Clone)]
pub enum CommandStatus {
    Running,
    Completed,
    Failed,
    Waiting,
}

impl TemplateUIState {
    pub fn new(
        execution: TemplateExecution,
        pods: Vec<PodInfo>,
        template: Template,
    ) -> Self {
        let pod_states = pods.into_iter().map(|pod_info| {
            PodExecutionState {
                pod_info,
                status: PodStatus::Starting,
                current_command_index: 0,
                total_commands: template.commands.len(),
                command_logs: Vec::new(),
                downloaded_files: Vec::new(),
                error_message: None,
            }
        }).collect();

        Self {
            execution,
            pods: pod_states,
            selected_pod_index: 0,
            log_scroll_offset: 0,
            show_logs: false,
            start_time: Instant::now(),
        }
    }

    pub fn handle_update(&mut self, update: UIUpdate) {
        match update {
            UIUpdate::PodStatusChanged { pod_index, status } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.status = status;
                }
            }
            UIUpdate::CommandStarted { pod_index, command_index, description } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.current_command_index = command_index;
                    pod.command_logs.push(CommandLog {
                        timestamp: chrono::Local::now(),
                        command_index,
                        description,
                        output: None,
                        status: CommandStatus::Running,
                    });
                }
            }
            UIUpdate::CommandOutput { pod_index, command_index, output } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    if let Some(log) = pod.command_logs.iter_mut()
                        .find(|log| log.command_index == command_index) {
                        if let Some(ref mut existing_output) = log.output {
                            existing_output.push('\n');
                            existing_output.push_str(&output);
                        } else {
                            log.output = Some(output);
                        }
                    }
                }
            }
            UIUpdate::CommandCompleted { pod_index, command_index, success } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    if let Some(log) = pod.command_logs.iter_mut()
                        .find(|log| log.command_index == command_index) {
                        log.status = if success { CommandStatus::Completed } else { CommandStatus::Failed };
                    }
                }
            }
            UIUpdate::FileDownloaded { pod_index, file } => {
                if let Some(pod) = self.pods.get_mut(pod_index) {
                    pod.downloaded_files.push(file);
                }
            }
            UIUpdate::ExecutionCompleted => {
                // Mark any remaining pods as completed
                for pod in &mut self.pods {
                    if !matches!(pod.status, PodStatus::Completed | PodStatus::Failed { .. }) {
                        pod.status = PodStatus::Completed;
                    }
                }
            }
        }
    }

    pub fn is_execution_complete(&self) -> bool {
        self.pods.iter().all(|pod| {
            matches!(pod.status, PodStatus::Completed | PodStatus::Failed { .. })
        })
    }
}
```