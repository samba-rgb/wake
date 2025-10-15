use anyhow::Result;
use async_trait::async_trait;
use std::io::{self, Write};
use std::fs::OpenOptions;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use crate::k8s::logs::LogEntry;
use crate::output::Formatter;
use crate::cli::Args;
use crate::config::Config;
use super::LogOutput;

impl std::fmt::Debug for TerminalOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalOutput")
         .field("formatter", &self.formatter)
         .field("primary_writer", &"Box<dyn Write + Send>")
         .field("autosave_writer", &self.autosave_writer.as_ref().map(|_| "Box<dyn Write + Send>"))
         .finish()
    }
}

pub struct TerminalOutput {
    formatter: Formatter,
    primary_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    autosave_writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
}

impl TerminalOutput {
    pub fn new(args: &Args) -> Result<Self> {
        let formatter = Formatter::new(args);
        let config = Config::load().unwrap_or_default();
        
        // Set up output writers
        let (primary_writer, autosave_writer) = setup_output_writers(args, &config)?;
        
        info!("🖥️  Terminal output handler initialized");
        if args.output_file.is_some() {
            info!("   File output: {:?}", args.output_file);
        }
        
        let primary_writer = Arc::new(Mutex::new(primary_writer));
        let autosave_writer = autosave_writer.map(|w| Arc::new(Mutex::new(w)));
        
        Ok(Self {
            formatter,
            primary_writer,
            autosave_writer,
        })
    }
}

#[async_trait]
impl LogOutput for TerminalOutput {
    async fn send_log(&mut self, entry: &LogEntry) -> Result<()> {
        if let Some(formatted) = self.formatter.format_without_filtering(entry) {
            // Write to primary output
            {
                let mut writer = self.primary_writer.lock().unwrap();
                if let Err(e) = writeln!(writer, "{formatted}") {
                    error!("Failed to write to primary output: {:?}", e);
                    if e.kind() == io::ErrorKind::BrokenPipe {
                        info!("Output pipe closed");
                        return Ok(());
                    }
                    return Err(anyhow::anyhow!("Failed to write to primary output: {:?}", e));
                }
                let _ = writer.flush();
            }
            
            // Write to autosave if configured
            if let Some(ref autosave_writer) = self.autosave_writer {
                let mut writer = autosave_writer.lock().unwrap();
                if let Err(e) = writeln!(writer, "{formatted}") {
                    error!("Failed to write to autosave output: {:?}", e);
                }
                let _ = writer.flush();
            }
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        {
            let mut writer = self.primary_writer.lock().unwrap();
            let _ = writer.flush();
        }
        if let Some(ref autosave_writer) = self.autosave_writer {
            let mut writer = autosave_writer.lock().unwrap();
            let _ = writer.flush();
        }
        Ok(())
    }

    fn output_type(&self) -> &'static str {
        "terminal"
    }
}

/// Set up output writers based on autosave config and CLI args
fn setup_output_writers(args: &Args, config: &Config) -> Result<(Box<dyn Write + Send>, Option<Box<dyn Write + Send>>)> {
    use std::path::Path;
    use crate::logging::determine_autosave_path;
    
    // Determine autosave file path
    let autosave_writer: Option<Box<dyn Write + Send>> = if config.autosave.enabled {
        let autosave_path = if args.output_file.is_some() {
            // If -w flag is provided, no separate autosave needed
            None
        } else {
            // No -w flag, use autosave configuration
            if let Some(ref configured_path) = config.autosave.path {
                let resolved_path = if Path::new(configured_path).is_dir() {
                    determine_autosave_path(configured_path)?
                } else {
                    configured_path.clone()
                };
                Some(resolved_path)
            } else {
                let autosave_path = determine_autosave_path(std::env::current_dir()?.to_str().unwrap())?;
                Some(autosave_path)
            }
        };

        if let Some(path) = autosave_path {
            info!("Autosave enabled: writing logs to {}", path);
            Some(Box::new(OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?))
        } else {
            None
        }
    } else {
        None
    };
    
    // Primary output writer
    let primary_writer: Box<dyn Write + Send> = if let Some(ref output_file) = args.output_file {
        info!("Primary output: writing to file {:?}", output_file);
        Box::new(std::fs::File::create(output_file)?)
    } else {
        info!("Primary output: console");
        Box::new(io::stdout())
    };
    
    Ok((primary_writer, autosave_writer))
}