//! ccusage - Analyze Claude Code usage data from local JSONL files
//!
//! This library provides functionality to:
//! - Parse JSONL usage logs from multiple Claude data directories
//! - Calculate token costs using LiteLLM pricing data
//! - Generate reports in table and JSON formats
//! - Provide MCP server support for API access
//! - Support live monitoring mode for active sessions

pub mod aggregation;
pub mod cli;
pub mod cost_calculator;
pub mod data_loader;
pub mod error;
pub mod mcp;
pub mod output;
pub mod pricing_fetcher;
pub mod types;

// Re-export commonly used types
pub use error::{CcusageError, Result};
pub use types::{CostMode, DailyDate, ISOTimestamp, ModelName, SessionId, TokenCounts};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
