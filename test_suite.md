# Wake Test Suite Documentation

## Overview

Wake's test suite is organized into three main categories, following standard Rust testing practices:

1. **Unit Tests** - Test individual components in isolation
2. **Integration Tests** - Test multiple components working together
3. **Common Test Utilities** - Shared test fixtures and mocks

## Directory Structure

```
tests/
├── mod.rs                    # Root test module
├── test_utils.rs             # Common test utilities
├── common/                   # Shared test resources
│   ├── fixtures/            # Test data fixtures
│   └── mocks/              # Mock implementations
├── unit/                    # Unit test modules
│   ├── cli/                # CLI component tests
│   ├── k8s/                # Kubernetes component tests
│   ├── logging/            # Logging component tests
│   └── output/             # Output formatting tests
└── integration/            # Integration test modules
    ├── app_test.rs         # End-to-end application tests
    ├── edge_cases_test.rs  # Edge case handling tests
    ├── filtering_test.rs   # Log filtering tests
    └── performance_test.rs # Performance benchmark tests
```

## Test Categories and Test Cases

### 1. Unit Tests

#### CLI Tests (`unit/cli/`)

##### Args Test (`args_test.rs`)
- **test_default_args**
  - Purpose: Verifies default CLI argument values
  - Tests: All argument fields have correct default values
  - Validation: Namespace="default", pod_selector=".*", etc.

- **test_pod_regex**
  - Purpose: Tests pod name regex pattern matching
  - Tests: Regex compilation and matching for pod names
  - Validation: Matches/non-matches against sample pod names

- **test_container_regex**
  - Purpose: Tests container name regex pattern matching
  - Tests: Regex compilation and matching for container names
  - Validation: Matches/non-matches against sample container names

- **test_parse_args**
  - Purpose: Tests CLI argument parsing from command line
  - Tests: Multiple argument combinations and formats
  - Validation: Correct parsing of flags, values, and positional args

#### Kubernetes Tests (`unit/k8s/`)

##### Client Test (`client_test.rs`)
- **test_create_client_default**
  - Purpose: Tests default Kubernetes client creation
  - Tests: Client initialization with default settings
  - Note: Requires kubeconfig setup (marked as ignored)

- **test_create_client_custom_context**
  - Purpose: Tests client creation with specific context
  - Tests: Client initialization with custom context
  - Validation: Client creation with "minikube" context

##### Logs Test (`logs_test.rs`)
- **test_log_entry_creation**
  - Purpose: Tests LogEntry struct creation and validation
  - Tests: All fields of LogEntry are correctly set
  - Validation: Namespace, pod name, container name, message, timestamp

- **test_log_watcher_creation**
  - Purpose: Tests LogWatcher initialization
  - Tests: Creation with client and args
  - Note: Requires Kubernetes config (marked as ignored)

##### Pod Test (`pod_test.rs`)
- **test_pod_info_creation**
  - Purpose: Tests PodInfo struct creation
  - Tests: All fields of PodInfo are correctly set
  - Validation: Name, namespace, containers list

- **test_select_pods_with_regex**
  - Purpose: Tests pod selection using regex patterns
  - Tests: Pod filtering based on name patterns
  - Validation: Correct pods selected based on regex

- **test_select_pods_across_namespaces**
  - Purpose: Tests cross-namespace pod selection
  - Tests: Pod discovery across multiple namespaces
  - Validation: Pods found in different namespaces

#### Output Tests (`unit/output/`)

##### Formatter Test (`formatter_test.rs`)
- **test_text_formatter**
  - Purpose: Tests basic text output formatting
  - Tests: Default text format without timestamps
  - Validation: Correct format "[namespace/pod/container] message"

- **test_text_formatter_with_timestamp**
  - Purpose: Tests text format with timestamps
  - Tests: Text format including timestamp information
  - Validation: Timestamp format and placement

- **test_json_formatter**
  - Purpose: Tests JSON output formatting
  - Tests: Log entry serialization to JSON
  - Validation: JSON structure and field names

- **test_raw_formatter**
  - Purpose: Tests raw output format
  - Tests: Unformatted log message output
  - Validation: Only message content without metadata

