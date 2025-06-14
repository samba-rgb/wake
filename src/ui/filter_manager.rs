use regex::Regex;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use crate::k8s::logs::LogEntry;
use anyhow::Result;

/// Manages dynamic filtering with runtime pattern updates
#[derive(Clone)]
pub struct DynamicFilterManager {
    include_pattern: Arc<RwLock<Option<Arc<Regex>>>>,
    exclude_pattern: Arc<RwLock<Option<Arc<Regex>>>>,
    log_buffer: Arc<RwLock<Vec<LogEntry>>>,
    buffer_size: usize,
}

impl DynamicFilterManager {
    pub fn new(
        initial_include: Option<String>,
        initial_exclude: Option<String>,
        buffer_size: usize,
    ) -> Result<Self> {
        info!("FILTER_MANAGER: Creating new DynamicFilterManager with buffer_size: {}", buffer_size);
        info!("FILTER_MANAGER: Initial include pattern: {:?}", initial_include);
        info!("FILTER_MANAGER: Initial exclude pattern: {:?}", initial_exclude);
        
        let include_pattern = if let Some(pattern) = initial_include {
            info!("FILTER_MANAGER: Compiling include regex: {}", pattern);
            Some(Arc::new(Regex::new(&pattern)?))
        } else {
            None
        };
        
        let exclude_pattern = if let Some(pattern) = initial_exclude {
            info!("FILTER_MANAGER: Compiling exclude regex: {}", pattern);
            Some(Arc::new(Regex::new(&pattern)?))
        } else {
            None
        };

        info!("FILTER_MANAGER: DynamicFilterManager created successfully");
        Ok(Self {
            include_pattern: Arc::new(RwLock::new(include_pattern)),
            exclude_pattern: Arc::new(RwLock::new(exclude_pattern)),
            log_buffer: Arc::new(RwLock::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        })
    }

    /// Update the include pattern at runtime
    pub async fn update_include_pattern(&self, pattern: Option<String>) -> Result<()> {
        info!("FILTER_MANAGER: Updating include pattern to: {:?}", pattern);
        let new_pattern = if let Some(p) = pattern {
            if p.is_empty() {
                info!("FILTER_MANAGER: Include pattern is empty, clearing filter");
                None
            } else {
                info!("FILTER_MANAGER: Compiling new include regex: {}", p);
                Some(Arc::new(Regex::new(&p)?))
            }
        } else {
            info!("FILTER_MANAGER: Clearing include pattern");
            None
        };

        *self.include_pattern.write().await = new_pattern;
        info!("FILTER_MANAGER: Include pattern updated successfully");
        Ok(())
    }

    /// Update the exclude pattern at runtime
    pub async fn update_exclude_pattern(&self, pattern: Option<String>) -> Result<()> {
        info!("FILTER_MANAGER: Updating exclude pattern to: {:?}", pattern);
        let new_pattern = if let Some(p) = pattern {
            if p.is_empty() {
                info!("FILTER_MANAGER: Exclude pattern is empty, clearing filter");
                None
            } else {
                info!("FILTER_MANAGER: Compiling new exclude regex: {}", p);
                Some(Arc::new(Regex::new(&p)?))
            }
        } else {
            info!("FILTER_MANAGER: Clearing exclude pattern");
            None
        };

        *self.exclude_pattern.write().await = new_pattern;
        info!("FILTER_MANAGER: Exclude pattern updated successfully");
        Ok(())
    }

    /// Check if a log entry passes the current filters
    pub async fn should_include(&self, entry: &LogEntry) -> bool {
        let include_guard = self.include_pattern.read().await;
        let exclude_guard = self.exclude_pattern.read().await;

        // Check include pattern
        let passes_include = match include_guard.as_ref() {
            Some(pattern) => {
                let matches = pattern.is_match(&entry.message);
                debug!("FILTER_MANAGER: Include check for '{}': {}", 
                      entry.message.chars().take(30).collect::<String>(), matches);
                matches
            },
            None => {
                debug!("FILTER_MANAGER: No include pattern, allowing entry");
                true // No include pattern means include all
            }
        };

        // Check exclude pattern
        let passes_exclude = match exclude_guard.as_ref() {
            Some(pattern) => {
                let matches = pattern.is_match(&entry.message);
                let passes = !matches;
                debug!("FILTER_MANAGER: Exclude check for '{}': excluded={}, passes={}", 
                      entry.message.chars().take(30).collect::<String>(), matches, passes);
                passes
            },
            None => {
                debug!("FILTER_MANAGER: No exclude pattern, allowing entry");
                true // No exclude pattern means exclude nothing
            }
        };

        let final_result = passes_include && passes_exclude;
        if !final_result {
            debug!("FILTER_MANAGER: Entry filtered out - include: {}, exclude_pass: {}", 
                  passes_include, passes_exclude);
        }

        final_result
    }

    /// Add a log entry to the buffer (maintains circular buffer)
    pub async fn add_to_buffer(&self, entry: LogEntry) {
        // If buffer size is 0, don't store any entries (no retroactive filtering)
        if self.buffer_size == 0 {
            return;
        }
        
        let mut buffer = self.log_buffer.write().await;
        
        if buffer.len() >= self.buffer_size && !buffer.is_empty() {
            buffer.remove(0); // Remove oldest entry
        }
        
        buffer.push(entry);
    }

    /// Get filtered entries from the buffer (for retroactive filtering)
    pub async fn get_filtered_buffer(&self) -> Vec<LogEntry> {
        let buffer = self.log_buffer.read().await;
        let mut filtered = Vec::new();

        for entry in buffer.iter() {
            if self.should_include(entry).await {
                filtered.push(entry.clone());
            }
        }

        filtered
    }

    /// Get current filter patterns as strings for display
    pub async fn get_current_patterns(&self) -> (Option<String>, Option<String>) {
        let include_guard = self.include_pattern.read().await;
        let exclude_guard = self.exclude_pattern.read().await;

        let include_str = include_guard.as_ref().map(|r| r.as_str().to_string());
        let exclude_str = exclude_guard.as_ref().map(|r| r.as_str().to_string());

        (include_str, exclude_str)
    }

    /// Start the filtering process for a stream of log entries with cancellation support
    pub async fn start_filtering_with_cancellation(
        &self,
        mut input_rx: mpsc::Receiver<LogEntry>,
        cancellation_token: CancellationToken,
    ) -> mpsc::Receiver<LogEntry> {
        info!("FILTER_MANAGER: Starting filtering process with cancellation support");
        let (output_tx, output_rx) = mpsc::channel(1024);
        let manager = self.clone();

        tokio::spawn(async move {
            info!("FILTER_TASK: Filter processing task started");
            let mut filter_count = 0;
            let mut passed_count = 0;
            let mut filtered_count = 0;
            
            loop {
                tokio::select! {
                    // Check for cancellation
                    _ = cancellation_token.cancelled() => {
                        info!("FILTER_TASK: Received cancellation signal, shutting down gracefully");
                        break;
                    }
                    // Process log entries
                    entry = input_rx.recv() => {
                        match entry {
                            Some(entry) => {
                                filter_count += 1;
                                
                                if filter_count <= 10 || filter_count % 100 == 0 {
                                    info!("FILTER_TASK: Processing entry #{}: pod={}, message={}", 
                                          filter_count, entry.pod_name, 
                                          entry.message.chars().take(30).collect::<String>());
                                }
                                
                                // Add to buffer for retroactive filtering
                                manager.add_to_buffer(entry.clone()).await;

                                // Check if entry passes current filters
                                if manager.should_include(&entry).await {
                                    passed_count += 1;
                                    if passed_count <= 10 || passed_count % 100 == 0 {
                                        info!("FILTER_TASK: Entry #{} PASSED filter (total passed: {}), sending to display", 
                                              filter_count, passed_count);
                                    }
                                    if let Err(_) = output_tx.send(entry).await {
                                        warn!("FILTER_TASK: Output channel closed, stopping filter processing");
                                        break;
                                    }
                                } else {
                                    filtered_count += 1;
                                    if filtered_count <= 10 || filtered_count % 100 == 0 {
                                        info!("FILTER_TASK: Entry #{} FILTERED OUT (total filtered: {})", 
                                              filter_count, filtered_count);
                                    }
                                }
                            }
                            None => {
                                info!("FILTER_TASK: Input channel closed");
                                break;
                            }
                        }
                    }
                }
            }
            info!("FILTER_TASK: Filtering completed - {} entries processed, {} passed, {} filtered out", 
                  filter_count, passed_count, filtered_count);
        });

        info!("FILTER_MANAGER: Filtering process setup complete, returning output channel");
        output_rx
    }

    /// Start the filtering process for a stream of log entries (legacy method for compatibility)
    pub async fn start_filtering(
        &self,
        input_rx: mpsc::Receiver<LogEntry>,
    ) -> mpsc::Receiver<LogEntry> {
        // Create a cancellation token that never gets cancelled for backward compatibility
        let cancellation_token = CancellationToken::new();
        self.start_filtering_with_cancellation(input_rx, cancellation_token).await
    }
}