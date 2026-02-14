---
sidebar_position: 3
---

# CLI Reference

This page provides a complete reference for all command-line options available in Wake.

```
Options:
  -n, --namespace <NAMESPACE>     Kubernetes namespace [default: default]
  -A, --all-namespaces            Show logs from all namespaces
  -c, --container <CONTAINER>     Container selector regex [default: .*]
  -k, --kubeconfig <KUBECONFIG>   Path to kubeconfig file
  -x, --context <CONTEXT>         Kubernetes context to use
  -t, --tail <TAIL>               Lines of logs to display from beginning [default: 10]
  -f, --follow                    Follow logs (stream in real time) [default: true]
  -i, --include <INCLUDE>         Filter logs using advanced pattern syntax (supports &&, ||, !, quotes, regex)
  -E, --exclude <EXCLUDE>         Exclude logs using advanced pattern syntax (supports &&, ||, !, quotes, regex)
  -T, --timestamps                Show timestamps in logs
  -o, --output <OUTPUT>           Output format (text, json, raw) [default: text]
  -w, --output-file <FILE>        Write logs to file (use with --ui for both file and UI)
  -r, --resource <RESOURCE>       Use specific resource type filter (pod, deployment, statefulset)
      --template <TEMPLATE>       Custom template for log output
      --since <SINCE>             Since time (e.g., 5s, 2m, 3h)
      --threads <THREADS>         Number of threads for log filtering
      --buffer-size <SIZE>        Number of log entries to keep in memory [default: 20000]
      --ui                        Enable interactive UI mode with dynamic filtering
      --dev                       Enable development mode (show internal logs)
  -v, --verbosity <VERBOSITY>     Verbosity level for debug output [default: 0]
  -L, --list-containers           List all containers in matched pods
      --all-containers            Show logs from all containers in pods
      --script-in <PATH>          Path to a script to run in each selected pod
      --script-outdir <DIR>       Directory to save script output tar
      --his [QUERY]               Show command history or search for commands
      --sample <NUMBER>           Randomly sample a subset of matching pods
  -h, --help                      Print help
  -V, --version                   Print version
```
