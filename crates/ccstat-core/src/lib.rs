//! Core types, traits, and utilities for ccstat
//!
//! This crate provides the foundational types, error handling,
//! timezone configuration, filters, and utility modules used
//! by all other ccstat crates.

pub mod aggregation_types;
pub mod error;
pub mod filters;
pub mod memory_pool;
pub mod model_formatter;
pub mod string_pool;
pub mod timezone;
pub mod types;

#[cfg(test)]
pub mod test_utils;

// Re-export commonly used types
pub use error::{CcstatError, Result};
pub use types::{CostMode, DailyDate, ISOTimestamp, ModelName, SessionId, TokenCounts};
