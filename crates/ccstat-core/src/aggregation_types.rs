//! Aggregation data types for ccstat
//!
//! Pure data structures used for aggregated usage summaries.
//! These types have no dependencies on cost_calculator or data_loader.

use crate::types::{DailyDate, ModelName, SessionId, TokenCounts};
use serde::{Deserialize, Serialize};

/// Daily usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    /// Date of usage
    pub date: DailyDate,
    /// Token counts for the day
    pub tokens: TokenCounts,
    /// Total cost for the day in USD
    pub total_cost: f64,
    /// List of unique models used during the day
    pub models_used: Vec<String>,
    /// Individual entries for verbose mode (only populated when verbose flag is set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<VerboseEntry>>,
}

/// Verbose entry for detailed token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerboseEntry {
    /// Timestamp of the entry
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Session ID
    pub session_id: String,
    /// Model used
    pub model: String,
    /// Token counts
    pub tokens: TokenCounts,
    /// Calculated cost for this entry
    pub cost: f64,
}

/// Daily usage grouped by instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyInstanceUsage {
    /// Date of usage
    pub date: DailyDate,
    /// Instance identifier (or "default" if none)
    pub instance_id: String,
    /// Token counts for the day
    pub tokens: TokenCounts,
    /// Total cost for the day
    pub total_cost: f64,
    /// Models used during the day
    pub models_used: Vec<String>,
}

/// Session usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsage {
    /// Session identifier
    pub session_id: SessionId,
    /// Start timestamp (earliest usage in session)
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End timestamp (latest usage in session)
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Token counts for the session
    pub tokens: TokenCounts,
    /// Total cost for the session
    pub total_cost: f64,
    /// Primary model used
    pub model: ModelName,
}

/// Monthly usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyUsage {
    /// Year and month in YYYY-MM format
    pub month: String,
    /// Total token counts for the month
    pub tokens: TokenCounts,
    /// Total cost for the month in USD
    pub total_cost: f64,
    /// Number of days with usage in this month
    pub active_days: usize,
}

/// 5-hour billing block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBlock {
    /// Block start time (floored to hour boundary)
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Block end time (5 hours after start)
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// First activity timestamp in this block (None for gap blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Last activity timestamp in this block (None for gap blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Sessions included in this block
    pub sessions: Vec<SessionUsage>,
    /// Total tokens used in this block
    pub tokens: TokenCounts,
    /// Total cost for this block in USD
    pub total_cost: f64,
    /// List of unique models used in this block
    pub models_used: Vec<String>,
    /// List of unique projects used in this block
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub projects_used: Vec<String>,
    /// Whether this block is currently active
    pub is_active: bool,
    /// Whether this is a gap block (period of inactivity)
    #[serde(default)]
    pub is_gap: bool,
    /// Optional warning message if approaching or exceeding token limits
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Calculate totals from aggregated data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Totals {
    pub tokens: TokenCounts,
    pub total_cost: f64,
}

impl Totals {
    pub fn from_daily(daily_usage: &[DailyUsage]) -> Self {
        let mut totals = Self::default();
        for daily in daily_usage {
            totals.tokens += daily.tokens;
            totals.total_cost += daily.total_cost;
        }
        totals
    }

    pub fn from_daily_instances(daily_instances: &[DailyInstanceUsage]) -> Self {
        let mut totals = Self::default();
        for daily in daily_instances {
            totals.tokens += daily.tokens;
            totals.total_cost += daily.total_cost;
        }
        totals
    }

    pub fn from_sessions(sessions: &[SessionUsage]) -> Self {
        let mut totals = Self::default();
        for session in sessions {
            totals.tokens += session.tokens;
            totals.total_cost += session.total_cost;
        }
        totals
    }

    pub fn from_monthly(monthly_usage: &[MonthlyUsage]) -> Self {
        let mut totals = Self::default();
        for monthly in monthly_usage {
            totals.tokens += monthly.tokens;
            totals.total_cost += monthly.total_cost;
        }
        totals
    }

    pub fn from_blocks(blocks: &[SessionBlock]) -> Self {
        let mut totals = Self::default();
        for block in blocks {
            totals.tokens += block.tokens;
            totals.total_cost += block.total_cost;
        }
        totals
    }
}
