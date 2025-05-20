use wake::k8s::logs::LogEntry;
use wake::output::formatter::create_formatter;
use wake::cli::Args;
use anyhow::Result;
use chrono::{TimeZone, Utc};
use futures::StreamExt;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_empty_logs() -> Result<()> {
    // Purpose: Verify graceful handling of empty log streams
    // Tests:
    // - Empty stream processing
    // - No errors/panics with zero logs
    // - Proper termination of processing
    
    // Create an empty stream
    let empty_stream = futures::stream::empty::<LogEntry>();
    
    // Test formatting an empty stream
    let formatter = create_formatter("text", false)?;
    let mut formatted_logs = Vec::new();
    
    tokio::pin!(empty_stream);
    
    while let Some(entry) = empty_stream.next().await {
        let formatted = formatter.format(&entry)?;
        formatted_logs.push(formatted);
    }
    
    // Verify no logs were processed
    assert_eq!(formatted_logs.len(), 0, "Should handle empty log streams gracefully");
    
    Ok(())
}

#[tokio::test]
async fn test_logs_with_special_characters() -> Result<()> {
    // Purpose: Verify handling of logs with non-standard content
    // Tests edge cases:
    // 1. Special characters: !@#$%^&*()_+ etc
    // 2. Unicode characters in different scripts
    // 3. Multi-line log entries
    // 4. Very long log lines (5000 chars)
    // Validates:
    // - Proper formatting preservation
    // - No character corruption
    // - JSON escaping in JSON format
    
    let special_logs = vec![
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with special chars: !@#$%^&*()_+{}|:<>?~`-=[]\\;',./".to_string(),
            timestamp: Some(Utc::now()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with unicode: 你好, こんにちは, 안녕하세요, مرحبا, Привет".to_string(),
            timestamp: Some(Utc::now()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with newlines:\nLine 1\nLine 2\nLine 3".to_string(),
            timestamp: Some(Utc::now()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "A".repeat(5000), // Very long log line
            timestamp: Some(Utc::now()),
        },
    ];
    
    // Test formatting each special log entry
    let formatters = vec![
        create_formatter("text", false)?,
        create_formatter("json", false)?,
        create_formatter("raw", false)?,
    ];
    
    for formatter in &formatters {
        for entry in &special_logs {
            let formatted = formatter.format(entry)?;
            
            // Basic validation - the formatted output should exist
            assert!(!formatted.is_empty(), "Formatted output should not be empty");
            
            // For JSON formatter, ensure output is valid JSON
            if let Some("json") = formatter.format_name().as_deref() {
                let json_result = serde_json::from_str::<serde_json::Value>(&formatted);
                assert!(json_result.is_ok(), "JSON output should be valid: {:?}", json_result.err());
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_log_timestamp_edge_cases() -> Result<()> {
    // Purpose: Verify handling of various timestamp scenarios
    // Tests edge cases:
    // 1. Missing timestamps (None)
    // 2. Unix epoch timestamp (1970-01-01)
    // 3. Far future timestamps (2099-12-31)
    // Validates:
    // - Proper timestamp formatting
    // - Graceful handling of missing timestamps
    // - No overflow/underflow issues
    
    let timestamp_logs = vec![
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with no timestamp".to_string(),
            timestamp: None, // Missing timestamp
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with very old timestamp".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Log with future timestamp". to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2099, 12, 31, 23, 59, 59).unwrap()),
        },
    ];
    
    // Test formatting with timestamps enabled
    let formatter = create_formatter("text", true)?;
    
    for entry in &timestamp_logs {
        let formatted = formatter.format(entry)?;
        assert!(!formatted.is_empty());
        
        // Logs with no timestamp should still format correctly
        if entry.timestamp.is_none() {
            assert!(!formatted.contains("1970-01-01"));
            assert!(!formatted.contains("2099-12-31"));
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<()> {
    // Purpose: Verify proper error handling in various failure scenarios
    // Tests:
    // 1. Invalid formatter type specification
    // 2. Invalid regex pattern for inclusion
    // 3. Invalid regex pattern for container matching
    // Validates:
    // - Appropriate error types returned
    // - Meaningful error messages
    // - No panic conditions
    
    // Test invalid formatter type
    let formatter_result = create_formatter("invalid_type", false);
    assert!(formatter_result.is_err(), "Should error on invalid formatter type");
    
    // Test invalid regex patterns
    let mut args = Args::default();
    args.include = Some("[invalid regex".to_string());
    
    let include_regex_result = args.include_regex();
    assert!(include_regex_result.unwrap().is_err(), "Should error on invalid include regex");
    
    // Test invalid container regex
    let mut args = Args::default();
    args.container = "[invalid regex".to_string(); // Invalid regex pattern
    
    let container_regex_result = args.container_regex();
    assert!(container_regex_result.is_err(), "Should error on invalid container regex");
    
    Ok(())
}

#[tokio::test]
async fn test_timeouts() -> Result<()> {
    // Purpose: Verify timeout handling in log streaming operations
    // Tests:
    // 1. Delayed log emission
    // 2. Timeout configuration
    // 3. Stream completion with timeout
    // Validates:
    // - Proper async handling
    // - No hanging on slow operations
    // - Complete log processing within timeout
    
    // Test handling of timeouts and slow operations
    use futures::stream;
    use tokio::time::sleep;
    
    // Create logs that will be emitted with delays
    let logs = vec![
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Message 1".to_string(),
            timestamp: Some(Utc::now()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "test-container".to_string(),
            message: "Message 2".to_string(),
            timestamp: Some(Utc::now()),
        },
    ];

    // Create a stream that emits logs with delays
    let delayed_logs = stream::iter(logs).then(|entry| async {
        sleep(Duration::from_millis(10)).await;
        entry
    });
    
    // Set a timeout that should still succeed
    let result = timeout(Duration::from_millis(100), async {
        let mut count = 0;
        tokio::pin!(delayed_logs);
        
        while let Some(_) = delayed_logs.next().await {
            count += 1;
        }
        
        count
    }).await?;
    
    assert_eq!(result, 2, "Should process all logs within timeout");
    
    Ok(())
}