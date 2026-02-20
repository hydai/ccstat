//! Terminal output formatting for ccstat
//!
//! This crate provides table and JSON output formatters,
//! model name formatting, and billing block display.

pub mod blocks_monitor;
pub mod output;

pub use output::{JsonFormatter, OutputFormatter, TableFormatter, get_formatter};
