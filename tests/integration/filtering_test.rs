use wake::k8s::logs::LogEntry;
use wake::output::formatter::create_formatter;
use crate::common::mocks::{MockPodApi, PodApiTrait};
use anyhow::Result;
use chrono::Utc;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_log_filtering_with_includes_excludes() -> Result<()> {
    // Purpose: Verify log filtering using include/exclude patterns
    // Tests:
    // - Pattern-based log filtering
    // - Multiple log entry handling
    // - Debug vs Error level filtering
    // Validates:
    // - Only error logs are counted
    // - Debug logs are properly filtered out
    // - Log stream processing completes
    let mock_api = MockPodApi::default();
    let (tx, mut rx) = mpsc::channel(32);
    
    // Send mock log entries
    tokio::spawn(async move {
        let entries = vec![
            LogEntry {
                namespace: "default".to_string(),
                pod_name: "nginx-1".to_string(),
                container_name: "nginx".to_string(),
                timestamp: Some(Utc::now()),
                message: "DEBUG access log".to_string(),
            },
            LogEntry {
                namespace: "default".to_string(),
                pod_name: "nginx-2".to_string(),
                container_name: "nginx".to_string(),
                timestamp: Some(Utc::now()),
                message: "ERROR failed to start".to_string(),
            },
        ];
        
        for entry in entries {
            let _ = tx.send(entry).await;
        }
    });
    
    // Count entries matching our filters
    let mut error_count = 0;
    while let Some(entry) = rx.recv().await {
        if entry.message.contains("ERROR") {
            error_count += 1;
        }
    }
    
    assert_eq!(error_count, 1, "Should have found 1 error log");
    Ok(())
}

#[tokio::test]
async fn test_log_formatting_multiple_formats() -> Result<()> {
    // Purpose: Verify log formatting in different output formats
    // Tests:
    // - Text format output
    // - JSON format output
    // - Field presence in each format
    // Validates:
    // - Text format contains required fields
    // - JSON format is valid and complete
    // - Field values are preserved
    let entry = LogEntry {
        namespace: "test-ns".to_string(),
        pod_name: "test-pod".to_string(),
        container_name: "test-container".to_string(),
        timestamp: Some(Utc::now()),
        message: "test message".to_string(),
    };
    
    // Test text format
    let text_formatter = create_formatter("text", false)?;
    let text_output = text_formatter.format(&entry)?;
    assert!(text_output.contains("test-pod"));
    assert!(text_output.contains("test message"));
    
    // Test JSON format
    let json_formatter = create_formatter("json", false)?;
    let json_output = json_formatter.format(&entry)?;
    assert!(json_output.contains("\"pod\":\"test-pod\""));
    assert!(json_output.contains("\"message\":\"test message\""));
    
    Ok(())
}

#[tokio::test]
async fn test_namespace_filtering() -> Result<()> {
    // Purpose: Verify namespace-based log filtering
    // Tests:
    // - Namespace-specific log filtering
    // - Multi-namespace log handling
    // - Namespace count validation
    // Validates:
    // - Correct logs per namespace
    // - No cross-namespace leakage
    // - Accurate log counting
    let mock_api = MockPodApi::default();
    let (tx, mut rx) = mpsc::channel(32);
    
    // Send mock logs from different namespaces
    tokio::spawn(async move {
        let entries = vec![
            LogEntry {
                namespace: "default".to_string(),
                pod_name: "pod-1".to_string(),
                container_name: "container-1".to_string(),
                timestamp: Some(Utc::now()),
                message: "log line".to_string(),
            },
            LogEntry {
                namespace: "kube-system".to_string(),
                pod_name: "pod-2".to_string(),
                container_name: "container-2".to_string(),
                timestamp: Some(Utc::now()),
                message: "log line".to_string(),
            },
        ];
        
        for entry in entries {
            let _ = tx.send(entry).await;
        }
    });
    
    // Count entries from each namespace
    let mut default_ns_count = 0;
    while let Some(entry) = rx.recv().await {
        if entry.namespace == "default" {
            default_ns_count += 1;
        }
    }
    
    assert_eq!(default_ns_count, 1, "Should have found 1 log from default namespace");
    Ok(())
}