# Wake Pod Monitoring Feature Design

This document outlines the design for the pod monitoring feature in Wake, which allows users to visualize CPU and memory metrics for Kubernetes pods.

## Command Line Interface

```
wake <pod selector> -m
# OR
wake -m
wake <pod selector> -c <container selector> -m  # Monitor specific containers
wake <pod selector> --all-containers -m         # Monitor all containers 

# Examples:
wake nginx -m
wake deployment/nginx -m
wake -m  # Show all pods
wake nginx -c "nginx.*" -m  # Monitor only nginx containers in nginx pods
```

## UI Layout and Components

The monitoring UI leverages the existing Wake terminal UI architecture but replaces the log view with metrics visualizations.

### Overall Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Wake Monitor                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│           Pod Selection: [nginx.*]     Container Selection: [nginx-main]    │
│                                                                             │
├─────────────────────────┬───────────────────────────────────────────────────┤
│                         │                                                   │
│                         │                                                   │
│                         │                                                   │
│                         │                                                   │
│  Pod/Container Tree     │              Metrics Dashboard Panel              │
│                         │                                                   │
│                         │                                                   │
│                         │                                                   │
│                         │                                                   │
├─────────────────────────┴───────────────────────────────────────────────────┤
│                             Status/Navigation Bar                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                 Help Bar                                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Details

#### 1. Selection Area
- Header with current filter/selection info
- Shows the pod selector pattern being used
- Shows the container selector pattern being used
- Provides ability to update filters in real-time
- Keyboard shortcut to toggle between pod and container selection mode

#### 2. Pod/Container Tree Panel (Left)
- Hierarchical view of pods and their containers
- Expandable tree structure:
  ```
  ├── nginx-deployment-7b64d5b86-abc12
  │   ├── nginx-main [selected]
  │   └── nginx-sidecar
  ├── nginx-deployment-7b64d5b86-def34
  │   ├── nginx-main
  │   └── nginx-sidecar
  └── nginx-deployment-7b64d5b86-ghi56
      ├── nginx-main
      └── nginx-sidecar
  ```
- Color-coded indicators showing pod/container health
- Selection mechanism for both pods and containers
- Multi-select capability to compare across pods and containers

#### 3. Metrics Dashboard Panel (Right)
- Multiple tabs for different metric categories:
  - Overview (default)
  - CPU
  - Memory
  - Network (if available)
  - Disk (if available)
