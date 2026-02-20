//! Core domain types for ccstat
//!
//! This module contains the fundamental types used throughout the ccstat library.
//! These types provide strong typing for common concepts like model names, session IDs,
//! timestamps, and token counts.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign};
use uuid::Uuid;

/// Strongly-typed model name wrapper
///
/// This ensures model names are consistently handled throughout the application
/// and provides type safety when working with model identifiers.
///
/// # Examples
/// ```
/// use ccstat_core::types::ModelName;
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

/// Strongly-typed session ID wrapper
///
/// Represents a unique identifier for a Claude session. Session IDs are used to group
/// related usage entries together for aggregation and billing purposes.
///
/// # Examples
/// ```
/// use ccstat_core::types::SessionId;
///
/// let session = SessionId::new("550e8400-e29b-41d4-a716-446655440000");
/// assert_eq!(session.as_str(), "550e8400-e29b-41d4-a716-446655440000");
/// ```
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

/// ISO timestamp wrapper for UTC timestamps
///
/// Provides a strongly-typed wrapper around chrono's `DateTime<Utc>` with
/// serialization support and convenient conversion methods.
///
/// # Examples
/// ```
/// use ccstat_core::types::ISOTimestamp;
/// use chrono::{TimeZone, Utc};
///
/// let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
/// let timestamp = ISOTimestamp::new(dt);
///
/// // Convert to daily date for aggregation
/// let daily = timestamp.to_daily_date();
/// assert_eq!(daily.format("%Y-%m-%d"), "2024-01-15");
/// ```
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

    /// Convert to DailyDate using UTC
    pub fn to_daily_date(&self) -> DailyDate {
        DailyDate::new(self.0.date_naive())
    }

    /// Convert to DailyDate using specified timezone
    pub fn to_daily_date_with_tz(&self, tz: &Tz) -> DailyDate {
        let local_dt = self.0.with_timezone(tz);
        DailyDate::new(local_dt.date_naive())
    }
}

impl AsRef<DateTime<Utc>> for ISOTimestamp {
    fn as_ref(&self) -> &DateTime<Utc> {
        &self.0
    }
}

/// Daily date for aggregation
///
/// Represents a calendar date without time information, used for daily
/// aggregation of usage data. This type ensures consistent date handling
/// across the application.
///
/// # Examples
/// ```
/// use ccstat_core::types::DailyDate;
/// use chrono::NaiveDate;
///
/// let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
/// let daily = DailyDate::new(date);
///
/// // Format for display
/// assert_eq!(daily.format("%Y-%m-%d"), "2024-01-15");
/// assert_eq!(daily.format("%B %d, %Y"), "January 15, 2024");
/// ```
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

    /// Create from a timestamp using UTC
    pub fn from_timestamp(ts: &ISOTimestamp) -> Self {
        ts.to_daily_date()
    }

    /// Create from a timestamp using specified timezone
    pub fn from_timestamp_with_tz(ts: &ISOTimestamp, tz: &Tz) -> Self {
        ts.to_daily_date_with_tz(tz)
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
/// use ccstat_core::types::TokenCounts;
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
///
/// Determines how costs are calculated when generating reports. This enum
/// provides flexibility in handling pre-calculated costs vs. dynamic calculation.
///
/// # Examples
/// ```
/// use ccstat_core::types::CostMode;
/// use std::str::FromStr;
///
/// // Parse from string
/// let mode = CostMode::from_str("auto").unwrap();
/// assert_eq!(mode, CostMode::Auto);
///
/// // Display mode formatting
/// assert_eq!(CostMode::Calculate.to_string(), "calculate");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostMode {
    /// Use pre-calculated costs when available, otherwise calculate
    ///
    /// This is the default mode that provides the best balance between
    /// accuracy and performance.
    #[default]
    Auto,
    /// Always calculate from tokens using current pricing data
    ///
    /// Use this mode when you need the most up-to-date cost calculations
    /// based on the latest pricing information.
    Calculate,
    /// Always use pre-calculated costs, error if not available
    ///
    /// Use this mode when you only want to display costs that were
    /// calculated at the time of usage.
    Display,
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
///
/// Contains pricing rates for different types of token usage. All costs are
/// in USD per token. Fields are optional to handle models that may not have
/// all pricing types available.
///
/// # Examples
/// ```
/// use ccstat_core::types::ModelPricing;
///
/// let pricing = ModelPricing {
///     input_cost_per_token: Some(0.00001),  // $0.01 per 1K tokens
///     output_cost_per_token: Some(0.00003), // $0.03 per 1K tokens
///     cache_creation_input_token_cost: Some(0.0000125),
///     cache_read_input_token_cost: Some(0.0000025),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPricing {
    /// Cost per input token in USD
    pub input_cost_per_token: Option<f64>,
    /// Cost per output token in USD
    pub output_cost_per_token: Option<f64>,
    /// Cost per cache creation token in USD
    pub cache_creation_input_token_cost: Option<f64>,
    /// Cost per cache read token in USD
    pub cache_read_input_token_cost: Option<f64>,
}

