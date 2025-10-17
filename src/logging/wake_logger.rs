use tracing::{debug, error, info, warn};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::Mutex;
use std::path::Path;
use anyhow::Result;
use once_cell::sync::OnceCell;

// Global flag to determine if we're in development mode
static mut IS_DEV_MODE: bool = false;

// Global file logger handle
static FILE_LOGGER: OnceCell<Mutex<Option<File>>> = OnceCell::new();

/// Initialize the logger with dev mode setting and optional log file
pub fn init(dev_mode: bool, log_file_path: Option<&str>) -> Result<()> {
    // SAFETY: This is safe because it's only called during initialization
    unsafe {
        IS_DEV_MODE = dev_mode;
    }
    
    // Initialize the file logger if a path is provided
    if let Some(path) = log_file_path {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
            
        FILE_LOGGER.set(Mutex::new(Some(file)))
            .map_err(|_| anyhow::anyhow!("Failed to initialize file logger"))?;
            
        info!("Wake logger initialized with dev_mode: {} and log file: {}", dev_mode, path);
    } else {
        FILE_LOGGER.set(Mutex::new(None))
            .map_err(|_| anyhow::anyhow!("Failed to initialize file logger"))?;
            
        info!("Wake logger initialized with dev_mode: {}", dev_mode);
    }
    
    Ok(())
}

/// Write to the file logger if available
fn write_to_file(level: &str, message: &str) -> Result<()> {
    if let Some(file_logger) = FILE_LOGGER.get() {
        if let Some(ref mut file) = *file_logger.lock().unwrap() {
            writeln!(file, "[{level}] {message}")?;
            file.flush()?;
        }
    }
    Ok(())
}

/// Log a message at info level if in dev mode
pub fn info(message: &str) {
    // SAFETY: Reading a static during runtime
    let dev_mode = unsafe { IS_DEV_MODE };
    info!("{}", message);
    if dev_mode {
        println!("[INFO] {message}");
    }
    let _ = write_to_file("INFO", message);
}

/// Log a message at debug level if in dev mode
pub fn debug(message: &str) {
    // SAFETY: Reading a static during runtime
    let dev_mode = unsafe { IS_DEV_MODE };
    debug!("{}", message);
    if dev_mode {
        println!("[DEBUG] {message}");
    }
    let _ = write_to_file("DEBUG", message);
}

/// Log a message at warning level if in dev mode
pub fn warn(message: &str) {
    // SAFETY: Reading a static during runtime
    let dev_mode = unsafe { IS_DEV_MODE };
    warn!("{}", message);
    if dev_mode {
        println!("[WARNING] {message}");
    }
    let _ = write_to_file("WARNING", message);
}

/// Log a message at error level - always printed regardless of dev mode
pub fn error(message: &str) {
    error!("{}", message);
    // Errors are always printed to stderr
    eprintln!("[ERROR] {message}");
    let _ = write_to_file("ERROR", message);
}

/// Log a message only if in dev mode - for direct replacements of println!
pub fn dev_println(message: &str) {
    // SAFETY: Reading a static during runtime
    let dev_mode = unsafe { IS_DEV_MODE };
    debug!("{}", message);
    if dev_mode {
        println!("{message}");
    }
    let _ = write_to_file("DEBUG", message);
}