- Time-series graphs showing real-time metrics
- For the Overview tab:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Pod: nginx-deployment-7b64d5b86-abc12  Container: nginx-main               │
│ Status: Running                                                            │
├─────────────────────────┬───────────────────────────────────────────────────┤
│                         │                                                   │
│  CPU Usage              │  Memory Usage                                     │
│  ▁▂▃▅▇█▇▅▆▇█           │  ▂▃▃▃▄▄▅▅▆▆▇▇                                     │
│  65% (250m/500m)        │  420MiB / 512MiB (82%)                           │
│                         │                                                   │
├─────────────────────────┼───────────────────────────────────────────────────┤
│                         │                                                   │
│  Network                │  Disk I/O                                         │
│  ▁▁▁▂▂▃▃▅█▇            │  ▂▂▂▂▃▃▂▂▂▁                                       │
│  RX: 2.5MB/s TX: 1MB/s  │  Read: 200KB/s Write: 50KB/s                     │
│                         │                                                   │
└─────────────────────────┴───────────────────────────────────────────────────┘
```

- For detailed metrics tabs:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ [Overview] [CPU] [Memory] [Network] [Disk]                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  CPU Usage Over Time                                                        │
│                                                                             │
│  100% ┤                 ╭─────╮     ╭───╮                                   │
│   80% ┤       ╭────╮    │     │     │   │                                   │
│   60% ┤ ╭──╮  │    │    │     ╰─────╯   ╰───╮                              │
│   40% ┤ │  │  │    ╰────╯                   │                              │
│   20% ┤ │  ╰──╯                             ╰────────                      │
│    0% ┤─────────────────────────────────────────────────────────────────   │
│       └─────────────────────────────────────────────────────────────────   │
│                                                                             │
│  CPU Throttling                                                             │
│  █████████████░░░░░    25% throttled                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 4. Status/Navigation Bar
- Pod count and container count (total and selected)
- Current refresh rate
- Active filters for both pods and containers
- Navigation indicators

#### 5. Help Bar
- Key shortcuts relevant to monitoring:
  - Tab/Arrow keys: Navigate between pods and containers
  - Enter: Expand/collapse pod to show/hide containers
  - Space: Select/deselect pod or container for monitoring
  - 1-5: Switch between metric tabs
  - +/-: Adjust time scale
  - f: Toggle auto-refresh
  - c: Compare selected pods/containers
  - p: Switch to pod selection mode
  - n: Switch to container selection mode

### Container Selection Mode

When entering container selection mode (by pressing 'n'):

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      CONTAINER SELECTION MODE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ Enter container pattern: nginx.*_                                           │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ Matched containers:                                                         │
│ - nginx-main                                                                │
│ - nginx-sidecar                                                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Features and Interactions

### 1. Real-Time Updates
- Metrics refresh automatically (default: every 5 seconds)
- Visual indication when metrics are refreshing
- User-configurable refresh rate

### 2. Multi-Pod/Container Comparison
- Select multiple pods/containers to compare metrics side-by-side
- Overlaid graphs with different colors for each pod/container
- Summary table showing key metrics across selections
- Options to:
  - Compare pods (aggregate of all containers)
  - Compare specific containers across pods
  - Compare containers within the same pod

### 3. Time Range Selection
- Default view shows last 5 minutes of metrics
- Options to zoom out to longer time periods:
  - Last 15 minutes
  - Last hour
  - Last 3 hours
- Ability to pause auto-refresh to examine a specific time period

### 4. Interactive Elements
- Mouse hover for detailed values at specific points on graphs
- Click on pods/containers to select/deselect for comparison
- Keyboard navigation between pods, containers, and metric tabs
- Keyboard shortcuts to change container selection pattern

### 5. Visual Indicators
- Color-coded metrics (green/yellow/red) based on resource usage thresholds
- Special indicators for resource limits and requests
- Visual alerts for pods/containers approaching resource limits
- Different icons for pods vs containers in the tree view

## Case Handling

### Multiple Containers in a Pod
- Hierarchical tree view with pods as parents and containers as children
- Ability to expand/collapse pods to show/hide their containers
- Ability to select individual containers or entire pods
- Container metrics display shows both the container name and its parent pod
- Easy keyboard/mouse navigation between containers in the same pod
- Option to view aggregate metrics for all containers in a pod

### Multiple Pods from Selector
- List view showing all matching pods with summary metrics
- Ability to sort/filter the pod list by various criteria:
  - Name
  - Namespace
  - CPU usage
  - Memory usage
  - Status
- Option to view aggregate metrics across all selected pods
- Pagination for large numbers of pods

### Container Selection Refinement
- Ability to update the container selector pattern in the UI
- Real-time filtering as the pattern is updated
- Visual feedback showing which containers match the current pattern
- Option to match all containers (default: ".*") or specific ones
- Keyboard shortcut to toggle the --all-containers mode

## Technical Implementation Notes

### Metrics Collection
- Leverage the Kubernetes Metrics API
- Implement efficient polling with backoff for large clusters
- Cache metrics locally to reduce API load
- Support for both in-cluster and external cluster access
- Separate collection paths for pod-level and container-level metrics

### UI Rendering
- Use Ratatui for terminal graphics
- Implement custom widgets for specialized graph types
- Optimize for different terminal sizes
- Support for color and monochrome terminals
- Hierarchical tree widget for pod/container navigation

### Data Management
- Time-series data structure for historical metrics
- Efficient data pruning to manage memory usage
- Background thread for metrics collection independent of UI rendering
- Indexing for quick lookup by pod/container identifiers