/// Raw message usage data from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageUsage {
    /// Input tokens used
    pub input_tokens: u64,
    /// Output tokens generated
    #[serde(default)]
    pub output_tokens: u64,
    /// Cache creation tokens
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    /// Cache read tokens
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

/// Raw message data from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Model used
    pub model: String,
    /// Usage data
    pub usage: MessageUsage,
    /// Message ID (used for deduplication)
    #[serde(default)]
    pub id: Option<String>,
    /// Message content (may contain error messages)
    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

/// Raw JSONL entry from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawJsonlEntry {
    /// Session ID
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
    /// Timestamp
    pub timestamp: String,
    /// Message containing model and usage
    pub message: Message,
    /// Entry type
    #[serde(rename = "type", default)]
    pub entry_type: Option<String>,
    /// Unique identifier for the event
    #[serde(default)]
    pub uuid: Option<String>,
    /// Current working directory when the event occurred
    #[serde(default)]
    pub cwd: Option<String>,
    /// References parent events for threaded conversations
    #[serde(rename = "parentUuid", default)]
    pub parent_uuid: Option<String>,
    /// Boolean indicating if this is a branch conversation
    #[serde(rename = "isSidechain", default)]
    pub is_sidechain: Option<bool>,
    /// Categorizes the user type (typically "external")
    #[serde(rename = "userType", default)]
    pub user_type: Option<String>,
    /// Claude Code version number
    #[serde(default)]
    pub version: Option<String>,
    /// Active git branch at event time
    #[serde(rename = "gitBranch", default)]
    pub git_branch: Option<String>,
    /// Pre-calculated cost in USD (snake_case for compatibility)
    #[serde(rename = "cost_usd", default)]
    pub cost_usd: Option<f64>,
    /// Pre-calculated cost in USD (camelCase as per ccusage spec)
    #[serde(rename = "costUSD", default)]
    pub cost_usd_camel: Option<f64>,
    /// Request ID (used for deduplication)
    #[serde(rename = "requestId", default)]
    pub request_id: Option<String>,
    /// Flag indicating if this is an API error message
    #[serde(rename = "isApiErrorMessage", default)]
    pub is_api_error_message: Option<bool>,
}

/// Usage entry from JSONL
///
/// Represents a single Claude API usage event parsed from JSONL log files.
/// This is the primary data structure for tracking usage across the application.
///
/// # Examples
/// ```
/// use ccstat_core::types::{UsageEntry, SessionId, ISOTimestamp, ModelName, TokenCounts};
/// use chrono::Utc;
///
/// let entry = UsageEntry {
///     session_id: SessionId::new("550e8400-e29b-41d4-a716-446655440000"),
///     timestamp: ISOTimestamp::new(Utc::now()),
///     model: ModelName::new("claude-3-opus"),
///     tokens: TokenCounts::new(1000, 500, 100, 50),
///     total_cost: Some(0.0255),
///     project: Some("my-project".to_string()),
///     instance_id: Some("instance-123".to_string()),
/// };
///
/// // Entries can be serialized to JSON
/// let json = serde_json::to_string(&entry).unwrap();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    /// Session identifier for grouping related API calls
    pub session_id: SessionId,
    /// Timestamp when the API call was made
    pub timestamp: ISOTimestamp,
    /// Model that was used for this API call
    pub model: ModelName,
    /// Token counts broken down by type
    #[serde(flatten)]
    pub tokens: TokenCounts,
    /// Pre-calculated total cost in USD (optional)
    ///
    /// This may be present in the usage logs if cost was calculated
    /// at the time of the API call.
    pub total_cost: Option<f64>,
    /// Project name extracted from working directory (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Instance identifier (UUID) for the API call (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

