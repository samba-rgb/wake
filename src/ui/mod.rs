pub mod app;
pub mod input;
pub mod display;
pub mod filter_manager;
pub mod template_ui;
pub mod monitor;
pub mod monitor_ui;
pub mod config_ui;

use anyhow::Result;
use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use crate::templates::*;

use futures::Stream;
use std::pin::Pin;

pub async fn run_with_ui(
    log_stream: Pin<Box<dyn Stream<Item = LogEntry> + Send>>,
    args: Args,
) -> Result<()> {
    app::run_app(log_stream, args).await
}

pub async fn run_with_monitor_ui(args: Args) -> Result<()> {
    app::run_monitor_app(args).await
}

pub async fn run_with_config_ui() -> Result<()> {
    config_ui::run_config_ui().await
}