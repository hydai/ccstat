//! Core domain types for ccusage
//!
//! This module contains the fundamental types used throughout the ccusage library.
//! These types provide strong typing for common concepts like model names, session IDs,
//! timestamps, and token counts.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign};

/// Strongly-typed model name wrapper
/// 
/// This ensures model names are consistently handled throughout the application
/// and provides type safety when working with model identifiers.
/// 
/// # Examples
/// ```
/// use ccusage::types::ModelName;
/// 
/// let model = ModelName::new("claude-3-opus");
/// assert_eq!(model.as_str(), "claude-3-opus");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelName(String);

impl ModelName {
    /// Create a new ModelName from any string-like type
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strongly-typed session ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new SessionId
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// ISO timestamp wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ISOTimestamp(DateTime<Utc>);

impl ISOTimestamp {
    /// Create a new ISOTimestamp
    pub fn new(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }

    /// Get the inner DateTime
    pub fn inner(&self) -> &DateTime<Utc> {
        &self.0
    }

    /// Convert to DailyDate
    pub fn to_daily_date(&self) -> DailyDate {
        DailyDate::new(self.0.date_naive())
    }
}

impl AsRef<DateTime<Utc>> for ISOTimestamp {
    fn as_ref(&self) -> &DateTime<Utc> {
        &self.0
    }
}

/// Daily date for aggregation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DailyDate(NaiveDate);

impl DailyDate {
    /// Create a new DailyDate
    pub fn new(date: NaiveDate) -> Self {
        Self(date)
    }

    /// Get the inner NaiveDate
    pub fn inner(&self) -> &NaiveDate {
        &self.0
    }

    /// Create from a timestamp
    pub fn from_timestamp(ts: &ISOTimestamp) -> Self {
        ts.to_daily_date()
    }

    /// Format as YYYY-MM-DD
    pub fn format(&self, fmt: &str) -> String {
        self.0.format(fmt).to_string()
    }
}

/// Token counts for usage tracking
/// 
/// This struct tracks all types of tokens consumed during Claude API usage,
/// including input, output, and cache-related tokens.
/// 
/// # Examples
/// ```
/// use ccusage::types::TokenCounts;
/// 
/// let tokens = TokenCounts::new(100, 50, 10, 5);
/// assert_eq!(tokens.total(), 165);
/// 
/// // TokenCounts supports arithmetic operations
/// let tokens2 = TokenCounts::new(50, 25, 5, 2);
/// let combined = tokens + tokens2;
/// assert_eq!(combined.input_tokens, 150);
/// ```
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenCounts {
    /// Input tokens used
    pub input_tokens: u64,
    /// Output tokens generated
    pub output_tokens: u64,
    /// Cache creation tokens
    pub cache_creation_tokens: u64,
    /// Cache read tokens
    pub cache_read_tokens: u64,
}

impl TokenCounts {
    /// Create new TokenCounts
    pub fn new(
        input_tokens: u64,
        output_tokens: u64,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
    ) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
        }
    }

    /// Calculate total tokens
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

impl Add for TokenCounts {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_creation_tokens: self.cache_creation_tokens + other.cache_creation_tokens,
            cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
        }
    }
}

impl AddAssign for TokenCounts {
    fn add_assign(&mut self, other: Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
    }
}

/// Cost calculation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostMode {
    /// Use pre-calculated costs when available
    Auto,
    /// Always calculate from tokens
    Calculate,
    /// Always use pre-calculated costs
    Display,
}

impl Default for CostMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl fmt::Display for CostMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Calculate => write!(f, "calculate"),
            Self::Display => write!(f, "display"),
        }
    }
}

impl std::str::FromStr for CostMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "calculate" => Ok(Self::Calculate),
            "display" => Ok(Self::Display),
            _ => Err(format!("Invalid cost mode: {s}")),
        }
    }
}

/// Model pricing information from LiteLLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Cost per input token
    pub input_cost_per_token: Option<f64>,
    /// Cost per output token
    pub output_cost_per_token: Option<f64>,
    /// Cost per cache creation token
    pub cache_creation_input_token_cost: Option<f64>,
    /// Cost per cache read token
    pub cache_read_input_token_cost: Option<f64>,
}

/// Usage entry from JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    /// Session identifier
    pub session_id: SessionId,
    /// Timestamp of the usage
    pub timestamp: ISOTimestamp,
    /// Model used
    pub model: ModelName,
    /// Token counts
    #[serde(flatten)]
    pub tokens: TokenCounts,
    /// Pre-calculated total cost (optional)
    pub total_cost: Option<f64>,
    /// Project name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Instance identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_model_name() {
        let model = ModelName::new("claude-3-opus");
        assert_eq!(model.as_str(), "claude-3-opus");
        assert_eq!(model.to_string(), "claude-3-opus");
    }

    #[test]
    fn test_session_id() {
        let session = SessionId::new("abc123");
        assert_eq!(session.as_str(), "abc123");
    }

    #[test]
    fn test_token_counts_arithmetic() {
        let tokens1 = TokenCounts::new(100, 50, 10, 5);
        let tokens2 = TokenCounts::new(200, 100, 20, 10);

        let sum = tokens1 + tokens2;
        assert_eq!(sum.input_tokens, 300);
        assert_eq!(sum.output_tokens, 150);
        assert_eq!(sum.cache_creation_tokens, 30);
        assert_eq!(sum.cache_read_tokens, 15);
        assert_eq!(sum.total(), 495);
    }

    #[test]
    fn test_cost_mode_parsing() {
        assert_eq!("auto".parse::<CostMode>().unwrap(), CostMode::Auto);
        assert_eq!(
            "calculate".parse::<CostMode>().unwrap(),
            CostMode::Calculate
        );
        assert_eq!("display".parse::<CostMode>().unwrap(), CostMode::Display);
        assert!("invalid".parse::<CostMode>().is_err());
    }

    #[test]
    fn test_daily_date() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let ts = ISOTimestamp::new(dt);
        let daily = ts.to_daily_date();

        assert_eq!(daily.format("%Y-%m-%d"), "2024-01-15");
    }
}
