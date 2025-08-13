//! Aggregation module for summarizing usage data
//!
//! This module provides functionality to aggregate raw usage entries into
//! meaningful summaries like daily usage, monthly rollups, session statistics,
//! and billing blocks.
//!
//! # Cloning Strategy
//!
//! This module follows a deliberate cloning strategy to balance performance and simplicity:
//!
//! - **Entry References**: Methods take `&UsageEntry` to avoid unnecessary moves of large structs.
//! - **Model Names**: We clone `ModelName` strings when inserting into HashSets/Maps because:
//!   - Model names are typically small strings (e.g., "claude-3-opus")
//!   - There are only a few dozen unique model names at most
//!   - The alternative (Arc or string interning) adds complexity for minimal benefit
//! - **Stream Processing**: When processing streams, we clone entries individually rather than
//!   cloning entire collections, which reduces peak memory usage for large datasets.
//!
//! Future optimization opportunities (if profiling shows bottlenecks):
//! - Use the existing string interning infrastructure in `string_pool.rs` for model names
//! - Switch to `Arc<ModelName>` for shared ownership without cloning
//! - Implement zero-copy aggregation using lifetimes (complex but most efficient)
//!
//! # Examples
//!
//! ```no_run
//! use ccstat::{
//!     aggregation::Aggregator,
//!     cost_calculator::CostCalculator,
//!     data_loader::DataLoader,
//!     pricing_fetcher::PricingFetcher,
//!     timezone::TimezoneConfig,
//!     types::CostMode,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> ccstat::Result<()> {
//! let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
//! let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
//! let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
//!
//! let data_loader = DataLoader::new().await?;
//! let entries = data_loader.load_usage_entries_parallel();
//!
//! // Aggregate by day
//! let daily_data = aggregator.aggregate_daily(entries, CostMode::Auto).await?;
//!
//! // Create monthly rollups
//! let monthly_data = Aggregator::aggregate_monthly(&daily_data);
//! # Ok(())
//! # }
//! ```

use crate::cost_calculator::CostCalculator;
use crate::error::Result;
use crate::timezone::TimezoneConfig;
use crate::types::{CostMode, DailyDate, ModelName, SessionId, TokenCounts, UsageEntry};
use futures::stream::{Stream, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Daily usage summary
///
/// Aggregates all usage entries for a single day, providing token counts,
/// costs, and model usage information.
///
/// # Examples
/// ```
/// use ccstat::aggregation::DailyUsage;
/// use ccstat::types::{DailyDate, TokenCounts};
/// use chrono::NaiveDate;
///
/// let daily = DailyUsage {
///     date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()),
///     tokens: TokenCounts::new(10000, 5000, 1000, 500),
///     total_cost: 0.255,
///     models_used: vec!["claude-3-opus".to_string(), "claude-3-sonnet".to_string()],
///     entries: None,
/// };
/// ```
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
///
/// Aggregates all usage entries for a single session, tracking duration,
/// token usage, and costs. Sessions are identified by their UUID.
///
/// # Examples
/// ```
/// use ccstat::aggregation::SessionUsage;
/// use ccstat::types::{SessionId, TokenCounts, ModelName};
/// use chrono::Utc;
///
/// let session = SessionUsage {
///     session_id: SessionId::new("550e8400-e29b-41d4-a716-446655440000"),
///     start_time: Utc::now() - chrono::Duration::hours(2),
///     end_time: Utc::now(),
///     tokens: TokenCounts::new(5000, 2500, 500, 250),
///     total_cost: 0.1275,
///     model: ModelName::new("claude-3-opus"),
/// };
///
/// // Calculate session duration
/// let duration = session.end_time - session.start_time;
/// ```
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
///
/// Aggregates daily usage data into monthly summaries for billing and
/// trend analysis purposes.
///
/// # Examples
/// ```
/// use ccstat::aggregation::MonthlyUsage;
/// use ccstat::types::TokenCounts;
///
/// let monthly = MonthlyUsage {
///     month: "2024-01".to_string(),
///     tokens: TokenCounts::new(500000, 250000, 50000, 25000),
///     total_cost: 12.75,
///     active_days: 20,
/// };
///
/// // Average daily cost
/// let avg_daily_cost = monthly.total_cost / monthly.active_days as f64;
/// ```
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
///
/// Groups sessions into 5-hour windows based on Claude's billing model.
/// This helps track usage within billing periods and identify when approaching
/// token limits.
///
/// # Examples
/// ```
/// use ccstat::aggregation::{SessionBlock, SessionUsage};
/// use ccstat::types::{SessionId, TokenCounts, ModelName};
/// use chrono::Utc;
///
/// let block = SessionBlock {
///     start_time: Utc::now() - chrono::Duration::hours(3),
///     end_time: Utc::now() + chrono::Duration::hours(2),
///     sessions: vec![],
///     tokens: TokenCounts::new(8_000_000, 4_000_000, 0, 0),
///     total_cost: 240.0,
///     is_active: true,
///     warning: Some("⚠️  Block has used 12,000,000 tokens, exceeding threshold of 10,000,000 tokens".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBlock {
    /// Block start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Block end time (5 hours after start)
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Sessions included in this block
    pub sessions: Vec<SessionUsage>,
    /// Total tokens used in this block
    pub tokens: TokenCounts,
    /// Total cost for this block in USD
    pub total_cost: f64,
    /// Whether this block is currently active (contains recent usage)
    pub is_active: bool,
    /// Optional warning message if approaching or exceeding token limits
    pub warning: Option<String>,
}

