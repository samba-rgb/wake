# Wake Web View Feature Design Document

## Overview

The Web View feature adds a new mode to the Wake tool that enables sending filtered Kubernetes logs to a web endpoint via HTTP requests instead of displaying them in the terminal. This feature is designed for integration with web dashboards, logging services, and monitoring systems.

## Feature Requirements

### Command Line Interface
```bash
wake pod_selector -i <include_pattern> --web [--web-endpoint <url>] [--web-method <method>] [--web-headers <headers>]
```

### Key Features
- Non-UI mode operation (CLI mode only)
- HTTP/HTTPS endpoint support
- Configurable HTTP methods (POST, PUT, PATCH)
- Custom headers support
- Batch and streaming modes
- Error handling and retry mechanisms
- Authentication support (Bearer tokens, API keys)

## Current Flow vs Web Flow

### Current Flow (CLI Mode)
```
Pods → K8s API → Log Buffer → Filtering → Display Buffer → Terminal Screen
```

### New Web Flow
```
Pods → K8s API → Log Buffer → Filtering → Display Buffer → HTTP Client → Web Endpoint
```

## Architecture Design

### 1. New Components

#### WebOutputHandler
**Location**: `src/output/web.rs`
```rust
pub struct WebOutputHandler {
    endpoint: String,
    method: HttpMethod,
    headers: HashMap<String, String>,
    client: reqwest::Client,
    batch_size: usize,
    batch_timeout: Duration,
    retry_config: RetryConfig,
}
```

**Responsibilities**:
- HTTP client management
- Batch log aggregation
- Retry logic with exponential backoff
- Authentication header injection
- Error handling and logging

#### WebConfig
**Location**: `src/config/web.rs`
```rust
pub struct WebConfig {
    pub default_endpoint: Option<String>,
    pub default_method: HttpMethod,
    pub default_headers: HashMap<String, String>,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
    pub timeout_ms: u64,
}
```

### 2. Modified Components

#### CLI Args (`src/cli/args.rs`)
**New fields to add**:
```rust
/// Enable web mode - send logs to HTTP endpoint instead of terminal
#[arg(long, help = "Send filtered logs to web endpoint via HTTP")]
pub web: bool,

/// Web endpoint URL for sending logs
#[arg(long = "web-endpoint", help = "HTTP endpoint URL to send logs to")]
pub web_endpoint: Option<String>,

/// HTTP method for web requests
#[arg(long = "web-method", help = "HTTP method (POST, PUT, PATCH)", default_value = "POST")]
pub web_method: String,

/// Custom HTTP headers (format: key=value,key2=value2)
#[arg(long = "web-headers", help = "Custom HTTP headers (key=value,key2=value2)")]
pub web_headers: Option<String>,

/// Batch size for web requests (number of log entries per request)
#[arg(long = "web-batch-size", help = "Number of log entries to batch per HTTP request", default_value = "10")]
pub web_batch_size: usize,

/// Timeout for web requests in seconds
#[arg(long = "web-timeout", help = "HTTP request timeout in seconds", default_value = "30")]
pub web_timeout: u64,
```

#### Output Module (`src/output/mod.rs`)
**New enum variant**:
```rust
pub enum OutputMode {
    Terminal,
    File(PathBuf),
    Web(WebOutputHandler),
    TerminalAndFile(PathBuf),
    TerminalAndWeb(WebOutputHandler),
    FileAndWeb(PathBuf, WebOutputHandler),
}
```

### 3. Data Flow Implementation

#### Log Entry Format for Web
```rust
#[derive(Serialize, Clone)]
pub struct WebLogEntry {
    pub timestamp: String,           // ISO 8601 format
    pub namespace: String,
    pub pod_name: String,
    pub container_name: String,
    pub message: String,
    pub level: Option<String>,       // Extracted if possible
    pub source: String,              // "kubernetes"
    pub cluster: Option<String>,     // From kube context
    pub metadata: HashMap<String, String>, // Additional fields
}
```

#### HTTP Payload Formats

**Single Entry Mode**:
```json
{
  "timestamp": "2025-10-14T10:30:00Z",
  "namespace": "default",
  "pod_name": "my-app-123",
  "container_name": "app",
  "message": "Application started successfully",
  "level": "INFO",
  "source": "kubernetes",
  "cluster": "production",
  "metadata": {
    "wake_version": "1.0.0",
    "filter_pattern": "info"
  }
}
```

**Batch Mode**:
```json
{
  "entries": [
    { /* log entry 1 */ },
    { /* log entry 2 */ },
    { /* log entry N */ }
  ],
  "batch_info": {
    "size": 10,
    "timestamp": "2025-10-14T10:30:00Z",
    "source": "wake"
  }
}
```

## Implementation Plan

### Phase 1: Core Web Output
1. **Create WebOutputHandler** (`src/output/web.rs`)
   - Basic HTTP client with reqwest
   - Single log entry sending
   - Basic error handling

2. **Extend CLI Args** (`src/cli/args.rs`)
   - Add `--web` flag
   - Add `--web-endpoint` option
   - Add validation

