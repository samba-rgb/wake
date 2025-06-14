use crate::k8s::logs::LogEntry;
use std::sync::Arc;
use regex::Regex;
use threadpool::ThreadPool;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

/// Advanced pattern that supports logical operators
#[derive(Debug, Clone)]
pub enum FilterPattern {
    Simple(Regex),
    And(Box<FilterPattern>, Box<FilterPattern>),
    Or(Box<FilterPattern>, Box<FilterPattern>),
    Not(Box<FilterPattern>),
    Contains(String),
}

impl FilterPattern {
    /// Parse a pattern string with logical operators
    /// Supports: (pattern1 && pattern2), (pattern1 || pattern2), !pattern, "text"
    pub fn parse(pattern: &str) -> Result<Self, String> {
        let trimmed = pattern.trim();
        
        // Handle parentheses
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            return Self::parse(&trimmed[1..trimmed.len()-1]);
        }
        
        // Handle NOT operator
        if trimmed.starts_with('!') {
            let inner = Self::parse(&trimmed[1..])?;
            return Ok(FilterPattern::Not(Box::new(inner)));
        }
        
        // Handle quoted strings (exact text match)
        if trimmed.starts_with('"') && trimmed.ends_with('"') {
            let text = trimmed[1..trimmed.len()-1].to_string();
            return Ok(FilterPattern::Contains(text));
        }
        
        // Look for logical operators (prioritize && over ||)
        if let Some(and_pos) = find_top_level_operator(trimmed, "&&") {
            let left = Self::parse(&trimmed[..and_pos])?;
            let right = Self::parse(&trimmed[and_pos + 2..])?;
            return Ok(FilterPattern::And(Box::new(left), Box::new(right)));
        }
        
        if let Some(or_pos) = find_top_level_operator(trimmed, "||") {
            let left = Self::parse(&trimmed[..or_pos])?;
            let right = Self::parse(&trimmed[or_pos + 2..])?;
            return Ok(FilterPattern::Or(Box::new(left), Box::new(right)));
        }
        
        // If no logical operators, treat as regex
        match Regex::new(trimmed) {
            Ok(regex) => Ok(FilterPattern::Simple(regex)),
            Err(e) => Err(format!("Invalid regex pattern '{}': {}", trimmed, e)),
        }
    }
    
    /// Check if a log message matches this pattern
    pub fn matches(&self, message: &str) -> bool {
        match self {
            FilterPattern::Simple(regex) => regex.is_match(message),
            FilterPattern::And(left, right) => left.matches(message) && right.matches(message),
            FilterPattern::Or(left, right) => left.matches(message) || right.matches(message),
            FilterPattern::Not(pattern) => !pattern.matches(message),
            FilterPattern::Contains(text) => message.contains(text),
        }
    }
}

/// Find the position of a top-level operator (not inside parentheses)
fn find_top_level_operator(s: &str, op: &str) -> Option<usize> {
    let mut paren_depth = 0;
    let mut quote_depth = 0;
    let chars: Vec<char> = s.chars().collect();
    
    for i in 0..chars.len() {
        match chars[i] {
            '(' if quote_depth == 0 => paren_depth += 1,
            ')' if quote_depth == 0 => paren_depth -= 1,
            '"' if paren_depth == 0 => quote_depth = (quote_depth + 1) % 2,
            _ => {}
        }
        
        if paren_depth == 0 && quote_depth == 0 {
            if s[i..].starts_with(op) {
                return Some(i);
            }
        }
    }
    None
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
    
    /// Creates a new log filter with advanced pattern support
    pub fn new_with_patterns(
        include_pattern: Option<String>,
        exclude_pattern: Option<String>,
        num_threads: usize,
    ) -> Result<Self, String> {
        let include_arc = if let Some(pattern) = include_pattern {
            Some(Arc::new(FilterPattern::parse(&pattern)?))
        } else {
            None
        };
        
        let exclude_arc = if let Some(pattern) = exclude_pattern {
            Some(Arc::new(FilterPattern::parse(&pattern)?))
        } else {
            None
        };

        let thread_pool = ThreadPool::new(num_threads);
        info!("Created advanced log filter with {} worker threads", num_threads);
        
        Ok(Self {
            include_pattern: include_arc,
            exclude_pattern: exclude_arc,
            thread_pool,
        })
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