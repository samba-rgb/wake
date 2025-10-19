#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

// Re-export modules so they can be used from tests
pub mod cli;
pub mod k8s;
pub mod logging;
pub mod output;
pub mod filtering;
pub mod ui;
pub mod config;
pub mod kernel;
pub mod templates;
pub mod search;
pub mod guide;
pub mod common;
pub mod update_manager;