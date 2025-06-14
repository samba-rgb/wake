# Wake Complete Architecture - UML Diagrams

This document provides comprehensive UML diagrams for the Wake Kubernetes log tailing tool architecture, covering every component, relationship, and data flow.

## 1. High-Level System Architecture

```mermaid
graph TB
    subgraph "External Systems"
        K8S[Kubernetes API Server]
        TERM[Terminal/Console]
        FILE[File System]
    end
    
    subgraph "Wake Application"
        subgraph "Entry Point"
            MAIN[main.rs]
            CLI[cli::run]
        end
        
        subgraph "Core Modules"
            K8S_MOD[k8s Module]
            FILTER[filtering Module]
            OUTPUT[output Module]
            LOGGING[logging Module]
            UI_MOD[ui Module]
        end
        
        subgraph "Data Processing Pipeline"
            WATCHER[LogWatcher]
            STREAM[Log Stream]
            THREADS[Thread Pool]
            FORMATTER[Formatter]
        end
    end
    
    K8S --> K8S_MOD
    MAIN --> CLI
    CLI --> K8S_MOD
    CLI --> UI_MOD
    CLI --> LOGGING
    
    K8S_MOD --> WATCHER
    WATCHER --> STREAM
    STREAM --> FILTER
    FILTER --> THREADS
    THREADS --> OUTPUT
    OUTPUT --> FORMATTER
    FORMATTER --> TERM
    FORMATTER --> FILE
    
    UI_MOD --> TERM
```

## 2. Core Data Models Class Diagram

```mermaid
classDiagram
    class LogEntry {
        +String namespace
        +String pod_name
        +String container_name
        +String message
        +Option~DateTime~Utc~~ timestamp
        +clone() LogEntry
    }
    
    class Args {
        +String pod_selector
        +String container
        +String namespace
        +Option~String~ include
        +Option~String~ exclude
        +bool timestamps
        +String output
        +Option~PathBuf~ output_file
        +Option~String~ resource
        +Option~String~ template
        +Option~String~ since
        +Option~usize~ threads
        +bool ui
        +bool dev
        +u8 verbosity
        +i64 tail
        +bool follow
        +bool all_containers
        +bool all_namespaces
        +bool list_containers
        +Option~String~ context
        +pod_regex() Result~Regex~
        +container_regex() Result~Regex~
        +include_regex() Option~Result~Regex~~
        +exclude_regex() Option~Result~Regex~~
    }
    
    class PodInfo {
        +String namespace
        +String name
        +Vec~String~ containers
        +clone() PodInfo
    }
    
    class ResourceType {
        <<enumeration>>
        Pod
        Deployment
        ReplicaSet
        StatefulSet
        Job
        +from_str(s: &str) Option~Self~
    }
    
    LogEntry --> Args : "configured by"
    PodInfo --> ResourceType : "typed as"
```

## 3. Kubernetes Integration Module

```mermaid
classDiagram
    class LogWatcher {
        -Client client
        -Arc~Args~ args
        +new(client: Client, args: &Args) Self
        +stream() Result~Pin~Box~dyn Stream~Item=LogEntry~ + Send~~~
        -stream_container_logs(client: Client, namespace: &str, pod_name: &str, container_name: &str, follow: bool, tail_lines: i64, timestamps: bool, tx: Sender~LogEntry~, since: Option~String~) Result~()~
    }
    
    class Client {
        <<external>>
        +Api~Pod~ namespaced
        +logs() Stream
        +log_stream() Stream
    }
    
    class PodSelector {
        +select_pods(client: &Client, namespace: &str, pod_regex: &Regex, container_regex: &Regex, all_namespaces: bool, resource: Option~&str~) Result~Vec~PodInfo~~
        +list_container_names(client: &Client, namespace: &str, pod_regex: &Regex, all_namespaces: bool, resource: Option~&str~) Result~()~
    }
    
    class ResourceFilter {
        +filter_by_resource_type(pods: Vec~Pod~, resource_type: Option~&str~) Vec~Pod~
        +get_pods_for_deployment(client: &Client, namespace: &str, name: &str) Result~Vec~Pod~~
        +get_pods_for_statefulset(client: &Client, namespace: &str, name: &str) Result~Vec~Pod~~
    }
    
    LogWatcher --> Client : "uses"
    LogWatcher --> PodSelector : "uses"
    PodSelector --> ResourceFilter : "uses"
    LogWatcher --> LogEntry : "produces"
    LogWatcher --> Args : "configured by"
```

