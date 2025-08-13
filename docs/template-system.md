# Wake Template System Documentation

## Overview

The Wake Template System provides an interactive terminal UI for executing predefined diagnostic commands across multiple Kubernetes pods in parallel. It features real-time progress tracking, clickable pod selection, and live command output viewing.

## Key Features

### 🚀 Parallel Execution
- Execute templates on multiple pods simultaneously
- Real-time progress tracking for each pod
- Independent error handling per pod

### 🎯 Interactive Terminal UI
- Live status updates with progress bars
- Clickable pod selection
- Real-time command output viewing
- Mouse and keyboard navigation

### 📊 Progress Visualization
- Visual progress bars (●●●○○○○○○○)
- Status indicators (🔄 Starting, ⚡ Running, ✅ Completed, ❌ Failed)
- Command-by-command execution tracking
- File download progress

### 🔧 Built-in Templates
- **JFR**: Java Flight Recorder profiling
- **heap-dump**: Java heap memory dumps
- **thread-dump**: Java thread stack traces

## Usage Examples

### Basic Template Execution
```bash
# Execute heap dump with interactive UI
wake -t heap-dump 1234

# Execute JFR recording for 5 minutes
wake -t jfr 1234 5m

# Execute thread dump on specific pods
wake -n production -p "java-app.*" -t thread-dump 1234
```

### Advanced Usage
```bash
# Custom output directory
wake -t jfr 1234 30s --template-outdir ./diagnostics

# Disable UI for headless environments
wake -t heap-dump 1234 --no-template-ui

# Multi-namespace execution
wake -A -p "service-.*" -t jfr 1234 2m
```

## Terminal UI Controls

### Navigation
- **↑/↓ Arrow Keys**: Select pods
- **Enter**: Toggle log view for selected pod
- **Mouse Click**: Click on pod to view logs
- **PgUp/PgDn**: Scroll through logs

### Display
- **Progress Bars**: Visual command progress
- **Status Colors**: 
  - Gray: Starting
  - Yellow: Running
  - Green: Completed
  - Red: Failed
- **Real-time Updates**: Live command output and progress

### Exit
- **q/Esc**: Quit (with confirmation if running)
- **Ctrl+C**: Force quit

## Template Structure

### Template Definition
```rust
Template {
    name: "jfr",
    description: "Java Flight Recorder profiling",
    parameters: [
        Parameter { name: "pid", type: Integer, required: true },
        Parameter { name: "time", type: Duration, required: true }
    ],
    commands: [
        Command { description: "Check jcmd availability", ... },
        Command { description: "Start JFR recording", ... },
        Command { description: "Wait for completion", command: ["wait", "{{time}}"] },
        Command { description: "List output files", ... }
    ],
    output_files: [
        OutputFile { pattern: "/tmp/jfr_*.jfr", required: true }
    ]
}
```

### Wait Command
The built-in `wait` command enables timing-dependent operations:
```bash
# Wait 30 seconds with progress bar
wait 30s

# Wait 5 minutes with visual progress
wait 5m

# Wait 1 hour
wait 1h
```

## UI State Management

### Pod States
- **Starting**: Initializing execution
- **Running**: Executing commands
- **WaitingLocal**: Local wait with progress
- **DownloadingFiles**: Retrieving output files
- **Completed**: Successfully finished
- **Failed**: Error occurred

### Log Tracking
Each pod maintains:
- Command execution history
- Real-time output capture
- Timestamp tracking
- Error state management

## Performance Benefits

### Parallel vs Sequential
**Traditional Sequential Execution:**
- 10 pods × 5 minute JFR = 50 minutes total

**Wake Parallel Execution:**
- 10 pods × 5 minute JFR = 5 minutes total

### Resource Efficiency
- Non-blocking wait commands
- Concurrent file downloads
- Minimal memory footprint
- Optimized rendering (10 FPS)

## File Organization

### Output Structure
```
wake-templates-20250812_143022/
├── production/
│   ├── java-app-1/
│   │   └── jfr_java-app-1_1234_20250812_143022.jfr
│   ├── java-app-2/
│   │   └── jfr_java-app-2_1234_20250812_143022.jfr
│   └── java-app-3/
│       └── jfr_java-app-3_1234_20250812_143022.jfr
└── execution-summary.txt
```

### File Types
- **Binary**: `.jfr`, `.hprof` files
- **Text**: Thread dumps, logs
- **Archive**: Compressed collections

## Integration with Wake

### CLI Arguments
```rust
#[arg(short = 't', long = "template")]
template: Option<Vec<String>>,

#[arg(long = "template-outdir")]
template_outdir: Option<PathBuf>,

#[arg(long = "no-template-ui")]
no_template_ui: bool,
```

### Execution Flow
1. Parse template arguments
2. Select target pods
3. Initialize UI state
4. Execute in parallel with updates
5. Download output files
6. Display results summary

## Error Handling

### Graceful Degradation
- Individual pod failures don't stop others
- Optional commands can fail without breaking execution
- UI shows detailed error messages
- Partial results are still collected

### Recovery Options
- Retry failed commands
- Continue with successful pods
- Download partial results
- Detailed error logging

## Customization

### Creating Custom Templates
Templates can be defined in YAML format:
```yaml
name: "custom-profile"
description: "Custom Java profiling"
parameters:
  - name: "pid"
    type: "Integer"
    required: true
commands:
  - description: "Start profiling"
    command: ["jcmd", "{{pid}}", "JFR.start"]
  - description: "Wait for completion"
    command: ["wait", "60s"]
```

### UI Theming
The terminal UI uses ratatui with customizable colors and styles:
- Header: Cyan titles, white text
- Progress: Green bars, yellow running state
- Logs: Gray timestamps, white output
- Errors: Red highlighting

## Troubleshooting

### Common Issues
1. **No pods found**: Check namespace and selector
2. **Template not found**: Verify template name
3. **Command failures**: Check required tools in pods
4. **File download errors**: Verify kubectl cp permissions

### Debug Mode
Enable verbose logging:
```bash
RUST_LOG=debug wake -t jfr 1234 5m
```

### Performance Tuning
- Adjust render interval for slower terminals
- Increase batch sizes for high-throughput scenarios
- Use `--no-template-ui` for automated environments