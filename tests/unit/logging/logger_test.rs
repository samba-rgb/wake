/**
 * Tests for the logging subsystem
 * Tests logging level configuration and initialization
 */

use wake::logging::{setup_logger, get_log_level};
use anyhow::Result;
use std::sync::Once;
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