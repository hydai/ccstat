//! ccstat - Analyze Claude Code usage data from local JSONL files
//!
//! This library provides functionality to:
//! - Parse JSONL usage logs from multiple Claude data directories
//! - Calculate token costs using LiteLLM pricing data
//! - Generate reports in table and JSON formats
//! - Support live monitoring mode for active sessions
//!
//! # Examples
//!
//! ```no_run
//! use ccstat::{
//!     data_loader::DataLoader,
//!     aggregation::Aggregator,
//!     cost_calculator::CostCalculator,
//!     pricing_fetcher::PricingFetcher,
//!     timezone::TimezoneConfig,
//!     types::CostMode,
//! };
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> ccstat::Result<()> {
//!     // Initialize components
//!     let data_loader = DataLoader::new().await?;
//!     let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
//!     let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
//!     let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
//!
//!     // Load and aggregate usage data
//!     let entries = data_loader.load_usage_entries_parallel();
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
pub mod memory_pool;
pub mod model_formatter;
pub mod output;
pub mod pricing_fetcher;
pub mod statusline;
pub mod string_pool;
pub mod timezone;
pub mod types;

// Test utilities module (only compiled for tests)
#[cfg(test)]
pub mod test_utils;

// Re-export commonly used types
pub use error::{CcstatError, Result};
pub use types::{CostMode, DailyDate, ISOTimestamp, ModelName, SessionId, TokenCounts};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        // VERSION is a compile-time constant, so we know it's not empty
        assert_ne!(VERSION, "");
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn test_all_modules_accessible() {
        // Test that all public modules are accessible
        use crate::{aggregation, cost_calculator, data_loader};

        // Just ensure they compile - no runtime test needed
        let _ = std::mem::size_of::<aggregation::Aggregator>();
        let _ = std::mem::size_of::<cost_calculator::CostCalculator>();
        let _ = std::mem::size_of::<data_loader::DataLoader>();

        // Verify module paths are valid
        let _ = crate::filters::UsageFilter::new();
        let _ = crate::output::TableFormatter::new(false);
        let _ = crate::output::JsonFormatter;
        let _ = crate::types::CostMode::Auto;
    }

    #[test]
    fn test_reexported_types() {
        // Test that re-exported types are accessible
        let _ = CostMode::Auto;
        let _ = std::mem::size_of::<DailyDate>();
        let _ = std::mem::size_of::<ISOTimestamp>();
        let _ = std::mem::size_of::<ModelName>();
        let _ = std::mem::size_of::<SessionId>();
        let _ = std::mem::size_of::<TokenCounts>();
    }

    #[test]
    fn test_error_type_accessible() {
        // Ensure error types are properly re-exported
        fn returns_result() -> Result<()> {
            Ok(())
        }

        assert!(returns_result().is_ok());
    }
}
