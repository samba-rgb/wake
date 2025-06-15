use crate::k8s::logs::LogEntry;
use std::sync::Arc;
use regex::Regex;
use threadpool::ThreadPool;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

/// Simple pattern that supports regex matching
#[derive(Debug, Clone)]
pub enum FilterPattern {
    Simple(Regex),
}

impl FilterPattern {
    /// Parse a pattern string as a simple regex
    #[allow(dead_code)]
    pub fn parse(pattern: &str) -> Result<Self, String> {
        match Regex::new(pattern.trim()) {
            Ok(regex) => Ok(FilterPattern::Simple(regex)),
            Err(e) => Err(format!("Invalid regex pattern '{}': {}", pattern.trim(), e)),
        }
    }
    
    /// Check if a log message matches this pattern
    pub fn matches(&self, message: &str) -> bool {
        match self {
            FilterPattern::Simple(regex) => regex.is_match(message),
        }
    }
}

/// Handles log filtering operations in a dedicated thread pool
pub struct LogFilter {
    include_pattern: Option<Arc<FilterPattern>>,
    exclude_pattern: Option<Arc<FilterPattern>>,
    thread_pool: ThreadPool,
}

impl LogFilter {
    /// Creates a new log filter with the specified patterns and thread count
    pub fn new(
        include_pattern: Option<Regex>,
        exclude_pattern: Option<Regex>,
        num_threads: usize,
    ) -> Self {
        // Convert patterns to Arc for safe sharing between threads
        let include_arc = include_pattern.map(|p| Arc::new(FilterPattern::Simple(p)));
        let exclude_arc = exclude_pattern.map(|p| Arc::new(FilterPattern::Simple(p)));

        // Create a thread pool with the specified number of threads
        let thread_pool = ThreadPool::new(num_threads);

        info!("Created log filter with {} worker threads", num_threads);
        
        Self {
            include_pattern: include_arc,
            exclude_pattern: exclude_arc,
            thread_pool,
        }
    }
    
    /// Start the filtering process, consuming from input channel and sending to output channel
    pub fn start_filtering(&self, mut input_rx: mpsc::Receiver<LogEntry>) -> mpsc::Receiver<LogEntry> {
        let (output_tx, output_rx) = mpsc::channel(1024);
        
        // Clone needed data for the task
        let thread_pool = self.thread_pool.clone();
        let include_pattern = self.include_pattern.clone();
        let exclude_pattern = self.exclude_pattern.clone();
        
        // Spawn a task to process incoming log entries
        tokio::spawn(async move {
            let mut counter = 0;
            
            while let Some(entry) = input_rx.recv().await {
                counter += 1;
                if counter % 10000 == 0 {
                    debug!("Processed {} log entries", counter);
                }
                
                // Create a oneshot channel for this single entry
                let (tx, rx) = oneshot::channel();
                
                // Clone values needed for the thread
                let entry_clone = entry;
                let include_clone = include_pattern.clone();
                let exclude_clone = exclude_pattern.clone();
                
                // Execute the filtering in the thread pool
                thread_pool.execute(move || {
                    // Apply filtering logic with advanced patterns
                    let should_include = match &include_clone {
                        Some(pattern) => pattern.matches(&entry_clone.message),
                        _ => true, // If no include pattern, include all logs
                    };
                    
                    let should_exclude = match &exclude_clone {
                        Some(pattern) => pattern.matches(&entry_clone.message),
                        _ => false, // If no exclude pattern, exclude nothing
                    };
                    
                    // Only send back entries that pass the filters
                    if should_include && !should_exclude {
                        let _ = tx.send(Some(entry_clone));
                    } else {
                        let _ = tx.send(None);
                    }
                });
                
                // Await the result from the thread pool
                if let Ok(Some(filtered_entry)) = rx.await {
                    let _ = output_tx.send(filtered_entry).await;
                }
            }
            
            debug!("Input channel closed, filtering complete");
        });
        
        output_rx
    }

    /// Get the recommended number of threads for filtering based on CPU count
    pub fn recommended_threads() -> usize {
        // Use CPU count as a starting point for thread count
        let cpu_count = num_cpus::get();
        std::cmp::max(2, cpu_count)
    }
}