## 4. Filtering System Architecture

```mermaid
classDiagram
    class FilterPattern {
        <<enumeration>>
        Simple(Regex)
        And(Box~FilterPattern~, Box~FilterPattern~)
        Or(Box~FilterPattern~, Box~FilterPattern~)
        Not(Box~FilterPattern~)
        Contains(String)
        +parse(pattern: &str) Result~Self~
        +matches(message: &str) bool
    }
    
    class LogFilter {
        -Option~Arc~FilterPattern~~ include_pattern
        -Option~Arc~FilterPattern~~ exclude_pattern
        -ThreadPool thread_pool
        +new(include_pattern: Option~Regex~, exclude_pattern: Option~Regex~, num_threads: usize) Self
        +new_with_patterns(include_pattern: Option~String~, exclude_pattern: Option~String~, num_threads: usize) Result~Self~
        +start_filtering(input_rx: Receiver~LogEntry~) Receiver~LogEntry~
        +recommended_threads() usize
    }
    
    class DynamicFilterManager {
        -Arc~RwLock~Option~Arc~Regex~~~~ include_pattern
        -Arc~RwLock~Option~Arc~Regex~~~~ exclude_pattern
        -Arc~RwLock~Vec~LogEntry~~~ log_buffer
        -usize buffer_size
        +new(initial_include: Option~String~, initial_exclude: Option~String~, buffer_size: usize) Result~Self~
        +update_include_pattern(pattern: Option~String~) Result~()~
        +update_exclude_pattern(pattern: Option~String~) Result~()~
        +should_include(entry: &LogEntry) bool
        +add_to_buffer(entry: LogEntry)
        +get_filtered_buffer() Vec~LogEntry~
        +get_current_patterns() (Option~String~, Option~String~)
        +start_filtering(input_rx: Receiver~LogEntry~) Receiver~LogEntry~
    }
    
    class ThreadPool {
        <<external>>
        +new(size: usize) Self
        +execute(job: F)
    }
    
    LogFilter --> FilterPattern : "uses"
    LogFilter --> ThreadPool : "manages"
    DynamicFilterManager --> LogEntry : "processes"
    LogFilter --> LogEntry : "processes"
```

## 5. Output System Architecture

```mermaid
classDiagram
    class OutputFormatter {
        <<interface>>
        +format(entry: &LogEntry) Result~String~
        +format_name() Option~String~
    }
    
    class TextFormatter {
        -bool show_timestamps
        +new(show_timestamps: bool) Self
        +format(entry: &LogEntry) Result~String~
        +format_name() Option~String~
    }
    
    class JsonFormatter {
        +new() Self
        +format(entry: &LogEntry) Result~String~
        +format_name() Option~String~
    }
    
    class RawFormatter {
        +new() Self
        +format(entry: &LogEntry) Result~String~
        +format_name() Option~String~
    }
    
    class Formatter {
        -OutputFormat output_format
        -Option~Regex~ include_pattern
        -Option~Regex~ exclude_pattern
        -bool show_timestamps
        -Mutex~HashMap~String, Color~~ pod_colors
        -Mutex~HashMap~String, Color~~ container_colors
        +new(args: &Args) Self
        +format(entry: &LogEntry) Option~String~
        +format_without_filtering(entry: &LogEntry) Option~String~
        -format_text(entry: &LogEntry) String
        -format_json(entry: &LogEntry) String
        -get_color_for_pod(pod_name: &str) Color
        -get_color_for_container(container_name: &str) Color
        -get_or_assign_color(colors: &Mutex~HashMap~String, Color~~, key: &str) Color
    }
    
    class OutputFormat {
        <<enumeration>>
        Text
        Json
        Raw
        Template(String)
    }
    
    OutputFormatter <|-- TextFormatter
    OutputFormatter <|-- JsonFormatter
    OutputFormatter <|-- RawFormatter
    Formatter --> OutputFormat : "uses"
    Formatter --> LogEntry : "formats"
    Formatter --> Args : "configured by"
```

