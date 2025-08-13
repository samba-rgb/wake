# Wake Template System

The Wake Template System provides a powerful way to execute predefined diagnostic and maintenance tasks across multiple Kubernetes pods. Templates allow you to automate common operations like generating heap dumps, thread dumps, JFR recordings, and other diagnostic procedures.

## Overview

Templates are reusable automation scripts that can:
- Execute sequences of commands on multiple pods simultaneously
- Generate diagnostic files (heap dumps, thread dumps, JFR recordings)
- Download generated files to your local machine
- Provide real-time progress monitoring with an interactive UI
- Clean up temporary files after execution

## Quick Start

```bash
# List available templates
wake --list-templates

# Execute JFR template on matching pods
wake --exec-template jfr --template-args 1234 30s

# Execute heap dump template
wake --exec-template heap-dump --template-args 1234

# Execute thread dump template  
wake --exec-template thread-dump --template-args 1234
```

## Built-in Templates

### 1. JFR (Java Flight Recorder)
Generates performance profiles for Java applications.

**Usage:**
```bash
wake --exec-template jfr --template-args <pid> <duration>
```

**Parameters:**
- `pid` (integer): Process ID to profile
- `duration` (string): Recording duration (e.g., "30s", "5m", "1h")

**Example:**
```bash
wake --exec-template jfr --template-args 1234 30s
```

**What it does:**
1. Checks if `jcmd` is available in the pod
2. Verifies the process exists
3. Starts JFR recording with profile settings
4. Waits for the specified duration
5. Downloads the generated `.jfr` file
6. Cleans up temporary files from pods

### 2. Heap Dump
Generates Java heap dumps for memory analysis.

**Usage:**
```bash
wake --exec-template heap-dump --template-args <pid>
```

**Parameters:**
- `pid` (integer): Process ID to dump

**Example:**
```bash
wake --exec-template heap-dump --template-args 1234
```

**What it does:**
1. Checks if `jmap` is available in the pod
2. Verifies the process exists  
3. Generates heap dump using `jmap`
4. Downloads the generated `.hprof` file
5. Cleans up temporary files from pods

### 3. Thread Dump
Generates Java thread dumps for deadlock and performance analysis.

**Usage:**
```bash
wake --exec-template thread-dump --template-args <pid>
```

**Parameters:**
- `pid` (integer): Process ID to dump

**Example:**
```bash
wake --exec-template thread-dump --template-args 1234
```

**What it does:**
1. Verifies the process exists
2. Generates thread dump using `jstack` (fallback to `jcmd`)
3. Downloads the generated `.txt` file
4. Cleans up temporary files from pods

## Interactive UI

The template system includes a powerful interactive UI that provides real-time monitoring of template execution:

### Features
- **Pod Overview**: See all selected pods and their execution status
- **Real-time Progress**: Track command execution progress with visual indicators
- **Resource Monitoring**: Live CPU and memory usage for each pod (updated every 2 seconds)
- **Command Logs**: Detailed output from each command execution
- **File Downloads**: Track downloaded files and their sizes
- **Error Handling**: Clear visibility into failed commands and errors

### UI Navigation
- **Tab/Shift+Tab**: Switch between pods
- **↑/↓**: Scroll through command logs
- **h**: Show help
- **q**: Quit application

### Status Indicators
- 🟡 **Starting**: Pod is initializing
- 🔵 **Running**: Executing commands
- ⏳ **Waiting**: Local wait period (e.g., JFR recording)
- 📥 **Downloading**: Downloading generated files
- ✅ **Completed**: Successfully finished
- ❌ **Failed**: Execution failed

### Resource Monitoring
The UI shows real-time resource usage for each pod:
- **CPU Usage**: Percentage with color coding (Green < 60%, Yellow 60-80%, Red > 80%)
- **Memory Usage**: Percentage and absolute values (e.g., "72.5% (1.2 GB/1.6 GB)")

## Template Execution Flow

1. **Pod Selection**: Uses the same pod selection logic as log tailing
2. **Template Validation**: Checks template exists and validates arguments
3. **Parallel Execution**: Runs template on all selected pods simultaneously
4. **Command Sequence**: Executes commands in order (excluding cleanup)
5. **File Download**: Downloads all generated files to local machine
6. **Cleanup**: Removes temporary files from pods
7. **Summary**: Shows execution results and output location

## Output Structure

Downloaded files are organized in a structured directory:

```
wake-templates-20250814_143022/
├── namespace1/
│   ├── pod1/
│   │   ├── jfr_1234_wake.jfr
│   │   └── heap_dump_1234_wake.hprof
│   └── pod2/
│       └── thread_dump_1234_wake.txt
└── namespace2/
    └── pod3/
        └── jfr_1234_wake.jfr
```

## Advanced Usage