### 2. Integration Tests

#### App Test (`integration/app_test.rs`)
- **test_log_streaming_with_real_cluster**
  - Purpose: Tests end-to-end log streaming with real K8s cluster
  - Tests: Complete log streaming workflow
  - Validation: Log retrieval and processing
  - Note: Requires running K8s cluster (marked as ignored)

- **test_log_streaming_with_mocks**
  - Purpose: Tests log streaming with mock K8s API
  - Tests: Streaming workflow with mock data
  - Validation: Log processing and formatting

#### Edge Cases Test (`edge_cases_test.rs`)
- **test_empty_logs**
  - Purpose: Tests handling of empty log streams
  - Tests: Behavior with no log entries
  - Validation: Graceful handling of empty streams

- **test_logs_with_special_characters**
  - Purpose: Tests handling of unusual log content
  - Tests: Special characters, Unicode, very long lines
  - Validation: Correct formatting and display

- **test_log_timestamp_edge_cases**
  - Purpose: Tests timestamp parsing edge cases
  - Tests: Missing timestamps, old dates, future dates
  - Validation: Proper handling of various timestamp formats

- **test_error_handling**
  - Purpose: Tests error conditions and recovery
  - Tests: Invalid formatters, regex patterns
  - Validation: Appropriate error messages and handling

#### Filtering Test (`filtering_test.rs`)
- **test_log_filtering_with_includes_excludes**
  - Purpose: Tests log filtering patterns
  - Tests: Include/exclude regex patterns
  - Validation: Correct filtering of log entries

- **test_log_formatting_multiple_formats**
  - Purpose: Tests different output format options
  - Tests: Text, JSON, and raw formats
  - Validation: Format-specific output requirements

- **test_namespace_filtering**
  - Purpose: Tests namespace-based filtering
  - Tests: Log filtering by namespace
  - Validation: Namespace-specific log selection

#### Performance Test (`performance_test.rs`)
- **test_log_processing_performance**
  - Purpose: Tests log processing throughput
  - Tests: Processing of large log volumes
  - Validation: Performance metrics and thresholds

- **test_concurrent_log_processing**
  - Purpose: Tests concurrent log stream handling
  - Tests: Multiple simultaneous log streams
  - Validation: Concurrent processing efficiency

- **test_filtering_performance**
  - Purpose: Tests filtering performance with large datasets
  - Tests: Large-scale log filtering operations
  - Validation: Filtering speed and resource usage

## Test Fixtures and Mocks

### Fixtures (`common/fixtures/`)
- **k8s_fixtures.rs**
  - `create_test_pods()`: Creates standard test pod definitions
  - `create_test_log_entries()`: Creates sample log entries
  - `create_test_pod()`: Creates a single test pod with specified properties

### Mocks (`common/mocks/`)
- **k8s_client.rs**
  - `MockPodApi`: Mock implementation of Pod API
  - `mock_pod()`: Creates mock Pod resources
  - `create_mock_pods()`: Creates a set of mock pods for testing

## Running Tests

### Command Line Options

1. Run all tests:
```bash
cargo test
```

2. Run specific test categories:
```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test integration
```

3. Run specific tests:
```bash
# Run a single test by name
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

4. Run ignored tests:
```bash
# Run tests that require special setup
cargo test -- --ignored
```

### Test Environment Setup

1. Unit Tests:
   - Most unit tests run without external dependencies
   - Some K8s client tests require kubeconfig setup

2. Integration Tests:
   - Some tests require a running Kubernetes cluster
   - Mock implementations available for cluster-dependent tests

3. Performance Tests:
   - May require specific system resources
   - Configurable thresholds for different environments

## Test Coverage Goals

1. Component Coverage:
   - CLI argument parsing: 100%
   - Kubernetes client operations: 90%
   - Log processing and filtering: 95%
   - Output formatting: 100%

2. Integration Coverage:
   - End-to-end workflows: 85%
   - Error conditions: 90%
   - Edge cases: 90%

3. Performance Metrics:
   - Log processing: >1000 entries/second
   - Concurrent streams: >10 simultaneous
   - Memory usage: <50MB for normal operation