## 6. UI Module Architecture

```mermaid
classDiagram
    class DisplayManager {
        +VecDeque~String~ log_lines
        +usize scroll_offset
        +usize max_lines
        +usize total_logs
        +usize filtered_logs
        +Formatter formatter
        +new(max_lines: usize, show_timestamps: bool) Result~Self~
        +add_log_entry(entry: &LogEntry)
        +add_system_message(message: &str)
        +clear_logs()
        +scroll_up(lines: usize)
        +scroll_down(lines: usize, viewport_height: usize)
        +scroll_to_top()
        +scroll_to_bottom(viewport_height: usize)
        +render(f: &mut Frame, input_handler: &InputHandler)
        -render_filter_area(f: &mut Frame, area: Rect, input_handler: &InputHandler)
        -render_log_area(f: &mut Frame, area: Rect, input_handler: &InputHandler)
        -render_status_bar(f: &mut Frame, area: Rect, input_handler: &InputHandler)
        -render_help_popup(f: &mut Frame, input_handler: &InputHandler)
    }
    
    class InputHandler {
        +InputMode mode
        +String include_input
        +String exclude_input
        +usize cursor_position
        +VecDeque~String~ input_history
        +Option~usize~ history_index
        +new(initial_include: Option~String~, initial_exclude: Option~String~) Self
        +handle_key_event(key: KeyEvent) Option~InputEvent~
        -handle_normal_mode(key: KeyEvent) Option~InputEvent~
        -handle_editing_mode(key: KeyEvent, is_include: bool) Option~InputEvent~
        -handle_help_mode(key: KeyEvent) Option~InputEvent~
        -add_to_history(input: String)
        -navigate_history(up: bool, is_include: bool)
        +get_help_text() Vec~&'static str~
    }
    
    class InputMode {
        <<enumeration>>
        Normal
        EditingInclude
        EditingExclude
        Help
    }
    
    class InputEvent {
        <<enumeration>>
        UpdateIncludeFilter(String)
        UpdateExcludeFilter(String)
        ToggleHelp
        ScrollUp
        ScrollDown
        ScrollToTop
        ScrollToBottom
        Quit
        Refresh
    }
    
    class UIApp {
        +run_app(log_stream: Pin~Box~dyn Stream~Item=LogEntry~ + Send~~, args: Args) Result~()~
        +run_with_ui(log_stream: Pin~Box~dyn Stream~Item=LogEntry~ + Send~~, args: Args) Result~()~
    }
    
    DisplayManager --> LogEntry : "displays"
    DisplayManager --> Formatter : "uses"
    InputHandler --> InputMode : "has"
    InputHandler --> InputEvent : "produces"
    UIApp --> DisplayManager : "manages"
    UIApp --> InputHandler : "uses"
    UIApp --> DynamicFilterManager : "uses"
```

## 7. Logging and Processing Module

```mermaid
classDiagram
    class LoggingModule {
        +setup_logger(verbosity: u8) Result~()~
        +get_log_level(verbosity: u8) Level
        +process_logs(log_stream: impl Stream~Item=LogEntry~, args: &Args, formatter: Formatter) Result~()~
        +setup_signal_handler() Result~()~
    }
    
    class Level {
        <<enumeration>>
        ERROR
        WARN
        INFO
        DEBUG
        TRACE
    }
    
    class ProcessingPipeline {
        -mpsc::Sender~LogEntry~ raw_tx
        -mpsc::Receiver~LogEntry~ raw_rx
        -LogFilter filter
        -Box~dyn Write + Send~ output_writer
        +new(args: &Args, formatter: Formatter) Self
        +process_stream(log_stream: Stream~LogEntry~) Result~()~
    }
    
    LoggingModule --> Level : "uses"
    LoggingModule --> ProcessingPipeline : "creates"
    ProcessingPipeline --> LogFilter : "uses"
    ProcessingPipeline --> Formatter : "uses"
    ProcessingPipeline --> LogEntry : "processes"
```

## 8. Complete Data Flow Sequence Diagram