impl UsageEntry {
    /// Create from raw JSONL entry
    pub fn from_raw(raw: RawJsonlEntry) -> Option<Self> {
        // Skip API error messages
        if raw.is_api_error_message.unwrap_or(false) {
            tracing::debug!("Skipping API error message entry");
            return None;
        }

        // Only process entries of type "assistant" (if type is present)
        if let Some(entry_type) = &raw.entry_type
            && entry_type != "assistant"
        {
            return None;
        }

        // Skip synthetic models (errors, no response requested, etc.)
        if raw.message.model == "<synthetic>" {
            return None;
        }

        // Parse and validate timestamp
        let timestamp = match DateTime::parse_from_rfc3339(&raw.timestamp) {
            Ok(dt) => ISOTimestamp::new(dt.with_timezone(&Utc)),
            Err(_) => return None,
        };

        // Validate UUID format if present
        let instance_id = raw.uuid.as_ref().map(|uuid_str| {
            // Try to parse as UUID to validate format
            match Uuid::parse_str(uuid_str) {
                Ok(_) => uuid_str.clone(),
                Err(_) => {
                    // Log warning but don't fail - UUID might be in different format
                    tracing::debug!("Invalid UUID format: {}", uuid_str);
                    uuid_str.clone() // Still use it as instance ID
                }
            }
        });

        // Generate session ID - use provided session_id or fallback to a generated one
        let session_id = raw.session_id.unwrap_or_else(|| {
            // Generate a session ID from the timestamp and model if not provided
            format!(
                "generated-{}-{}",
                timestamp.inner().timestamp(),
                raw.message.model
            )
        });

        // Validate session ID format (should be UUID)
        if Uuid::parse_str(&session_id).is_err() {
            tracing::debug!("Session ID is not a valid UUID: {}", session_id);
            // Don't fail - continue processing with the session ID as-is
        }

        // Use either cost_usd or costUSD, preferring costUSD (ccusage format)
        let total_cost = raw.cost_usd_camel.or(raw.cost_usd);

        Some(Self {
            session_id: SessionId::new(session_id),
            timestamp,
            model: ModelName::new(raw.message.model),
            tokens: TokenCounts {
                input_tokens: raw.message.usage.input_tokens,
                output_tokens: raw.message.usage.output_tokens,
                cache_creation_tokens: raw.message.usage.cache_creation_input_tokens,
                cache_read_tokens: raw.message.usage.cache_read_input_tokens,
            },
            total_cost,
            project: raw.cwd.as_ref().and_then(|cwd| {
                // Extract project name from cwd path
                std::path::Path::new(cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            }),
            instance_id,
        })
    }

    /// Generate a deduplication key from message.id and requestId
    pub fn dedup_key(raw: &RawJsonlEntry) -> Option<String> {
        match (&raw.message.id, &raw.request_id) {
            (Some(msg_id), Some(req_id)) => Some(format!("{msg_id}-{req_id}")),
            (Some(msg_id), None) => Some(msg_id.clone()),
            (None, Some(req_id)) => Some(req_id.clone()),
            _ => None,
        }
    }
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

    #[test]
    fn test_dedup_key_generation() {
        // Test with both message.id and requestId
        let raw1 = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: Some("msg_123".to_string()),
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            request_id: Some("req_456".to_string()),
            cost_usd: None,
            cost_usd_camel: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        assert_eq!(
            UsageEntry::dedup_key(&raw1),
            Some("msg_123-req_456".to_string())
        );

        // Test with only message.id
        let mut raw2 = raw1.clone();
        raw2.request_id = None;
        assert_eq!(UsageEntry::dedup_key(&raw2), Some("msg_123".to_string()));

        // Test with only requestId
        let mut raw3 = raw1.clone();
        raw3.message.id = None;
        assert_eq!(UsageEntry::dedup_key(&raw3), Some("req_456".to_string()));

        // Test with neither
        let mut raw4 = raw1.clone();
        raw4.message.id = None;
        raw4.request_id = None;
        assert_eq!(UsageEntry::dedup_key(&raw4), None);
    }

    #[test]
    fn test_cost_usd_field_parsing() {
        // Test camelCase costUSD field
        let raw1 = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: None,
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            cost_usd: None,
            cost_usd_camel: Some(0.123),
            request_id: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        let entry = UsageEntry::from_raw(raw1).unwrap();
        assert_eq!(entry.total_cost, Some(0.123));

        // Test snake_case cost_usd field
        let raw2 = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: None,
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            cost_usd: Some(0.456),
            cost_usd_camel: None,
            request_id: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        let entry = UsageEntry::from_raw(raw2).unwrap();
        assert_eq!(entry.total_cost, Some(0.456));

        // Test preference for costUSD over cost_usd
        let raw3 = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: None,
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            cost_usd: Some(0.456),
            cost_usd_camel: Some(0.789),
            request_id: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        let entry = UsageEntry::from_raw(raw3).unwrap();
        assert_eq!(entry.total_cost, Some(0.789)); // Should prefer costUSD (camelCase)
    }

    #[test]
    fn test_skip_api_error_messages() {
        let raw = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: None,
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            is_api_error_message: Some(true),
            cost_usd: None,
            cost_usd_camel: None,
            request_id: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        assert!(UsageEntry::from_raw(raw).is_none());
    }

    #[test]
    fn test_edge_cases() {
        // Test with empty string session_id
        let raw = RawJsonlEntry {
            session_id: Some("".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: Some("msg_123".to_string()),
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            request_id: Some("req_456".to_string()),
            cost_usd: None,
            cost_usd_camel: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        // Should handle empty session_id
        let entry = UsageEntry::from_raw(raw).unwrap();
        assert_eq!(entry.session_id.as_ref(), "");
    }

    #[test]
    fn test_model_name_edge_cases() {
        // Test empty model name
        assert_eq!(ModelName::new("").as_str(), "");

        // Test very long model name
        let long_name = "a".repeat(1000);
        assert_eq!(ModelName::new(&long_name).as_str(), &long_name);

        // Test model name with special characters
        let special_name = "claude-3.5-sonnet@2024";
        assert_eq!(ModelName::new(special_name).as_str(), special_name);
    }

    #[test]
    fn test_token_counts_large_values() {
        // Test with very large but non-overflowing values
        let t1 = TokenCounts::new(1_000_000_000, 500_000_000, 100_000_000, 50_000_000);
        let t2 = TokenCounts::new(2_000_000_000, 1_000_000_000, 200_000_000, 100_000_000);

        let result = t1 + t2;

        assert_eq!(result.input_tokens, 3_000_000_000);
        assert_eq!(result.output_tokens, 1_500_000_000);
        assert_eq!(result.cache_creation_tokens, 300_000_000);
        assert_eq!(result.cache_read_tokens, 150_000_000);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    #[cfg(debug_assertions)]
    fn test_token_counts_overflow_debug() {
        // In debug mode, addition should panic on overflow
        let t1 = TokenCounts::new(u64::MAX - 100, 50, 0, 0);
        let t2 = TokenCounts::new(200, 50, 0, 0);

        // This should panic in debug mode
        let _ = t1 + t2;
    }

    #[test]
    fn test_invalid_timestamp_parsing() {
        let raw = RawJsonlEntry {
            session_id: Some("session123".to_string()),
            timestamp: "not-a-valid-timestamp".to_string(),
            message: Message {
                model: "claude-3-opus".to_string(),
                usage: MessageUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                id: Some("msg_123".to_string()),
                content: None,
            },
            entry_type: Some("assistant".to_string()),
            request_id: Some("req_456".to_string()),
            cost_usd: None,
            cost_usd_camel: None,
            is_api_error_message: None,
            uuid: None,
            cwd: None,
            parent_uuid: None,
            is_sidechain: None,
            user_type: None,
            version: None,
            git_branch: None,
        };

        // Should return None for invalid timestamp
        assert!(UsageEntry::from_raw(raw).is_none());
    }

    #[test]
    fn test_daily_date_edge_cases() {
        // Test dates at year boundaries
        let date1 = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        let dd1 = DailyDate::new(date1);
        assert_eq!(dd1.inner(), &date1);

        // Test leap year date
        let date2 = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let dd2 = DailyDate::new(date2);
        assert_eq!(dd2.inner(), &date2);
    }

    #[test]
    fn test_iso_timestamp_ordering() {
        let ts1 = ISOTimestamp::new(Utc::now());
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = ISOTimestamp::new(Utc::now());

        assert!(ts1 < ts2);
        assert!(ts2 > ts1);
        assert_eq!(ts1, ts1);
    }
}
