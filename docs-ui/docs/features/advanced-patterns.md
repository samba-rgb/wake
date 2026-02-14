---
sidebar_position: 2
---

# Advanced Pattern Syntax

Wake supports sophisticated filtering patterns with logical operators to help you zero in on the exact logs you need.

## Logical Operators
- **`&&`**: Logical AND (logs must contain both patterns)
- **`||`**: Logical OR (logs can contain either pattern)
- **`!`**: Logical NOT (exclude logs with this pattern)
- **`()`**: Grouping for complex logic
- **`"text"`**: Exact text matching
- **`pattern`**: Regular expression matching

## Examples

### Basic Filtering
```bash
# Show only error logs
wake -n apps log-generator -i "error"

# Show both info and error logs
wake -n apps log-generator -i "info|error"
```

### Advanced Logical Operators
```bash
# Logical AND - logs must contain both "info" and "user"
wake -n apps log-generator -i 'info && "user"'

# Logical OR - logs containing either "info" or "error"
wake -n apps log-generator -i '"info" || "error"'

# Negation - exclude debug logs
wake -n apps log-generator -i '!debug'

# Complex combinations with grouping
wake -n apps log-generator -i '(info || error) && !"test"'
```
