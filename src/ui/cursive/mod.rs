//! Cursive-based UI implementation for Wake
//! Replaces the ratatui implementation with native copy-paste and scrolling

pub mod app;
pub mod event_handler;
pub mod filter_panel;
pub mod log_view;
pub mod status_bar;
pub mod theme;

use anyhow::Result;
use futures::Stream;
use std::pin::Pin;

use crate::cli::Args;
use crate::k8s::logs::LogEntry;
use crate::ui::cursive::app::WakeApp;

/// Main entry point for the Cursive UI
pub async fn run_with_cursive_ui(
    log_stream: Pin<Box<dyn Stream<Item = LogEntry> + Send>>,
    args: Args,
) -> Result<()> {
    let mut app = WakeApp::new(args)?;
    app.run_with_stream(log_stream).await
}