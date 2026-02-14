---
sidebar_position: 5
---

# Command History and Search

Wake automatically tracks your command history and provides intelligent search capabilities to help you find and reuse commands.

## Command History

To see your recent `wake` commands, run:

```bash
wake --his
```

This will show you:
- All `wake` commands you've run.
- Timestamps for when commands were executed.
- The working directory where commands were run.

## Intelligent Command Search

Wake includes a TF-IDF powered search engine to help you find the right command syntax.

```bash
# Search for commands related to configuration
wake --his "config"

# Search for UI-related commands
wake --his "ui mode"

# Search for filtering examples
wake --his "error logs"
```

### Search Features

- **Smart matching**: Finds commands based on meaning, not just exact text.
- **Contextual results**: Shows relevant commands with descriptions.
- **Example suggestions**: Provides related command categories when no exact match is found.