/// Accumulator for daily aggregation
struct DailyAccumulator {
    tokens: TokenCounts,
    cost: f64,
    models: HashSet<ModelName>,
    verbose_entries: Option<Vec<VerboseEntry>>,
}

impl DailyAccumulator {
    fn new(verbose: bool) -> Self {
        Self {
            tokens: TokenCounts::default(),
            cost: 0.0,
            models: HashSet::new(),
            verbose_entries: if verbose { Some(Vec::new()) } else { None },
        }
    }

    fn add_entry(&mut self, entry: &UsageEntry, calculated_cost: f64) {
        self.tokens += entry.tokens;
        self.cost += calculated_cost;
        self.models.insert(entry.model.clone());

        if let Some(ref mut entries) = self.verbose_entries {
            entries.push(VerboseEntry {
                timestamp: *entry.timestamp.inner(),
                session_id: entry.session_id.to_string(),
                model: entry.model.to_string(),
                tokens: entry.tokens,
                cost: calculated_cost,
            });
        }
    }

    fn into_daily_usage(self, date: DailyDate) -> DailyUsage {
        DailyUsage {
            date,
            tokens: self.tokens,
            total_cost: self.cost,
            models_used: self.models.into_iter().map(|m| m.to_string()).collect(),
            entries: self.verbose_entries,
        }
    }
}

/// Accumulator for session aggregation
struct SessionAccumulator {
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
    tokens: TokenCounts,
    cost: f64,
    primary_model: Option<ModelName>,
}

impl SessionAccumulator {
    fn new() -> Self {
        Self {
            start_time: None,
            end_time: None,
            tokens: TokenCounts::default(),
            cost: 0.0,
            primary_model: None,
        }
    }

    fn add_entry(&mut self, entry: &UsageEntry, calculated_cost: f64) {
        let timestamp = entry.timestamp.inner();

        // Update time bounds
        if self.start_time.is_none() || timestamp < &self.start_time.unwrap() {
            self.start_time = Some(*timestamp);
        }
        if self.end_time.is_none() || timestamp > &self.end_time.unwrap() {
            self.end_time = Some(*timestamp);
        }

        self.tokens += entry.tokens;
        self.cost += calculated_cost;

        if self.primary_model.is_none() {
            self.primary_model = Some(entry.model.clone());
        }
    }

    fn into_session_usage(self, session_id: SessionId) -> SessionUsage {
        SessionUsage {
            session_id,
            start_time: self.start_time.unwrap_or_default(),
            end_time: self.end_time.unwrap_or_default(),
            tokens: self.tokens,
            total_cost: self.cost,
            model: self
                .primary_model
                .unwrap_or_else(|| ModelName::new("unknown")),
        }
    }
}