```mermaid
sequenceDiagram
    participant User
    participant Main
    participant CLI
    participant K8sClient
    participant LogWatcher
    participant FilterManager
    participant ThreadPool
    participant Formatter
    participant UI
    participant Output
    
    User->>Main: wake -n apps "pod-name" --ui -i "ERROR"
    Main->>CLI: parse_args()
    Main->>CLI: run(args)
    
    CLI->>K8sClient: create_client()
    CLI->>LogWatcher: new(client, args)
    CLI->>LogWatcher: stream()
    
    LogWatcher->>K8sClient: select_pods()
    K8sClient-->>LogWatcher: Vec<PodInfo>
    
    loop For each pod/container
        LogWatcher->>K8sClient: log_stream()
        K8sClient-->>LogWatcher: Stream<LogEntry>
    end
    
    alt UI Mode
        CLI->>UI: run_with_ui(log_stream, args)
        UI->>FilterManager: new(include, exclude)
        
        loop Main UI Loop
            par Log Processing
                LogWatcher->>FilterManager: send(LogEntry)
                FilterManager->>ThreadPool: filter_task
                ThreadPool-->>FilterManager: filtered_entry
                FilterManager->>UI: send(filtered_entry)
            and User Input
                User->>UI: key_event
                UI->>FilterManager: update_pattern()
            and Rendering
                UI->>Formatter: format(entry)
                Formatter-->>UI: formatted_string
                UI->>Output: render_to_terminal()
            end
        end
    else CLI Mode
        CLI->>FilterManager: new(include, exclude)
        loop Log Processing
            LogWatcher->>FilterManager: send(LogEntry)
            FilterManager->>ThreadPool: filter_task
            ThreadPool-->>FilterManager: filtered_entry
            FilterManager->>Formatter: format(entry)
            Formatter-->>FilterManager: formatted_string
            FilterManager->>Output: write(stdout/file)
        end
    end
```

## 9. Threading and Concurrency Architecture

```mermaid
graph TB
    subgraph "Main Async Runtime (Tokio)"
        MAIN_TASK[Main Task]
        K8S_TASKS[Kubernetes API Tasks]
        UI_TASK[UI Event Loop Task]
        STREAM_TASK[Log Stream Processing Task]
        FILE_TASK[File Writing Task]
    end
    
    subgraph "OS Thread Pool (Filtering)"
        FILTER_THREAD_1[Filter Worker 1]
        FILTER_THREAD_2[Filter Worker 2]
        FILTER_THREAD_N[Filter Worker N]
    end
    
    subgraph "Channels (mpsc)"
        RAW_LOGS[Raw Log Channel]
        FILTERED_LOGS[Filtered Log Channel]
        INPUT_EVENTS[Input Event Channel]
        UI_UPDATES[UI Update Channel]
    end
    
    MAIN_TASK --> K8S_TASKS
    MAIN_TASK --> UI_TASK
    K8S_TASKS --> RAW_LOGS
    RAW_LOGS --> FILTER_THREAD_1
    RAW_LOGS --> FILTER_THREAD_2
    RAW_LOGS --> FILTER_THREAD_N
    FILTER_THREAD_1 --> FILTERED_LOGS
    FILTER_THREAD_2 --> FILTERED_LOGS
    FILTER_THREAD_N --> FILTERED_LOGS
    FILTERED_LOGS --> STREAM_TASK
    FILTERED_LOGS --> UI_TASK
    FILTERED_LOGS --> FILE_TASK
    UI_TASK --> INPUT_EVENTS
    INPUT_EVENTS --> UI_UPDATES
```

## 10. Error Handling and State Management

```mermaid
stateDiagram-v2
    [*] --> Initializing
    Initializing --> ConfigValidation
    ConfigValidation --> K8sConnection
    K8sConnection --> PodDiscovery
    PodDiscovery --> StreamSetup
    StreamSetup --> Running
    
    state Running {
        [*] --> LogStreaming
        LogStreaming --> Filtering
        Filtering --> Formatting
        Formatting --> Output
        Output --> LogStreaming
        
        state fork_state <<fork>>
        LogStreaming --> fork_state
        fork_state --> UIRendering
        fork_state --> FileWriting
        fork_state --> StdoutWriting
        
        UIRendering --> LogStreaming
        FileWriting --> LogStreaming
        StdoutWriting --> LogStreaming
    }
    
    Running --> Error : Connection Lost
    Running --> Terminated : User Quit
    Running --> Terminated : Signal
    Error --> Reconnecting : Retry
    Reconnecting --> Running : Success
    Reconnecting --> Terminated : Max Retries
    Terminated --> [*]
```

