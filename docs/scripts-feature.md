# Wake Scripts Feature

The `--scripts` feature provides a powerful way to create, manage, and execute reusable shell scripts across multiple Kubernetes pods simultaneously.

## Overview

Wake Scripts allows you to:
- **Create** reusable scripts with parameterized arguments
- **Save** scripts locally for repeated use
- **Execute** scripts across multiple pods with a beautiful TUI
- **Collect** outputs and merge/separate results automatically

## Quick Start

```bash
# Open the script selector (shows New, ALL, and saved scripts)
wake --scripts

# Create a new script
wake --scripts New

# List all saved scripts with preview
wake --scripts ALL

# Execute a saved script on pods
wake --scripts my_script -n production "app-.*"
```

## Usage

### Creating a New Script

```bash
wake --scripts New
```

This opens the **Script Editor TUI** with a template:

```
╔══════════════════════════════════════════════════════════════╗
║  📝 Script: <unnamed>                                        ║
╚══════════════════════════════════════════════════════════════╝
┌─────────────────────────────────────┬────────────────────────┐
│ Script Content                      │ Arguments              │
│                                     │                        │
│ #!/bin/sh                           │   No arguments         │
│ # Wake Script Template              │   Press 'a' or F3      │
│ # Description: <YOUR_DESC>          │   to add one           │
│ ...                                 │                        │
└─────────────────────────────────────┴────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│ F5 Save  F2 Rename  F3 Add Arg  Tab Switch  Esc Exit         │
└──────────────────────────────────────────────────────────────┘
```

### Script Editor Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `F5` | Save the script |
| `F2` | Rename the script |
| `F3` | Add a new argument |
| `Tab` | Switch between Script Content and Arguments panels |
| `Esc` | Exit the editor |

### Arguments Panel Shortcuts

| Key | Action |
|-----|--------|
| `a` | Add a new argument |
| `e` or `Enter` | Edit selected argument |
| `d` or `Delete` | Delete selected argument |
| `↑/↓` | Navigate through arguments |

### Argument Dialog

When adding/editing an argument:

| Key | Action |
|-----|--------|
| `Tab` | Move to next field |
| `Enter` | Save the argument |
| `Space` | Toggle "Required" checkbox (when on that field) |
| `Esc` | Cancel |
| `←/→` | Move cursor within field |

## Listing All Scripts

```bash
wake --scripts ALL
```

Opens the **Script List TUI**:

```
╔══════════════════════════════════════════════════════════════╗
║  📜 Saved Scripts (3 total)                                  ║
╚══════════════════════════════════════════════════════════════╝
┌─────────────────────────┬────────────────────────────────────┐
│ Scripts                 │ Preview                            │
│                         │                                    │
│ ▶ 📄 grep_logs (2 args) │ Name: grep_logs                    │
│   📄 disk_check         │                                    │
│   📄 memory_stats       │ Arguments:                         │
│                         │   • pattern* (required)            │
│                         │   • path = "/var/log/"             │
│                         │                                    │
│                         │ Script Content:                    │
│                         │ ─────────────────────              │
│                         │ #!/bin/sh                          │
│                         │ grep -r "${pattern}" ${path}       │
└─────────────────────────┴────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│ ↑↓ Navigate  Enter/e Edit  x Execute  d Delete  n New  q Quit│
└──────────────────────────────────────────────────────────────┘
```

### Script List Shortcuts

| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate scripts |
| `Enter` or `e` | Edit selected script |
| `x` | Execute selected script |
| `d` or `Delete` | Delete script (with confirmation) |
| `n` | Create new script |
| `p` | Toggle preview panel |
| `q` or `Esc` | Exit |

## Executing Scripts

### From Selector
```bash
wake --scripts my_script -n namespace "pod-pattern"
```

### From List UI
Press `x` on a selected script in the list view.

### Execution Flow

1. **Argument Collection** - If script has arguments, you'll be prompted:

```
╔═══════════════════════════════════════╗
║  📝 SCRIPT ARGUMENTS                  ║
╚═══════════════════════════════════════╝
┌─────────────── Argument 1/2 ──────────┐
│  📌 pattern  REQUIRED                 │
│                                       │
│  📋 The pattern to search for         │
│  💡 Default: "error"                  │
└───────────────────────────────────────┘
┌─────────────── ✏️ Enter Value ────────┐
│ ▌                                     │
└───────────────────────────────────────┘
         ● ○    Enter Submit  Esc Cancel
```

2. **Execution Progress** - Watch scripts run on each pod:

```
╔══════════════════════════════════════════════════════════════╗
║  🔄 EXECUTING: grep_logs                                     ║
╚══════════════════════════════════════════════════════════════╝
┌─────────────── Progress ─────────────────────────────────────┐
│ ████████████████░░░░░░░░░░  3 / 5 pods  │  ✅ 2  ❌ 1        │
└──────────────────────────────────────────────────────────────┘
┌─── 📦 Pods (5) ───┬───────────── ✅ Output ──────────────────┐
│ ▶ ✅ app-pod-1    │ Found 15 matches:                        │
│   ✅ app-pod-2    │ /var/log/app.log:error: timeout          │
│   🔄 app-pod-3    │ /var/log/app.log:error: connection       │
│   ⏳ app-pod-4    │ ...                                      │
│   ⏳ app-pod-5    │                                          │
└───────────────────┴──────────────────────────────────────────┘
```

3. **Save Results** - Choose how to save output:

```
┌─────────── 💾 Save Execution Results ────────────┐
│                                                  │
│  📊 Execution Complete: 5 pods │ ✅ 4 │ ❌ 1     │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │ ◉ Merge into single file                   │  │
│  │   → merged_output.txt                      │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │ ○ Save separate files                      │  │
│  │   → wake_script_output_20251203_120000/    │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│     ↑↓ Switch  Enter/Y/N Confirm  Esc Skip       │
└──────────────────────────────────────────────────┘
```

## Script Arguments

Scripts support parameterized arguments that can be:

- **Required** - Must be provided at execution time
- **Optional** - Use default value if not provided
- **With Description** - Help text shown during input
- **With Default Value** - Pre-filled if user presses Enter

### Using Arguments in Scripts

Use `${arg_name}` or `$arg_name` syntax:

```bash
#!/bin/sh
# Search for pattern in logs
grep -r "${pattern}" ${search_path:-/var/log/}

# With default fallback
echo "Searching in: ${search_path:-/var/log/}"
```

## Output Files

### Merged Output (`merged_output.txt`)

```
╔══════════════════════════════════════════════════════════════╗
║  🚀 Wake Script Execution Report                             ║
╚══════════════════════════════════════════════════════════════╝

📜 Script: grep_logs
🕐 Executed: 2025-12-03 12:00:00
📊 Total Pods: 5
✅ Success: 4 | ❌ Failed: 1

────────────────────────────────────────────────────────────────
📦 Pod 1: production/app-pod-1
────────────────────────────────────────────────────────────────
Status: ✅ SUCCESS

Found 15 matches in /var/log/app.log
...
```

### Separate Files

```
wake_script_output_20251203_120000/
├── production_app-pod-1.txt
├── production_app-pod-2.txt
├── production_app-pod-3.txt
├── production_app-pod-4.txt
└── production_app-pod-5.txt
```

## Script Storage

Scripts are stored in:
- **Linux/macOS**: `~/.config/wake/scripts/`
- **Windows**: `%APPDATA%\wake\scripts\`

Each script is saved as a JSON file with metadata:
- Script name
- Content
- Arguments (name, description, default, required)
- Created/Updated timestamps

## Examples

### Example 1: Log Search Script

```bash
# Create a script to search logs
wake --scripts New
```

Script content:
```bash
#!/bin/sh
# Search for errors in application logs
echo "Searching for: ${pattern}"
grep -rn "${pattern}" /var/log/app/ 2>/dev/null || echo "No matches found"
```

Arguments:
- `pattern` (required): "The text pattern to search for"

### Example 2: System Health Check

```bash
#!/bin/sh
# Quick system health check
echo "=== Disk Usage ==="
df -h /

echo ""
echo "=== Memory ==="
free -m

echo ""
echo "=== Top Processes ==="
ps aux --sort=-%mem | head -5
```

### Example 3: Application Diagnostics

```bash
#!/bin/sh
# Application diagnostics
APP_NAME="${app_name:-myapp}"

echo "=== Process Status ==="
pgrep -a "$APP_NAME" || echo "Process not running"

echo ""
echo "=== Recent Logs ==="
tail -n ${log_lines:-50} /var/log/$APP_NAME/*.log 2>/dev/null

echo ""
echo "=== Open Connections ==="
netstat -an | grep "${port:-8080}" | wc -l
```

Arguments:
- `app_name` (optional, default: "myapp"): Application name
- `log_lines` (optional, default: "50"): Number of log lines
- `port` (optional, default: "8080"): Port to check

## Best Practices

1. **Use descriptive names** - `check_disk_usage` instead of `script1`
2. **Add descriptions to arguments** - Help future you understand what each arg does
3. **Provide sensible defaults** - Make scripts runnable with minimal input
4. **Handle errors gracefully** - Use `|| echo "..."` for commands that might fail
5. **Test on single pod first** - Use `-s 1` to sample one pod before running on all

## Troubleshooting

### Script not found
```bash
wake --scripts ALL  # Check if script exists
```

### No pods matched
```bash
# Check your pod selector
wake -n namespace -L  # List available pods
```

### Permission denied
Ensure the script doesn't require root access or use `sudo` (which won't work in containers).

### Timeout issues
Long-running scripts may timeout. Consider breaking them into smaller scripts.

## See Also

- [Template System](template-system.md) - For built-in templates like JFR, heap dumps
- [Monitor Feature](monitor-feature-design.md) - Real-time pod monitoring