3. **Modify Output Module** (`src/output/mod.rs`)
   - Add Web output mode
   - Integrate with existing formatter

4. **Update Main Flow** (`src/cli/mod.rs`)
   - Detect web mode
   - Initialize WebOutputHandler
   - Route logs to web instead of terminal

### Phase 2: Advanced Features
1. **Batch Processing**
   - Configurable batch sizes
   - Time-based batching
   - Batch timeout handling

2. **HTTP Configuration**
   - Custom HTTP methods
   - Custom headers support
   - Authentication (Bearer tokens, API keys)

3. **Retry Mechanism**
   - Exponential backoff
   - Maximum retry attempts
   - Dead letter handling

### Phase 3: Configuration & Management
1. **Configuration System**
   - Default web endpoints in config
   - Persistent authentication tokens
   - Profile-based configurations

2. **Monitoring & Observability**
   - HTTP request metrics
   - Success/failure rates
   - Performance monitoring

## Configuration Examples

### Basic Usage
```bash
# Send logs to webhook
wake "my-app" -i "error" --web --web-endpoint "https://webhook.site/123"

# With custom headers
wake "api-*" -i "warning|error" --web \
  --web-endpoint "https://api.myservice.com/logs" \
  --web-headers "Authorization=Bearer token123,Content-Type=application/json"

# Batch mode
wake "frontend-*" --web \
  --web-endpoint "https://logs.company.com/ingest" \
  --web-batch-size 50 \
  --web-timeout 60
```

### Configuration File (`~/.wake/config.toml`)
```toml
[web]
default_endpoint = "https://logs.company.com/api/v1/ingest"
default_method = "POST"
batch_size = 25
batch_timeout_ms = 5000
retry_attempts = 3
retry_delay_ms = 1000
timeout_ms = 30000

[web.default_headers]
"Authorization" = "Bearer ${WAKE_API_TOKEN}"
"X-Source" = "wake-k8s-logs"
"Content-Type" = "application/json"
```

## Error Handling Strategy

### HTTP Error Responses
- **4xx errors**: Log and skip (don't retry)
- **5xx errors**: Retry with exponential backoff
- **Network errors**: Retry with backoff
- **Timeout errors**: Retry with longer timeout

### Fallback Mechanisms
1. **File Fallback**: Save failed logs to local file
2. **Terminal Fallback**: Output to terminal if web fails
3. **Dead Letter Queue**: Store failed batches for later retry

## Security Considerations

### Authentication
- Support for Bearer tokens
- API key authentication via headers
- Environment variable substitution for secrets

### HTTPS/TLS
- Enforce HTTPS for production endpoints
- Certificate validation
- Custom CA support if needed

### Data Sanitization
- Remove sensitive information from logs
- Configurable field masking
- Size limits to prevent DoS

## Testing Strategy

### Unit Tests
- WebOutputHandler functionality
- HTTP client behavior
- Retry logic
- Batch processing

### Integration Tests
- End-to-end log flow
- Real HTTP endpoint testing
- Error scenario testing
- Performance testing

### Test Utilities
```rust
// Mock HTTP server for testing
pub struct MockWebServer {
    server: mockito::ServerGuard,
    received_requests: Arc<Mutex<Vec<WebLogEntry>>>,
}
```

## Performance Considerations

### Memory Usage
- Bounded queues for batching
- Configurable buffer sizes
- Memory pressure handling

### Network Efficiency
- HTTP connection pooling
- Compression support (gzip)
- Keep-alive connections

### Throughput
- Concurrent HTTP requests
- Async processing pipeline
- Backpressure handling

## Monitoring and Observability

### Metrics to Track
- HTTP request success/failure rates
- Request latency percentiles
- Batch sizes and timing
- Retry attempt counts
- Memory usage

### Logging
- HTTP request/response logging (debug mode)
- Error details with context
- Performance metrics

## Future Enhancements

### Potential Features
1. **Multiple Endpoints**: Send to multiple web services
2. **Webhook Validation**: HMAC signatures for security
3. **Schema Validation**: Validate log format before sending
4. **Rate Limiting**: Respect endpoint rate limits
5. **Compression**: gzip/deflate support for large payloads
6. **WebSocket Support**: Real-time streaming via WebSockets
7. **GraphQL Support**: Send logs via GraphQL mutations

### Integration Possibilities
- Elasticsearch/OpenSearch ingestion
- Splunk HTTP Event Collector
- Datadog logs API
- New Relic logs API
- Custom internal logging services

## Migration Path

### Backward Compatibility
- All existing functionality remains unchanged
- Web mode is opt-in via `--web` flag
- No breaking changes to existing CLI interface

### Gradual Adoption
1. Start with basic HTTP POST to webhooks
2. Add authentication and custom headers
3. Implement batching and retry logic
4. Add configuration management
5. Extend to more complex integrations

This design provides a solid foundation for the web view feature while maintaining the simplicity and power of the existing Wake tool.