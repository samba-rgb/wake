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
    And(Box<FilterPattern>, Box<FilterPattern>),
    Or(Box<FilterPattern>, Box<FilterPattern>),
    Not(Box<FilterPattern>),
    Contains(String), // for exact text matching
}

impl FilterPattern {
    /// Parse a pattern string as a simple regex
    #[allow(dead_code)]
    pub fn parse(pattern: &str) -> Result<Self, String> {
        // Use a recursive descent parser for: &&, ||, !, (), "text", regex
        let tokens = FilterPattern::tokenize(pattern)?;
        let (pat, rest) = FilterPattern::parse_expr(&tokens)?;
        if !rest.is_empty() {
            return Err(format!("Unexpected tokens: {:?}", rest));
        }
        Ok(pat)
    }

    // Tokenize the pattern string
    fn tokenize(input: &str) -> Result<Vec<String>, String> {
        let mut tokens = Vec::new();
        let mut chars = input.chars().peekable();
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' | '\n' => { chars.next(); },
                '(' | ')' => { tokens.push(c.to_string()); chars.next(); },
                '&' => {
                    chars.next();
                    if chars.peek() == Some(&'&') { chars.next(); tokens.push("&&".to_string()); } else { return Err("Single '&' not allowed".to_string()); }
                },
                '|' => {
                    chars.next();
                    if chars.peek() == Some(&'|') { chars.next(); tokens.push("||".to_string()); } else { return Err("Single '|' not allowed".to_string()); }
                },
                '!' => { tokens.push("!".to_string()); chars.next(); },
                '"' => {
                    chars.next();
                    let mut s = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch == '"' { chars.next(); break; }
                        s.push(ch); chars.next();
                    }
                    tokens.push(format!("\"{}\"", s));
                },
                _ => {
                    // regex or word
                    let mut s = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_whitespace() || ch == '(' || ch == ')' || ch == '&' || ch == '|' || ch == '!' { break; }
                        s.push(ch); chars.next();
                    }
                    if !s.is_empty() { tokens.push(s); }
                }
            }
        }
        Ok(tokens)
    }

    // Recursive descent parser
    fn parse_expr(tokens: &[String]) -> Result<(Self, &[String]), String> {
        FilterPattern::parse_or(tokens)
    }
    fn parse_or(tokens: &[String]) -> Result<(Self, &[String]), String> {
        let (mut left, mut rest) = FilterPattern::parse_and(tokens)?;
        while rest.first().map(|s| s.as_str()) == Some("||") {
            rest = &rest[1..];
            let (right, r) = FilterPattern::parse_and(rest)?;
            left = FilterPattern::Or(Box::new(left), Box::new(right));
            rest = r;
        }
        Ok((left, rest))
    }
    fn parse_and(tokens: &[String]) -> Result<(Self, &[String]), String> {
        let (mut left, mut rest) = FilterPattern::parse_not(tokens)?;
        while rest.first().map(|s| s.as_str()) == Some("&&") {
            rest = &rest[1..];
            let (right, r) = FilterPattern::parse_not(rest)?;
            left = FilterPattern::And(Box::new(left), Box::new(right));
            rest = r;
        }
        Ok((left, rest))
    }
    fn parse_not(tokens: &[String]) -> Result<(Self, &[String]), String> {
        if tokens.first().map(|s| s.as_str()) == Some("!") {
            let (pat, rest) = FilterPattern::parse_not(&tokens[1..])?;
            Ok((FilterPattern::Not(Box::new(pat)), rest))
        } else {
            FilterPattern::parse_atom(tokens)
        }
    }
    fn parse_atom(tokens: &[String]) -> Result<(Self, &[String]), String> {
        match tokens.first() {
            Some(t) if t == "(" => {
                let (pat, rest) = FilterPattern::parse_expr(&tokens[1..])?;
                if rest.first().map(|s| s.as_str()) != Some(")") {
                    return Err("Expected ')'".to_string());
                }
                Ok((pat, &rest[1..]))
            },
            Some(t) if t.starts_with('"') && t.ends_with('"') => {
                let s = t.trim_matches('"').to_string();
                Ok((FilterPattern::Contains(s), &tokens[1..]))
            },
            Some(t) => {
                // treat as regex
                let re = Regex::new(t).map_err(|e| format!("Invalid regex '{}': {}", t, e))?;
                Ok((FilterPattern::Simple(re), &tokens[1..]))
            },
            None => Err("Unexpected end of pattern".to_string()),
        }
    }

    /// Check if a log message matches this pattern
    pub fn matches(&self, message: &str) -> bool {
        match self {
            FilterPattern::Simple(regex) => regex.is_match(message),
            FilterPattern::And(a, b) => a.matches(message) && b.matches(message),
            FilterPattern::Or(a, b) => a.matches(message) || b.matches(message),
            FilterPattern::Not(a) => !a.matches(message),
            FilterPattern::Contains(s) => message.contains(s),
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