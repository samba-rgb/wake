use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use crate::k8s::logs::LogEntry;
use crate::cli::Args;

mod terminal;
mod web;

pub use terminal::TerminalOutput;
pub use web::WebOutput;

/// Trait defining the output interface
#[async_trait]
pub trait LogOutput: Send + Sync {
    async fn send_log(&mut self, entry: &LogEntry) -> Result<()>;
    async fn flush(&mut self) -> Result<()>;
    fn output_type(&self) -> &'static str;
}

/// Factory for creating output handlers
pub struct OutputFactory;

impl OutputFactory {
    /// Creates the appropriate output handler based on arguments
    pub async fn create_output(args: &Args) -> Result<Box<dyn LogOutput>> {
        if args.web {
            // Web output mode - get configuration from config file
            let config = crate::config::Config::load().unwrap_or_default();
            let base_url = config.get_value("web.endpoint")
                .unwrap_or_else(|_| "http://localhost:5080".to_string());
            
            // Ensure base_url doesn't have trailing slash or path components
            let clean_base_url = if base_url.contains("/api/") {
                // Extract just the base URL (e.g., "http://localhost:5080")
                if let Some(pos) = base_url.find("/api/") {
                    base_url[..pos].to_string()
                } else {
                    base_url
                }
            } else {
                base_url
            };
            
            // Create dynamic stream name with today's date
            let today = chrono::Local::now().format("%Y_%m_%d").to_string();
            let stream_name = format!("logs_wake_{today}");
            let full_endpoint = format!("{clean_base_url}/api/default/{stream_name}/_json");
            
            let batch_size = config.get_value("web.batch_size")
                .unwrap_or_else(|_| "10".to_string())
                .parse::<usize>()
                .unwrap_or(10);
            let timeout_seconds = config.get_value("web.timeout_seconds")
                .unwrap_or_else(|_| "30".to_string())
                .parse::<u64>()
                .unwrap_or(30);

            Ok(Box::new(WebOutput::new(
                full_endpoint,
                batch_size,
                timeout_seconds,
            )?))
        } else {
            // Terminal output mode
            Ok(Box::new(TerminalOutput::new(args)?))
        }
    }
}

/// Decision maker that routes logs to the appropriate output
pub struct LogDecisionMaker {
    output_handler: Box<dyn LogOutput>,
}

impl LogDecisionMaker {
    pub async fn new(args: &Args) -> Result<Self> {
        let output_handler = OutputFactory::create_output(args).await?;
        Ok(Self { output_handler })
    }

    pub async fn process_log(&mut self, entry: LogEntry) -> Result<()> {
        self.output_handler.send_log(&entry).await
    }


    pub async fn flush(&mut self) -> Result<()> {
        self.output_handler.flush().await
    }

    pub fn get_output_type(&self) -> &'static str {
        self.output_handler.output_type()
    }
}