//! ccusage - Analyze Claude Code usage data from local JSONL files
//!
//! This library provides functionality to:
//! - Parse JSONL usage logs from multiple Claude data directories
//! - Calculate token costs using LiteLLM pricing data
//! - Generate reports in table and JSON formats
//! - Provide MCP server support for API access
//! - Support live monitoring mode for active sessions
//!
//! # Examples
//!
//! ```no_run
//! use ccusage::{
//!     data_loader::DataLoader,
//!     aggregation::Aggregator,
//!     cost_calculator::CostCalculator,
//!     pricing_fetcher::PricingFetcher,
//!     types::CostMode,
//! };
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> ccusage::Result<()> {
//!     // Initialize components
//!     let data_loader = DataLoader::new().await?;
//!     let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
//!     let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
//!     let aggregator = Aggregator::new(cost_calculator);
//!
//!     // Load and aggregate usage data
//!     let entries = data_loader.load_usage_entries();
//!     let daily_data = aggregator.aggregate_daily(entries, CostMode::Auto).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod aggregation;
pub mod cli;
pub mod cost_calculator;
pub mod data_loader;
pub mod error;
pub mod filters;
pub mod live_monitor;
pub mod mcp;
pub mod memory_pool;
pub mod output;
pub mod pricing_fetcher;
pub mod string_pool;
pub mod types;

// Re-export commonly used types
pub use error::{CcusageError, Result};
pub use types::{CostMode, DailyDate, ISOTimestamp, ModelName, SessionId, TokenCounts};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