### Custom Output Directory
```bash
wake --exec-template jfr --template-args 1234 30s --output-dir ./diagnostics
```

### Namespace Selection
```bash
# Specific namespace
wake -n production --exec-template heap-dump --template-args 1234

# All namespaces
wake -A --exec-template thread-dump --template-args 1234
```

### Pod Filtering
```bash
# Regex pod selection
wake api-* --exec-template jfr --template-args 1234 30s

# Resource-based selection
wake -r deployment/frontend --exec-template heap-dump --template-args 1234
```

## Error Handling

The template system provides robust error handling:

- **Command Failures**: Optionally ignored based on template configuration
- **Missing Tools**: Clear error messages when required tools are unavailable
- **Process Validation**: Ensures target processes exist before execution
- **File Download**: Handles missing files gracefully
- **Cleanup**: Always attempts cleanup even if main execution fails

## Best Practices

### Finding Process IDs
```bash
# List containers to find the right pod/container
wake -L

# Use kubectl to find Java processes
kubectl exec -n <namespace> <pod> -- ps aux | grep java
kubectl exec -n <namespace> <pod> -- jps
```

### Template Selection Guidelines
- **JFR**: Best for performance analysis, CPU profiling, and comprehensive diagnostics
- **Heap Dump**: Best for memory leak analysis and memory usage patterns  
- **Thread Dump**: Best for deadlock detection and thread analysis

### Resource Considerations
- **JFR**: Minimal overhead, can run on production systems
- **Heap Dump**: Can pause application briefly, use carefully in production
- **Thread Dump**: Very low overhead, safe for production use

### Timing Recommendations
- **JFR Duration**: 30s-5m for most issues, longer for intermittent problems
- **Multiple Samples**: Consider taking multiple thread dumps 10-30 seconds apart
- **Peak Times**: Capture diagnostics during high load periods when issues occur

## Troubleshooting

### Common Issues

**Template not found:**
```bash
# List available templates
wake --list-templates
```

**Process not found:**
```bash
# Check process exists
kubectl exec -n <namespace> <pod> -- ps aux | grep <process>
```

**Permission errors:**
```bash
# Ensure kubectl has proper permissions
kubectl auth can-i "*" "*" --as=system:serviceaccount:<namespace>:<serviceaccount>
```

**No files downloaded:**
- Check that the process ID is correct
- Verify the commands completed successfully in the UI logs
- Ensure adequate disk space in the pods

### Debug Mode
Add verbose logging to see detailed execution:
```bash
wake --exec-template jfr --template-args 1234 30s -v 2
```

## Template Configuration

Each template is defined with:
- **Metadata**: Name, description, parameters
- **Commands**: Sequence of shell commands to execute
- **Output Files**: Patterns for files to download
- **Validation**: Parameter type checking and regex validation
- **Error Handling**: Which commands can fail without stopping execution

### Parameter Types
- **Integer**: Validated as numbers (e.g., process IDs)
- **Duration**: Validated as time strings (e.g., "30s", "5m")
- **String**: Free-form text input

### File Patterns
Templates use glob patterns to find generated files:
- `/tmp/jfr_*.jfr` - Matches JFR recordings
- `/tmp/heap_dump_*.hprof` - Matches heap dumps
- `/tmp/thread_dump_*.txt` - Matches thread dumps

## Integration Examples

### CI/CD Integration
```bash
# Automated performance testing
wake -n testing --exec-template jfr --template-args $(get_java_pid) 60s
analyze_jfr_files wake-templates-*/testing/*/jfr_*.jfr
```

### Monitoring Integration
```bash
# Periodic diagnostics
while true; do
    wake -n production api-* --exec-template thread-dump --template-args $(get_java_pid)
    sleep 300
done
```

### Incident Response
```bash
# Quick diagnostic collection
wake -A --exec-template jfr --template-args 1234 30s
wake -A --exec-template heap-dump --template-args 1234
wake -A --exec-template thread-dump --template-args 1234
```

## Performance Impact

### Resource Usage
- **JFR**: < 1% CPU overhead, minimal memory impact
- **Heap Dump**: Brief pause (1-10s), temporary disk usage (≈ heap size)
- **Thread Dump**: < 0.1% CPU overhead, minimal memory impact

### Network Usage
- File downloads depend on diagnostic file sizes
- JFR files: 1-50 MB typically
- Heap dumps: 100 MB - several GB
- Thread dumps: 1-10 MB typically

### Parallel Execution
- Templates run on all selected pods simultaneously
- Network bandwidth scales with number of pods
- Consider batch processing for large deployments

## Future Enhancements

Planned improvements for the template system:
- **Custom Templates**: User-defined template creation
- **Template Marketplace**: Shared template repository
- **Scheduling**: Automated template execution
- **Integration**: Direct integration with analysis tools
- **Compression**: Automatic compression of large diagnostic files