## 11. Configuration and Dependency Management

```mermaid
classDiagram
    class ConfigurationManager {
        +load_kubeconfig() Result~Config~
        +validate_args(args: &Args) Result~()~
        +setup_logging(args: &Args) Result~()~
        +create_client(args: &Args) Result~Client~
    }
    
    class DependencyContainer {
        +Client k8s_client
        +Args configuration
        +LogFilter filter
        +Formatter formatter
        +Box~dyn Write~ output_writer
        +new(args: Args) Result~Self~
        +create_log_watcher() LogWatcher
        +create_ui_manager() UIManager
    }
    
    class ErrorHandler {
        +handle_k8s_error(error: KubeError) Result~()~
        +handle_io_error(error: IoError) Result~()~
        +handle_regex_error(error: RegexError) Result~()~
        +should_retry(error: &Error) bool
        +get_retry_delay(attempt: u32) Duration
    }
    
    ConfigurationManager --> Args : "validates"
    DependencyContainer --> ConfigurationManager : "uses"
    DependencyContainer --> ErrorHandler : "uses"
    ErrorHandler --> LogWatcher : "handles errors from"
    ErrorHandler --> FilterManager : "handles errors from"
```

## 12. Memory Management and Performance

```mermaid
graph LR
    subgraph "Memory Pools"
        LOG_BUFFER[Circular Log Buffer<br/>10K entries max]
        CHANNEL_BUFFERS[Channel Buffers<br/>1K-2K entries]
        REGEX_CACHE[Compiled Regex Cache<br/>Arc<Regex> shared]
    end
    
    subgraph "Performance Optimizations"
        BATCHING[Batch Processing<br/>Multiple entries at once]
        BACKPRESSURE[Backpressure Handling<br/>Bounded channels]
        LAZY_FORMATTING[Lazy Formatting<br/>Format only when needed]
        COLOR_CACHE[Color Assignment Cache<br/>Consistent pod colors]
    end
    
    subgraph "Resource Limits"
        THREAD_LIMIT[Thread Pool Limit<br/>2x CPU cores default]
        MEMORY_LIMIT[Memory Limit<br/>Circular buffer eviction]
        RATE_LIMIT[Rate Limiting<br/>Channel capacity control]
    end
    
    LOG_BUFFER --> BATCHING
    CHANNEL_BUFFERS --> BACKPRESSURE
    REGEX_CACHE --> LAZY_FORMATTING
    COLOR_CACHE --> LAZY_FORMATTING
    THREAD_LIMIT --> RATE_LIMIT
    MEMORY_LIMIT --> LOG_BUFFER
```

## 13. Testing Architecture

```mermaid
classDiagram
    class TestFramework {
        <<abstract>>
    }
    
    class UnitTests {
        +test_log_entry_creation()
        +test_formatter_text()
        +test_formatter_json()
        +test_formatter_raw()
        +test_filter_patterns()
        +test_args_parsing()
        +test_regex_compilation()
    }
    
    class IntegrationTests {
        +test_log_streaming()
        +test_filtering_pipeline()
        +test_ui_interaction()
        +test_file_output()
        +test_performance_large_volume()
        +test_edge_cases()
    }
    
    class MockObjects {
        +MockPodApi
        +MockK8sClient
        +MockLogStream
        +create_test_log_entries()
        +create_large_log_stream()
    }
    
    class TestFixtures {
        +k8s_fixtures.rs
        +pod_logs.txt
        +performance_data.json
    }
    
    TestFramework <|-- UnitTests
    TestFramework <|-- IntegrationTests
    IntegrationTests --> MockObjects : "uses"
    UnitTests --> TestFixtures : "uses"
    IntegrationTests --> TestFixtures : "uses"
```

This comprehensive UML documentation covers every aspect of the Wake architecture, from high-level system design to detailed class relationships, data flows, concurrency patterns, and testing strategies. Each diagram provides specific implementation details gleaned from the actual codebase analysis.