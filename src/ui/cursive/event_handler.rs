use anyhow::Result;
use cursive::CbSink;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{info, error};

use crate::k8s::logs::LogEntry;
use super::log_view::LogDisplayEntry;

#[derive(Debug, Clone)]
pub enum AppEvent {
    LogEntry(LogEntry),
    FilterChanged { include: String, exclude: String },
    StatusUpdate(String),
    Quit,
}

pub struct EventHandler {
    cursive_sink: Option<CbSink>,
    log_buffer: Arc<Mutex<Vec<LogDisplayEntry>>>,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            cursive_sink: None,
            log_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn set_cursive_sink(&mut self, sink: CbSink) {
        self.cursive_sink = Some(sink);
    }
    
    pub fn handle_log_entry_sync(&mut self, log_entry: LogEntry) -> Result<()> {
        // Convert LogEntry to LogDisplayEntry
        let display_entry = LogDisplayEntry {
            timestamp: log_entry.timestamp
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            pod: log_entry.pod_name.clone(),
            container: log_entry.container_name.clone(),
            message: log_entry.message.clone(),
            raw_entry: log_entry,
        };
        
        // Add to buffer
        {
            let mut buffer = self.log_buffer.lock().unwrap();
            buffer.push(display_entry);
            
            // Keep buffer size reasonable
            if buffer.len() > 10000 {
                buffer.remove(0);
            }
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    pub async fn handle_event(&mut self, event: AppEvent) -> Result<()> {
        match event {
            AppEvent::LogEntry(log_entry) => {
                self.handle_log_entry_sync(log_entry)?;
            },
            AppEvent::FilterChanged { include: _, exclude: _ } => {
                // Handle filter changes
                info!("Filter changed");
            },
            AppEvent::StatusUpdate(status) => {
                info!("Status update: {}", status);
            },
            AppEvent::Quit => {
                info!("Quit event received");
            },
        }
        Ok(())
    }
}