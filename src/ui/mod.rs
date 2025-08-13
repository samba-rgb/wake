pub mod app;
pub mod input;
pub mod display;
pub mod filter_manager;
pub mod template_ui;

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