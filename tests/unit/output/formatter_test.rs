use wake::k8s::logs::LogEntry;
use wake::output::formatter::{OutputFormatter, TextFormatter, JsonFormatter, RawFormatter};
use chrono::{DateTime, Utc, TimeZone};
use std::sync::Arc;
use anyhow::Result;

// Helper function to create a test log entry
fn create_test_log_entry() -> LogEntry {
    // Creates a consistent log entry for testing with known values:
    // - namespace: test-ns
    // - pod: test-pod
    // - container: test-container
    // - message: test message
    // - timestamp: 2023-05-15 12:30:45 UTC
    LogEntry {
        namespace: "test-ns".to_string(),
        pod_name: "test-pod".to_string(),
        container_name: "test-container".to_string(),
        message: "This is a test log message".to_string(),
        timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 12, 30, 45).unwrap()),
    }
}

#[test]
fn test_text_formatter() -> Result<()> {
    // Purpose: Verify text formatting of log entries without timestamps
    // Tests:
    // - Correct format: [namespace/pod/container] message
    // - Proper field placement and separation
    // - No timestamp included when disabled
    let entry = create_test_log_entry();
    let formatter = TextFormatter::new(false); // without timestamps
    
    let formatted = formatter.format(&entry)?;
    
    // Text format should include namespace, pod name, container name and message
    // in a specific format: [namespace/pod/container] message
    let expected = format!("[{}/{}/{}] {}", 
        entry.namespace,
        entry.pod_name,
        entry.container_name,
        entry.message
    );
    assert_eq!(formatted, expected);
    
    Ok(())
}

#[test]
fn test_text_formatter_with_timestamp() -> Result<()> {
    // Purpose: Verify text formatting of log entries with timestamps enabled
    // Tests:
    // - Timestamp format correctness (YYYY-MM-DD HH:MM:SS)
    // - Timestamp placement in output
    // - Complete log entry format with timestamp
    let entry = create_test_log_entry();
    let formatter = TextFormatter::new(true); // with timestamps
    
    let formatted = formatter.format(&entry)?;
    
    // Should contain timestamp
    assert!(formatted.contains("2023-05-15"));
    assert!(formatted.contains("12:30:45"));
    
    Ok(())
}

#[test]
fn test_json_formatter() -> Result<()> {
    // Purpose: Verify JSON formatting of log entries
    // Tests:
    // - Valid JSON output structure
    // - All fields present in JSON
    // - Correct field values
    // - Proper timestamp serialization
    let entry = create_test_log_entry();
    let formatter = JsonFormatter::new();
    
    let formatted = formatter.format(&entry)?;
    
    // Should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&formatted)?;
    
    // Check JSON fields
    assert_eq!(json["namespace"], "test-ns");
    assert_eq!(json["pod"], "test-pod");
    assert_eq!(json["container"], "test-container");
    assert_eq!(json["message"], "This is a test log message");
    
    // Should include timestamp
    assert!(json["timestamp"].is_string());
    assert!(json["timestamp"].as_str().unwrap().contains("2023-05-15"));
    
    Ok(())
}

#[test]
fn test_raw_formatter() -> Result<()> {
    // Purpose: Verify raw formatting (message-only output)
    // Tests:
    // - Only message content included
    // - No metadata or formatting added
    // - Direct message passthrough
    let entry = create_test_log_entry();
    let formatter = RawFormatter::new();
    
    let formatted = formatter.format(&entry)?;
    
    // Raw formatter should just return the message
    assert_eq!(formatted, "This is a test log message");
    
    Ok(())
}

