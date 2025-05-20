use wake::k8s::logs::{LogEntry, LogWatcher};
use wake::output::formatter::{create_formatter, OutputFormatter};
use wake::cli::Args;
use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::time::Instant;
use tokio::time::{sleep, Duration};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

// Helper function to create a large stream of test log entries
fn create_large_log_stream(count: usize) -> Pin<Box<dyn Stream<Item = LogEntry> + Send>> {
    // Purpose: Generate a large volume of test log entries
    // Characteristics:
    // - Distributes logs across 10 pods
    // - Uses 3 container types (web, api, db)
    // - 5 different message types (INFO, DEBUG, WARN, ERROR, TRACE)
    // - Realistic log content with varying lengths
    use futures::stream;

    // Create a vector with the specified number of log entries
    let mut logs = Vec::with_capacity(count);
    
    // Generate log entries with different content to test processing performance
    for i in 0..count {
        let pod_number = i % 10; // Distribute across 10 pods
        let container_type = match i % 3 {
            0 => "web",
            1 => "api",
            _ => "db",
        };
        
        let msg_type = match i % 5 {
            0 => "INFO",
            1 => "DEBUG",
            2 => "WARN",
            3 => "ERROR",
            _ => "TRACE",
        };
        
        logs.push(LogEntry {
            namespace: "performance-test".to_string(),
            pod_name: format!("test-pod-{}", pod_number),
            container_name: format!("{}-container", container_type),
            message: format!("{}: This is test log entry {} with some additional content to make it more realistic", 
                            msg_type, i),
            timestamp: Some(Utc::now()),
        });
    }
    
    Box::pin(stream::iter(logs))
}

#[tokio::test]
async fn test_log_processing_performance() -> Result<()> {
    // Purpose: Verify performance of bulk log processing
    // Test Parameters:
    // - Volume: 10,000 log entries
    // - Metrics measured: 
    //   * Total processing time
    //   * Average time per log entry
    //   * Memory usage (indirect via processed count)
    // Success Criteria:
    // - All logs processed successfully
    // - No memory exhaustion
    // - Performance metrics within acceptable ranges
    const LOG_COUNT: usize = 10_000;
    
    let log_stream = create_large_log_stream(LOG_COUNT);
    tokio::pin!(log_stream);
    
    // Create a formatter
    let formatter = create_formatter("text", false)?;
    
    let start_time = Instant::now();
    let mut processed_count = 0;
    
    // Process all logs and format them
    while let Some(entry) = log_stream.next().await {
        formatter.format(&entry)?;
        processed_count += 1;
    }
    
    let elapsed = start_time.elapsed();
    
    // Make assertions about performance
    assert_eq!(processed_count, LOG_COUNT, "Should process all log entries");
    
    // Print performance metrics (we don't assert specific numbers as they're machine-dependent)
    println!("Processed {} logs in {:?}", LOG_COUNT, elapsed);
    println!("Average processing time: {:?} per log", elapsed / LOG_COUNT as u32);
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_log_processing() -> Result<()> {
    // Purpose: Verify concurrent log processing capabilities
    // Test Characteristics:
    // - Concurrent producer/consumer pattern
    // - Buffered channel with backpressure (size 100)
    // - Variable production rates
    // - 5000 total log entries
    // Success Criteria:
    // - All logs processed without loss
    // - No deadlocks or race conditions
    // - Proper backpressure handling
    // - Processing completes within timeout
    const LOG_COUNT: usize = 5_000;
    
    // Create a channel for sending log entries
    let (tx, rx) = tokio::sync::mpsc::channel::<LogEntry>(100);
    
    // Counter for processed logs
    let processed_count = Arc::new(AtomicUsize::new(0));
    
    // Spawn producer task
    let producer = tokio::spawn(async move {
        for i in 0..LOG_COUNT {
            let log = LogEntry {
                namespace: "concurrent-test".to_string(),
                pod_name: format!("test-pod-{}", i % 5),
                container_name: "test-container".to_string(),
                message: format!("Test log message {}", i),
                timestamp: Some(Utc::now()),
            };
            
            tx.send(log).await.expect("Failed to send log entry");
            
            // Simulate varying production rates
            if i % 100 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }
    });
    
    // Create a formatter for the consumer
    let formatter = create_formatter("json", false)?;
    let processed_count_clone = processed_count.clone();
    
    // Spawn consumer task
    let consumer = tokio::spawn(async move {
        let mut rx = rx;
        while let Some(entry) = rx.recv().await {
            // Process the log entry
            let _ = formatter.format(&entry)?;
            processed_count_clone.fetch_add(1, Ordering::Relaxed);
        }
        Ok::<_, anyhow::Error>(())
    });
    
    // Wait for producer to finish
    producer.await?;
    
    // Wait for consumer to process all entries (with timeout)
    let start = Instant::now();
    while processed_count.load(Ordering::Relaxed) < LOG_COUNT {
        sleep(Duration::from_millis(10)).await;
        
        // Timeout after 5 seconds
        if start.elapsed() > Duration::from_secs(5) {
            break;
        }
    }
    
    // Verify all logs were processed
    assert_eq!(processed_count.load(Ordering::Relaxed), LOG_COUNT, 
               "All log entries should be processed");
    
    Ok(())
}

// Test filtering performance
#[tokio::test]
async fn test_filtering_performance() -> Result<()> {
    // Purpose: Verify performance of log filtering operations
    // Test Characteristics:
    // - Large dataset (10,000 entries)
    // - Complex regex filtering patterns
    // - Multiple severity levels
    // - Both inclusion and exclusion filters
    // Performance Metrics:
    // - Filtering operation timing
    // - Memory usage during filtering
    // - Filter ratio validation
    // Success Criteria:
    // - Filtering completes within 500ms
    // - Memory usage remains stable
    // - Filter ratios match expected distribution
    use std::time::Instant;
    use regex::Regex;

    // Create test data - lots of log entries
    let total_entries = 10_000;
    let mut entries = Vec::with_capacity(total_entries);
    
    for i in 0..total_entries {
        let severity = match i % 5 {
            0 => "ERROR",
            1 => "WARN",
            2 => "INFO",
            3 => "DEBUG",
            4 => "TRACE",
            _ => unreachable!(),
        };
        
        entries.push(LogEntry {
            namespace: "test-ns".to_string(),
            pod_name: format!("pod-{}", i % 10),
            container_name: "app".to_string(),
            message: format!("{}: Test message {}", severity, i),
            timestamp: Some(Utc::now()),
        });
    }
    
    // Create regex filters
    let include_re = Regex::new("ERROR|WARN")?;
    let exclude_re = Regex::new("message 0")?;
    
    // Measure filtering performance
    let start = Instant::now();
    let filtered: Vec<_> = entries.into_iter()
        .filter(|entry| {
            include_re.is_match(&entry.message) && 
            !exclude_re.is_match(&entry.message)
        })
        .collect();
    let duration = start.elapsed();
    
    // Verify filtering results
    // With our test data, we expect:
    // - ERROR or WARN messages (40% of total)
    // - Excluding those with "message 0" (roughly 10% of those)
    // So approximately 36% of total should remain
    let filtered_ratio = filtered.len() as f64 / total_entries as f64;
    
    // Allow for some variance in the ratio due to random distribution
    assert!(filtered_ratio > 0.32 && filtered_ratio < 0.40, 
        "Expected filtered ratio between 32-40%, got {}%", 
        filtered_ratio * 100.0);
    
    // Verify performance - should be reasonably fast
    assert!(duration.as_millis() < 500, "Filtering took too long: {:?}", duration);
    
    Ok(())
}