/// Main aggregation engine
pub struct Aggregator {
    cost_calculator: Arc<CostCalculator>,
    show_progress: bool,
    timezone_config: TimezoneConfig,
}

impl Aggregator {
    /// Create a new Aggregator
    pub fn new(cost_calculator: Arc<CostCalculator>, timezone_config: TimezoneConfig) -> Self {
        Self {
            cost_calculator,
            show_progress: false,
            timezone_config,
        }
    }

    /// Enable or disable progress bars
    pub fn with_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
    }

    /// Get the timezone configuration
    pub fn timezone_config(&self) -> &TimezoneConfig {
        &self.timezone_config
    }

    /// Aggregate entries by day and instance
    pub async fn aggregate_daily_by_instance(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
    ) -> Result<Vec<DailyInstanceUsage>> {
        let mut daily_map: BTreeMap<(DailyDate, String), DailyAccumulator> = BTreeMap::new();

        // Create progress spinner if enabled
        let progress = if self.show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg} [{elapsed_precise}] {pos} entries processed")
                    .unwrap(),
            );
            pb.set_message("Aggregating daily usage by instance");
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        let mut count = 0u64;

        tokio::pin!(entries);
        while let Some(result) = entries.next().await {
            let entry = result?;
            let date =
                DailyDate::from_timestamp_with_tz(&entry.timestamp, &self.timezone_config.tz);
            let instance_id = entry
                .instance_id
                .clone()
                .unwrap_or_else(|| "default".to_string());

            // Calculate cost
            let cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            daily_map
                .entry((date, instance_id.clone()))
                .or_insert_with(|| DailyAccumulator::new(false))
                .add_entry(&entry, cost);

            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message(format!("Aggregated {count} entries"));
        }

        Ok(daily_map
            .into_iter()
            .map(|((date, instance_id), acc)| DailyInstanceUsage {
                date,
                instance_id,
                tokens: acc.tokens,
                total_cost: acc.cost,
                models_used: acc.models.into_iter().map(|m| m.to_string()).collect(),
            })
            .collect())
    }

    /// Aggregate entries by day
    pub async fn aggregate_daily(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
    ) -> Result<Vec<DailyUsage>> {
        self.aggregate_daily_verbose(entries, cost_mode, false)
            .await
    }

    /// Aggregate entries by day with optional verbose mode
    pub async fn aggregate_daily_verbose(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
        verbose: bool,
    ) -> Result<Vec<DailyUsage>> {
        let mut daily_map: BTreeMap<DailyDate, DailyAccumulator> = BTreeMap::new();

        // Create progress spinner if enabled
        let progress = if self.show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg} [{elapsed_precise}] {pos} entries processed")
                    .unwrap(),
            );
            pb.set_message("Aggregating daily usage");
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        let mut count = 0u64;

        tokio::pin!(entries);
        while let Some(result) = entries.next().await {
            let entry = result?;
            let date =
                DailyDate::from_timestamp_with_tz(&entry.timestamp, &self.timezone_config.tz);

            // Calculate cost
            let cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            daily_map
                .entry(date)
                .or_insert_with(|| DailyAccumulator::new(verbose))
                .add_entry(&entry, cost);

            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message(format!(
                "Aggregated {} entries into {} days",
                count,
                daily_map.len()
            ));
        }

        Ok(daily_map
            .into_iter()
            .map(|(date, acc)| acc.into_daily_usage(date))
            .collect())
    }

    /// Aggregate entries by session
    pub async fn aggregate_sessions(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
    ) -> Result<Vec<SessionUsage>> {
        let mut session_map: BTreeMap<SessionId, SessionAccumulator> = BTreeMap::new();

        // Create progress spinner if enabled
        let progress = if self.show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg} [{elapsed_precise}] {pos} entries processed")
                    .unwrap(),
            );
            pb.set_message("Aggregating session usage");
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        let mut count = 0u64;

        tokio::pin!(entries);
        while let Some(result) = entries.next().await {
            let entry = result?;
            let session_id = entry.session_id.clone();

            // Calculate cost
            let cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            session_map
                .entry(session_id)
                .or_insert_with(SessionAccumulator::new)
                .add_entry(&entry, cost);

            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message(format!(
                "Aggregated {} entries into {} sessions",
                count,
                session_map.len()
            ));
        }

        let mut sessions: Vec<_> = session_map
            .into_iter()
            .map(|(id, acc)| acc.into_session_usage(id))
            .collect();

        // Sort by start time
        sessions.sort_by_key(|s| s.start_time);

        Ok(sessions)
    }

    /// Aggregate daily usage into monthly summaries
    pub fn aggregate_monthly(daily_usage: &[DailyUsage]) -> Vec<MonthlyUsage> {
        let mut monthly_map: BTreeMap<String, (TokenCounts, f64, usize)> = BTreeMap::new();

        for daily in daily_usage {
            let month = daily.date.format("%Y-%m");
            let entry = monthly_map
                .entry(month)
                .or_insert((TokenCounts::default(), 0.0, 0));

            entry.0 += daily.tokens;
            entry.1 += daily.total_cost;
            entry.2 += 1;
        }

        monthly_map
            .into_iter()
            .map(|(month, (tokens, cost, days))| MonthlyUsage {
                month,
                tokens,
                total_cost: cost,
                active_days: days,
            })
            .collect()
    }

    /// Truncate a timestamp to the hour boundary (XX:00:00)
    fn truncate_to_hour(timestamp: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
        use chrono::Timelike;
        timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .expect("truncating to hour should always be valid")
    }

    /// Group sessions into 5-hour billing blocks
    pub fn create_billing_blocks(sessions: &[SessionUsage]) -> Vec<SessionBlock> {
        if sessions.is_empty() {
            return Vec::new();
        }

        let mut blocks = Vec::new();
        let mut current_block_start: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut current_sessions = Vec::new();
        let mut current_tokens = TokenCounts::default();
        let mut current_cost = 0.0;

        let now = chrono::Utc::now();
        let five_hours = chrono::Duration::hours(5);

        for session in sessions {
            // Check if we need to start a new block
            if let Some(block_start) = current_block_start
                && session.start_time >= block_start + five_hours
            {
                // Finish current block
                blocks.push(SessionBlock {
                    start_time: block_start,
                    end_time: block_start + five_hours,
                    sessions: std::mem::take(&mut current_sessions),
                    tokens: std::mem::take(&mut current_tokens),
                    total_cost: std::mem::take(&mut current_cost),
                    is_active: false,
                    warning: None,
                });
                current_block_start = None;
            }

            // Start new block if needed
            if current_block_start.is_none() {
                // Align block start to hour boundary (XX:00)
                current_block_start = Some(Self::truncate_to_hour(session.start_time));
            }

            // Add session to current block
            current_sessions.push(session.clone());
            current_tokens += session.tokens;
            current_cost += session.total_cost;
        }

        // Handle remaining sessions
        if let Some(block_start) = current_block_start {
            let is_active = now < block_start + five_hours;
            blocks.push(SessionBlock {
                start_time: block_start,
                end_time: block_start + five_hours,
                sessions: current_sessions,
                tokens: current_tokens,
                total_cost: current_cost,
                is_active,
                warning: None,
            });
        }

        blocks
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_daily_accumulator() {
        let mut acc = DailyAccumulator::new(false);

        let entry = UsageEntry {
            session_id: SessionId::new("test"),
            timestamp: crate::types::ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 10, 5),
            total_cost: Some(0.01),
            project: None,
            instance_id: None,
        };

        acc.add_entry(&entry, 0.01);
        assert_eq!(acc.tokens.input_tokens, 100);
        assert_eq!(acc.cost, 0.01);
        assert_eq!(acc.models.len(), 1);
    }

    #[test]
    fn test_daily_accumulator_verbose() {
        let mut acc = DailyAccumulator::new(true);

        let entry = UsageEntry {
            session_id: SessionId::new("test"),
            timestamp: crate::types::ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 10, 5),
            total_cost: Some(0.01),
            project: None,
            instance_id: None,
        };

        acc.add_entry(&entry, 0.01);
        assert_eq!(acc.tokens.input_tokens, 100);
        assert_eq!(acc.cost, 0.01);
        assert_eq!(acc.models.len(), 1);
        assert!(acc.verbose_entries.is_some());
        assert_eq!(acc.verbose_entries.unwrap().len(), 1);
    }

    #[test]
    fn test_billing_blocks() {
        let base_time = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let sessions = vec![
            SessionUsage {
                session_id: SessionId::new("s1"),
                start_time: base_time,
                end_time: base_time + chrono::Duration::hours(1),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: 0.01,
                model: ModelName::new("claude-3-opus"),
            },
            SessionUsage {
                session_id: SessionId::new("s2"),
                start_time: base_time + chrono::Duration::hours(3),
                end_time: base_time + chrono::Duration::hours(4),
                tokens: TokenCounts::new(200, 100, 0, 0),
                total_cost: 0.02,
                model: ModelName::new("claude-3-opus"),
            },
            SessionUsage {
                session_id: SessionId::new("s3"),
                start_time: base_time + chrono::Duration::hours(6),
                end_time: base_time + chrono::Duration::hours(7),
                tokens: TokenCounts::new(150, 75, 0, 0),
                total_cost: 0.015,
                model: ModelName::new("claude-3-opus"),
            },
        ];

        let blocks = Aggregator::create_billing_blocks(&sessions);
        assert_eq!(blocks.len(), 2);

        // First block should contain s1 and s2
        assert_eq!(blocks[0].sessions.len(), 2);
        assert_eq!(blocks[0].tokens.input_tokens, 300);

        // Second block should contain s3
        assert_eq!(blocks[1].sessions.len(), 1);
        assert_eq!(blocks[1].tokens.input_tokens, 150);
    }

    #[test]
    fn test_billing_blocks_hour_alignment() {
        // Test that blocks are aligned to hour boundaries
        let base_time = chrono::Utc
            .with_ymd_and_hms(2024, 1, 1, 19, 23, 45)
            .unwrap(); // 19:23:45

        let sessions = vec![
            SessionUsage {
                session_id: SessionId::new("s1"),
                start_time: base_time, // Starts at 19:23:45
                end_time: base_time + chrono::Duration::hours(1),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: 0.01,
                model: ModelName::new("claude-3-opus"),
            },
            SessionUsage {
                session_id: SessionId::new("s2"),
                start_time: base_time + chrono::Duration::minutes(90), // 20:53:45
                end_time: base_time + chrono::Duration::hours(2),
                tokens: TokenCounts::new(200, 100, 0, 0),
                total_cost: 0.02,
                model: ModelName::new("claude-3-opus"),
            },
            SessionUsage {
                session_id: SessionId::new("s3"),
                start_time: base_time + chrono::Duration::hours(5) + chrono::Duration::minutes(10), // 00:33:45 next day
                end_time: base_time + chrono::Duration::hours(6),
                tokens: TokenCounts::new(150, 75, 0, 0),
                total_cost: 0.015,
                model: ModelName::new("claude-3-opus"),
            },
        ];

        let blocks = Aggregator::create_billing_blocks(&sessions);
        assert_eq!(blocks.len(), 2);

        // First block should start at 19:00:00 (aligned to hour)
        let expected_block1_start = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 19, 0, 0).unwrap();
        assert_eq!(blocks[0].start_time, expected_block1_start);
        assert_eq!(
            blocks[0].end_time,
            expected_block1_start + chrono::Duration::hours(5)
        );
        assert_eq!(blocks[0].sessions.len(), 2); // s1 and s2

        // Second block should start at 00:00:00 (aligned to hour)
        let expected_block2_start = chrono::Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        assert_eq!(blocks[1].start_time, expected_block2_start);
        assert_eq!(
            blocks[1].end_time,
            expected_block2_start + chrono::Duration::hours(5)
        );
        assert_eq!(blocks[1].sessions.len(), 1); // s3
    }
}
