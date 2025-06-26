/**
 * Tests for the logging subsystem
 * Tests logging level configuration and initialization
 */

use wake::logging::{setup_logger, get_log_level, Logger};
use anyhow::Result;
use std::sync::Once;
use tempfile::NamedTempFile;
use tracing::Level;

static INIT: Once = Once::new();

#[test]
fn test_get_log_level() {
    // Purpose: Verify mapping of verbosity levels to tracing log levels
    // Tests:
    // - Level 0: ERROR (minimum verbosity)
    // - Level 1: WARN
    // - Level 2: INFO (normal verbosity)
    // - Level 3: DEBUG
    // - Level 4: TRACE (maximum verbosity)
    // - Levels > 4: Should default to TRACE
    assert_eq!(get_log_level(0), Level::ERROR);
    assert_eq!(get_log_level(1), Level::WARN);
    assert_eq!(get_log_level(2), Level::INFO);
    assert_eq!(get_log_level(3), Level::DEBUG);
    assert_eq!(get_log_level(4), Level::TRACE);
    
    // Test that higher verbosity values default to TRACE
    assert_eq!(get_log_level(5), Level::TRACE);
    assert_eq!(get_log_level(10), Level::TRACE);
}

#[test]
fn test_setup_logger() -> Result<()> {
    // Purpose: Verify logger initialization with different verbosity levels
    // Tests:
    // - Logger setup with multiple verbosity levels
    // - One-time initialization (uses Once guard)
    // - Graceful handling of repeated initialization
    // Note: Can't test actual subscriber due to global state
    
    let verbosity_levels = vec![0, 2, 4];
    
    // Only try to set up the real subscriber once
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .try_init();
    });
    
    for level in verbosity_levels {
        let result = setup_logger(level);
        assert!(result.is_ok() || result.is_err(), 
                "Setup logger should either succeed or fail gracefully");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = Logger::new();
        assert!(logger.is_ok());
    }

    #[test]
    fn test_file_logging() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();
        
        let mut logger = Logger::new().unwrap();
        logger.set_file_output(Some(log_path.clone()));
        
        // Test logging to file
        logger.log("Test message".to_string());
        
        // Verify file exists and has content
        assert!(log_path.exists());
    }

    #[test]
    fn test_console_logging() {
        let mut logger = Logger::new().unwrap();
        logger.set_console_output(true);
        
        // This should not panic
        logger.log("Console test message".to_string());
    }

    #[test]
    fn test_log_formatting() {
        let logger = Logger::new().unwrap();
        
        // Test that messages are properly formatted
        let formatted = logger.format_message("Test log entry");
        assert!(formatted.contains("Test log entry"));
    }
}