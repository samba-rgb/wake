// Metrics module for Wake
pub mod collector;
pub mod timeseries;

// Re-export key types for convenience
pub use collector::{MetricsCollector, MetricsTimeSeries, MetricsSummary};