#[test]
fn test_formatter_factory() -> Result<()> {
    // Purpose: Verify formatter creation based on format type
    // Tests:
    // - Creation of each formatter type (text, json, raw)
    // - Correct behavior of created formatters
    // - Error handling for invalid formatter types
    use wake::output::formatter::create_formatter;
    
    // Test creation of different formatter types
    let text_formatter = create_formatter("text", false)?;
    let json_formatter = create_formatter("json", false)?;
    let raw_formatter = create_formatter("raw", false)?;
    
    // Verify they are the correct types by checking formatting behavior
    let entry = create_test_log_entry();
    
    // Text formatter should include pod and container names
    assert!(text_formatter.format(&entry)?.contains("test-pod"));
    
    // JSON formatter should produce valid JSON
    let json_output = json_formatter.format(&entry)?;
    assert!(serde_json::from_str::<serde_json::Value>(&json_output).is_ok());
    
    // Raw formatter should only return the message
    assert_eq!(raw_formatter.format(&entry)?, "This is a test log message");
    
    // Invalid formatter type should return an error
    assert!(create_formatter("invalid", false).is_err());
    
    Ok(())
}

use crate::output::formatter::{LogFormatter, FormatterType};
use crate::k8s::logs::LogEntry;
use chrono::{DateTime, Utc};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_formatter() {
        let timestamp = DateTime::parse_from_rfc3339("2023-06-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let entry = LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "main".to_string(),
            message: "Test log message".to_string(),
            timestamp: Some(timestamp),
        };

        let formatter = LogFormatter::new(FormatterType::Text, true);
        let formatted = formatter.format(&entry);

        assert!(formatted.contains("test-pod"));
        assert!(formatted.contains("main"));
        assert!(formatted.contains("Test log message"));
        assert!(formatted.contains("2023-06-15"));
    }

    #[test]
    fn test_json_formatter() {
        let timestamp = DateTime::parse_from_rfc3339("2023-06-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        let entry = LogEntry {
            namespace: "production".to_string(),
            pod_name: "api-server".to_string(),
            container_name: "api".to_string(),
            message: "API request processed".to_string(),
            timestamp: Some(timestamp),
        };

        let formatter = LogFormatter::new(FormatterType::Json, true);
        let formatted = formatter.format(&entry);

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["namespace"], "production");
        assert_eq!(parsed["pod_name"], "api-server");
        assert_eq!(parsed["container_name"], "api");
        assert_eq!(parsed["message"], "API request processed");
    }

    #[test]
    fn test_raw_formatter() {
        let entry = LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "main".to_string(),
            message: "Raw log message".to_string(),
            timestamp: Some(Utc::now()),
        };

        let formatter = LogFormatter::new(FormatterType::Raw, false);
        let formatted = formatter.format(&entry);

        // Raw format should only contain the message
        assert_eq!(formatted.trim(), "Raw log message");
    }

    #[test]
    fn test_formatter_without_timestamps() {
        let entry = LogEntry {
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_name: "main".to_string(),
            message: "Message without timestamp".to_string(),
            timestamp: None,
        };

        let formatter = LogFormatter::new(FormatterType::Text, true);
        let formatted = formatter.format(&entry);

        assert!(formatted.contains("test-pod"));
        assert!(formatted.contains("Message without timestamp"));
        // Should not contain timestamp info when none provided
    }

    #[test]
    fn test_multiline_message_formatting() {
        let entry = LogEntry {
            namespace: "default".to_string(),
            pod_name: "debug-pod".to_string(),
            container_name: "debug".to_string(),
            message: "Line 1\nLine 2\nLine 3".to_string(),
            timestamp: Some(Utc::now()),
        };

        let formatter = LogFormatter::new(FormatterType::Text, true);
        let formatted = formatter.format(&entry);

        assert!(formatted.contains("Line 1"));
        assert!(formatted.contains("Line 2"));
        assert!(formatted.contains("Line 3"));
    }

    #[test]
    fn test_special_characters_in_message() {
        let entry = LogEntry {
            namespace: "test".to_string(),
            pod_name: "special-pod".to_string(),
            container_name: "app".to_string(),
            message: "Message with \"quotes\" and 'apostrophes' and emojis 🚀".to_string(),
            timestamp: Some(Utc::now()),
        };

        let json_formatter = LogFormatter::new(FormatterType::Json, true);
        let json_formatted = json_formatter.format(&entry);

        // Should be valid JSON even with special characters
        let parsed: serde_json::Value = serde_json::from_str(&json_formatted).unwrap();
        assert!(parsed["message"].as_str().unwrap().contains("quotes"));
        assert!(parsed["message"].as_str().unwrap().contains("🚀"));
    }
}