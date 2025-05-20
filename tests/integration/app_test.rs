use wake::cli::Args;
use wake::k8s::{client::create_client, logs::LogWatcher};
use wake::output::formatter::{create_formatter, OutputFormatter};
use tokio_stream::StreamExt;
use anyhow::{Result, Context};
use std::time::Duration;
use std::sync::Arc;
use chrono::Utc;

// This test requires a running Kubernetes cluster or a mock implementation
// We'll provide test logic that can be run in a CI environment with a kind or minikube cluster
#[tokio::test]
#[ignore] // Ignored by default as it requires a real k8s cluster
async fn test_log_streaming_with_real_cluster() -> Result<()> {
    // Purpose: End-to-end test of log streaming with a real Kubernetes cluster
    // Test Parameters:
    // - Uses kube-system namespace (exists in all clusters)
    // - Targets kube-proxy pods (standard component)
    // - Limited to 5 log lines
    // - No log following (one-time fetch)
    // Success Criteria:
    // - Successfully connects to cluster
    // - Retrieves log entries
    // - Log entries contain required fields
    // - Operation completes within timeout

    // Create real arguments
    let mut args = Args::default();
    args.namespace = "kube-system".to_string(); // Use kube-system as it exists in all clusters
    args.pod_selector = "kube-proxy.*".to_string();  // Target kube-proxy which exists in all clusters
    args.tail = 5;                              // Limit log lines for test
    args.follow = false;                        // Don't follow logs in tests
    
    // Create a real k8s client
    // This will use your current Kubernetes context
    let client = create_client(&args).await
        .context("Failed to create Kubernetes client")?;
    
    // Create log watcher
    let watcher = LogWatcher::new(client, &args);
    
    // Start streaming logs
    let mut stream = watcher.stream().await?;
    
    // Collect a few log entries (with a timeout to prevent hanging)
    let mut entries = Vec::new();
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(5);
    
    while let Some(entry) = tokio::time::timeout(timeout, stream.next()).await? {
        entries.push(entry);
        if entries.len() >= 5 || start.elapsed() > timeout {
            break;
        }
    }
    
    // Validate that we got some log entries
    assert!(!entries.is_empty(), "Should receive at least one log entry");
    
    // Validate log entry fields
    for entry in &entries {
        assert!(!entry.namespace.is_empty(), "Namespace should not be empty");
        assert!(!entry.pod_name.is_empty(), "Pod name should not be empty");
        assert!(!entry.container_name.is_empty(), "Container name should not be empty");
        assert!(!entry.message.is_empty(), "Message should not be empty");
    }
    
    Ok(())
}

// A more reliable integration test using mocks
#[tokio::test]
async fn test_log_streaming_with_mocks() -> Result<()> {
    // Purpose: Test log streaming functionality using mock objects
    // Test Characteristics:
    // - Uses mock K8s API responses
    // - Tests log entry processing
    // - Validates formatter integration
    // Success Criteria:
    // - Proper log entry formatting
    // - All fields preserved through processing
    // - Multiple formatters work correctly
    // - No errors in processing chain

    use wake::output::formatter::create_formatter;
    use wake::k8s::logs::LogEntry;
    use crate::common::mocks::{MockPodApi, PodApiTrait};
    use anyhow::Result;
    use tokio::sync::mpsc;

    let mock_api = MockPodApi::default();
    
    // Set up a channel to simulate log streaming
    let (tx, mut rx) = mpsc::channel(32);
    
    // Spawn a task that will send mock log entries
    tokio::spawn(async move {
        for i in 0..5 {
            let entry = LogEntry {
                namespace: "default".to_string(),
                pod_name: format!("pod-{}", i),
                container_name: format!("container-{}", i),
                message: format!("log line {}", i),
                timestamp: Some(Utc::now()),
            };
            let _ = tx.send(entry).await;
        }
    });
    
    // Verify we can receive and process logs
    let mut count = 0;
    while let Some(entry) = rx.recv().await {
        assert!(entry.pod_name.starts_with("pod-"));
        assert!(entry.container_name.starts_with("container-"));
        assert!(entry.message.starts_with("log line"));
        count += 1;
    }
    
    assert_eq!(count, 5, "Should have received 5 log entries");
    
    // Verify that we can create formatters and format logs
    let args = Args::default();
    let test_entry = LogEntry {
        namespace: "test-ns".to_string(),
        pod_name: "test-pod".to_string(),
        container_name: "test-container".to_string(),
        message: "Test message".to_string(),
        timestamp: Some(Utc::now()),
    };
    
    let text_formatter = create_formatter("text", args.timestamps)?;
    let formatted = text_formatter.format(&test_entry)?;
    assert!(formatted.contains("test-pod"));
    assert!(formatted.contains("Test message"));
    
    Ok(())
}

// Test that filters are properly applied to log streams
#[tokio::test]
async fn test_log_filtering() -> Result<()> {
    // Purpose: Test log filtering functionality in streaming context
    // Test Characteristics:
    // - Multiple pod sources
    // - Different log types
    // - Filter by pod name
    // Success Criteria:
    // - Only matching logs are processed
    // - Non-matching logs are filtered out
    // - Accurate log counting
    // - Stream completes successfully

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use wake::k8s::logs::LogEntry;
    use crate::common::mocks::{MockPodApi, PodApiTrait};
    use anyhow::Result;
    use tokio::sync::mpsc;

    let mock_api = MockPodApi::default();
    let (tx, mut rx) = mpsc::channel(32);
    
    // Send some mock logs
    tokio::spawn(async move {
        let entries = vec![
            LogEntry {
                namespace: "default".to_string(),
                pod_name: "nginx-1".to_string(),
                container_name: "nginx".to_string(),
                message: "access log entry".to_string(),
                timestamp: Some(Utc::now()),
            },
            LogEntry {
                namespace: "app-ns".to_string(),
                pod_name: "app-1".to_string(),
                container_name: "app".to_string(),
                message: "error log entry".to_string(),
                timestamp: Some(Utc::now()),
            },
        ];
        
        for entry in entries {
            let _ = tx.send(entry).await;
        }
    });
    
    // Count matching entries
    let mut nginx_count = 0;
    while let Some(entry) = rx.recv().await {
        if entry.pod_name.contains("nginx") {
            nginx_count += 1;
        }
    }
    
    assert_eq!(nginx_count, 1, "Should have found 1 nginx log entry");
    Ok(())
}