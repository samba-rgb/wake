//! Scripts module for managing and executing user-defined scripts
//! 
//! This module provides:
//! - Script storage and retrieval
//! - Script editor TUI with template and validation
//! - Script list TUI for viewing/editing saved scripts
//! - Script execution with template-like UI
//! - Script selection with autocomplete
//! - Output merging capabilities

pub mod manager;
pub mod editor_ui;
pub mod executor_ui;
pub mod selector_ui;
pub mod list_ui;

pub use manager::{Script, ScriptArg, ScriptManager};
pub use editor_ui::run_script_editor;
pub use executor_ui::run_script_executor;
pub use selector_ui::{run_script_selector, ScriptSelection};
pub use list_ui::{run_script_list_ui, ListAction};
