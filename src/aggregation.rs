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
use crate::filters::MonthFilter;
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
/// Groups usage entries into 5-hour windows based on Claude's billing model.
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
///     actual_start_time: Some(Utc::now() - chrono::Duration::hours(3)),
///     actual_end_time: Some(Utc::now() - chrono::Duration::minutes(30)),
///     sessions: vec![],
///     tokens: TokenCounts::new(8_000_000, 4_000_000, 0, 0),
///     total_cost: 240.0,
///     models_used: vec!["claude-3-opus".to_string()],
///     is_active: true,
///     is_gap: false,
///     warning: Some("⚠️  Block has used 12,000,000 tokens, exceeding threshold of 10,000,000 tokens".to_string()),
/// };
/// ```
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
    /// Sessions included in this block (for backward compatibility)
    pub sessions: Vec<SessionUsage>,
    /// Total tokens used in this block
    pub tokens: TokenCounts,
    /// Total cost for this block in USD
    pub total_cost: f64,
    /// List of unique models used in this block
    pub models_used: Vec<String>,
    /// Whether this block is currently active (recent activity AND within block time)
    pub is_active: bool,
    /// Whether this is a gap block (period of inactivity)
    #[serde(default)]
    pub is_gap: bool,
    /// Optional warning message if approaching or exceeding token limits
    #[serde(skip_serializing_if = "Option::is_none")]
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
    fn new(detailed: bool) -> Self {
        Self {
            tokens: TokenCounts::default(),
            cost: 0.0,
            models: HashSet::new(),
            verbose_entries: if detailed { Some(Vec::new()) } else { None },
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
        self.aggregate_daily_detailed(entries, cost_mode, false)
            .await
    }

    /// Aggregate entries by day with optional detailed mode
    pub async fn aggregate_daily_detailed(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
        detailed: bool,
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
                .or_insert_with(|| DailyAccumulator::new(detailed))
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

    /// Group sessions into 5-hour billing blocks (legacy method for backward compatibility)
    pub fn create_billing_blocks(sessions: &[SessionUsage]) -> Vec<SessionBlock> {
        if sessions.is_empty() {
            return Vec::new();
        }

        let mut blocks = Vec::new();
        let mut current_block_start: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut current_sessions = Vec::new();
        let mut current_tokens = TokenCounts::default();
        let mut current_cost = 0.0;
        let mut models_used = HashSet::new();

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
                    actual_start_time: current_sessions
                        .first()
                        .map(|s: &SessionUsage| s.start_time),
                    actual_end_time: current_sessions.last().map(|s: &SessionUsage| s.end_time),
                    sessions: std::mem::take(&mut current_sessions),
                    tokens: std::mem::take(&mut current_tokens),
                    total_cost: std::mem::take(&mut current_cost),
                    models_used: models_used
                        .drain()
                        .map(|m: ModelName| m.to_string())
                        .collect(),
                    is_active: now < block_start + five_hours,
                    is_gap: false,
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
            models_used.insert(session.model.clone());
        }

        // Handle remaining sessions
        if let Some(block_start) = current_block_start {
            let is_active = now < block_start + five_hours;
            blocks.push(SessionBlock {
                start_time: block_start,
                end_time: block_start + five_hours,
                actual_start_time: current_sessions.first().map(|s| s.start_time),
                actual_end_time: current_sessions.last().map(|s| s.end_time),
                sessions: current_sessions,
                tokens: current_tokens,
                total_cost: current_cost,
                models_used: models_used.into_iter().map(|m| m.to_string()).collect(),
                is_active,
                is_gap: false,
                warning: None,
            });
        }

        blocks
    }

    /// Helper function to finalize a block and add it to the blocks vector
    #[allow(clippy::too_many_arguments)]
    fn finalize_block(
        blocks: &mut Vec<SessionBlock>,
        block_start: chrono::DateTime<chrono::Utc>,
        session_duration: chrono::Duration,
        first_entry_time: Option<chrono::DateTime<chrono::Utc>>,
        last_entry_time: Option<chrono::DateTime<chrono::Utc>>,
        tokens: TokenCounts,
        cost: f64,
        models: HashSet<ModelName>,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        let block_end = block_start + session_duration;
        let actual_end = last_entry_time.unwrap_or(block_start);

        // Check if block is active: recent activity AND within block time window
        let is_active = (now - actual_end < session_duration) && (now < block_end);

        blocks.push(SessionBlock {
            start_time: block_start,
            end_time: block_end,
            actual_start_time: first_entry_time,
            actual_end_time: Some(actual_end),
            sessions: Vec::new(), // We don't aggregate into sessions for this method
            tokens,
            total_cost: cost,
            models_used: models.into_iter().map(|m| m.to_string()).collect(),
            is_active,
            is_gap: false,
            warning: None,
        });
    }

    /// Create billing blocks directly from usage entries (matching TypeScript implementation)
    ///
    /// **Note:** This function collects all entries into memory to sort them by timestamp,
    /// which is necessary for accurate block boundary calculation. For very large datasets,
    /// this may consume significant memory. The entries must be sorted to ensure blocks
    /// are created with correct boundaries and gap detection works properly.
    pub async fn create_billing_blocks_from_entries(
        &self,
        entries: impl Stream<Item = Result<UsageEntry>>,
        cost_mode: CostMode,
        session_duration_hours: f64,
    ) -> Result<Vec<SessionBlock>> {
        let session_duration_ms = (session_duration_hours * 60.0 * 60.0 * 1000.0) as i64;
        let session_duration = chrono::Duration::milliseconds(session_duration_ms);

        // Collect and sort entries by timestamp
        let mut all_entries = Vec::new();
        tokio::pin!(entries);
        while let Some(result) = entries.next().await {
            let entry = result?;
            all_entries.push(entry);
        }

        if all_entries.is_empty() {
            return Ok(Vec::new());
        }

        // Sort by timestamp
        all_entries.sort_by_key(|e| *e.timestamp.inner());

        let mut blocks = Vec::new();
        let mut current_block_start: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut current_tokens = TokenCounts::default();
        let mut current_cost = 0.0;
        let mut current_models = HashSet::new();
        let mut first_entry_time: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut last_entry_time: Option<chrono::DateTime<chrono::Utc>> = None;

        let now = chrono::Utc::now();

        for entry in all_entries {
            let entry_time = *entry.timestamp.inner();

            // Determine if we need to start a new block
            let needs_new_block = if let Some(block_start) = current_block_start {
                let time_since_block_start = entry_time - block_start;
                let time_since_last_entry = if let Some(last_time) = last_entry_time {
                    entry_time - last_time
                } else {
                    chrono::Duration::zero()
                };

                // New block if either:
                // 1. Time since block start exceeds session duration
                // 2. Time since last entry exceeds session duration (gap)
                time_since_block_start > session_duration
                    || time_since_last_entry > session_duration
            } else {
                true // First entry always starts a new block
            };

            if needs_new_block {
                // Finish current block if it exists
                if let Some(block_start) = current_block_start {
                    Self::finalize_block(
                        &mut blocks,
                        block_start,
                        session_duration,
                        first_entry_time,
                        last_entry_time,
                        std::mem::take(&mut current_tokens),
                        std::mem::take(&mut current_cost),
                        std::mem::take(&mut current_models),
                        now,
                    );
                }

                // Create gap block if needed
                if let Some(last_time) = last_entry_time {
                    let time_gap = entry_time - last_time;
                    if time_gap > session_duration {
                        let gap_start = last_time + session_duration;
                        let gap_end = entry_time;

                        blocks.push(SessionBlock {
                            start_time: gap_start,
                            end_time: gap_end,
                            actual_start_time: None,
                            actual_end_time: None,
                            sessions: Vec::new(),
                            tokens: TokenCounts::default(),
                            total_cost: 0.0,
                            models_used: Vec::new(),
                            is_active: false,
                            is_gap: true,
                            warning: None,
                        });
                    }
                }

                // Start new block (floored to hour)
                current_block_start = Some(Self::truncate_to_hour(entry_time));
                first_entry_time = None; // Reset first entry time for new block
            }

            // Track first entry time in this block
            if first_entry_time.is_none() {
                first_entry_time = Some(entry_time);
            }

            // Calculate cost for this entry
            let entry_cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            // Add entry to current block
            current_tokens += entry.tokens;
            current_cost += entry_cost;
            current_models.insert(entry.model.clone());
            last_entry_time = Some(entry_time);
        }

        // Handle remaining entries in the last block
        if let Some(block_start) = current_block_start {
            Self::finalize_block(
                &mut blocks,
                block_start,
                session_duration,
                first_entry_time,
                last_entry_time,
                current_tokens,
                current_cost,
                current_models,
                now,
            );
        }

        Ok(blocks)
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

    pub fn from_blocks(blocks: &[SessionBlock]) -> Self {
        let mut totals = Self::default();
        for block in blocks {
            totals.tokens += block.tokens;
            totals.total_cost += block.total_cost;
        }
        totals
    }
}

/// Helper function to filter monthly data based on a MonthFilter
pub fn filter_monthly_data(monthly_data: &mut Vec<MonthlyUsage>, month_filter: &MonthFilter) {
    monthly_data.retain(|monthly| {
        // Parse month string (YYYY-MM) to check filter
        if let Ok(date) = crate::cli::parse_date_filter(&monthly.month) {
            month_filter.matches_date(&date)
        } else {
            // This should not happen if the month format is always "YYYY-MM"
            false
        }
    });
}

/// Helper function to filter blocks based on active and recent flags
pub fn filter_blocks(blocks: &mut Vec<SessionBlock>, active: bool, recent: bool) {
    if active {
        blocks.retain(|b| b.is_active);
    }

    if recent {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
        blocks.retain(|b| b.start_time > cutoff);
    }
}

/// Helper function to filter blocks based on date range
pub fn filter_blocks_by_date(
    blocks: &mut Vec<SessionBlock>,
    since: Option<chrono::NaiveDate>,
    until: Option<chrono::NaiveDate>,
) {
    if let Some(since_date) = since {
        let since_datetime = since_date
            .and_hms_opt(0, 0, 0)
            .expect("start of day is always a valid time")
            .and_utc();
        blocks.retain(|b| b.start_time >= since_datetime);
    }

    if let Some(until_date) = until {
        // Include blocks that start on or before the until date (end of day)
        let until_datetime = until_date
            .and_hms_opt(23, 59, 59)
            .expect("end of day is always a valid time")
            .and_utc();
        blocks.retain(|b| b.start_time <= until_datetime);
    }
}

/// Helper function to apply token limit warnings to blocks
/// Returns Result to handle parsing errors
pub fn apply_token_limit_warnings(
    blocks: &mut Vec<SessionBlock>,
    limit_str: &str,
    approx_max_tokens: f64,
) -> crate::error::Result<()> {
    use crate::error::CcstatError;

    // Parse token limit (can be a number or percentage like "80%")
    let (limit_value, is_percentage) = if limit_str.ends_with('%') {
        let value = limit_str
            .trim_end_matches('%')
            .parse::<f64>()
            .map_err(|_| CcstatError::InvalidTokenLimit(limit_str.to_string()))?;
        (value / 100.0, true)
    } else {
        let value = limit_str
            .parse::<u64>()
            .map_err(|_| CcstatError::InvalidTokenLimit(limit_str.to_string()))?;
        (value as f64, false)
    };

    // Apply warnings to blocks
    for block in blocks {
        let total_tokens = block.tokens.total();
        let threshold = if is_percentage {
            approx_max_tokens * limit_value
        } else {
            limit_value
        };

        if total_tokens as f64 >= threshold {
            block.warning = Some(format!(
                "⚠️  Block has used {} tokens, exceeding threshold of {}",
                total_tokens,
                if is_percentage {
                    format!(
                        "{}% (~{:.0} tokens)",
                        (limit_value * 100.0) as u32,
                        threshold
                    )
                } else {
                    format!("{} tokens", threshold as u64)
                }
            ));
        } else if total_tokens as f64 >= threshold * 0.8 {
            block.warning = Some(format!(
                "⚠️  Block approaching limit: {} tokens used ({}% of threshold)",
                total_tokens,
                ((total_tokens as f64 / threshold) * 100.0) as u32
            ));
        }
    }

    Ok(())
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

    #[test]
    fn test_billing_blocks_active_status() {
        // Test that blocks are correctly marked as active/inactive
        // This test reproduces the bug where closed blocks are incorrectly marked as inactive

        // Create a session that started 2 hours ago (should be in an active block)
        let now = chrono::Utc::now();
        let two_hours_ago = now - chrono::Duration::hours(2);
        let six_hours_ago = now - chrono::Duration::hours(6);

        let sessions = vec![
            // Session in a block that should still be active (started 2 hours ago)
            SessionUsage {
                session_id: SessionId::new("active_session"),
                start_time: two_hours_ago,
                end_time: two_hours_ago + chrono::Duration::minutes(30),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: 0.01,
                model: ModelName::new("claude-3-opus"),
            },
            // Session that starts a new block (more than 5 hours after the first)
            SessionUsage {
                session_id: SessionId::new("new_block_session"),
                start_time: two_hours_ago
                    + chrono::Duration::hours(5)
                    + chrono::Duration::minutes(1),
                end_time: two_hours_ago
                    + chrono::Duration::hours(5)
                    + chrono::Duration::minutes(31),
                tokens: TokenCounts::new(200, 100, 0, 0),
                total_cost: 0.02,
                model: ModelName::new("claude-3-opus"),
            },
        ];

        let blocks = Aggregator::create_billing_blocks(&sessions);
        assert_eq!(blocks.len(), 2);

        // The first block should be active because it started 2 hours ago
        // and billing blocks are 5 hours long
        assert!(
            blocks[0].is_active,
            "First block should be active as it started {} hours ago",
            2
        );

        // The second block just started, so it should definitely be active
        assert!(
            blocks[1].is_active,
            "Second block should be active as it just started"
        );

        // Test with an old session (should be inactive)
        let old_sessions = vec![SessionUsage {
            session_id: SessionId::new("old_session"),
            start_time: six_hours_ago,
            end_time: six_hours_ago + chrono::Duration::minutes(30),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: 0.01,
            model: ModelName::new("claude-3-opus"),
        }];

        let old_blocks = Aggregator::create_billing_blocks(&old_sessions);
        assert_eq!(old_blocks.len(), 1);
        assert!(
            !old_blocks[0].is_active,
            "Old block should be inactive as it started 6 hours ago"
        );
    }

    #[tokio::test]
    async fn test_billing_blocks_from_entries() {
        use crate::pricing_fetcher::PricingFetcher;
        use futures::stream;

        // Create test infrastructure
        let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

        let base_time = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 10, 30, 0).unwrap();

        // Create test entries with various timestamps
        let entries = vec![
            UsageEntry {
                session_id: SessionId::new("s1"),
                timestamp: crate::types::ISOTimestamp::new(base_time),
                model: ModelName::new("claude-3-opus"),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: Some(0.01),
                project: None,
                instance_id: None,
            },
            // Entry 3 hours later (still in same block)
            UsageEntry {
                session_id: SessionId::new("s1"),
                timestamp: crate::types::ISOTimestamp::new(base_time + chrono::Duration::hours(3)),
                model: ModelName::new("claude-3-opus"),
                tokens: TokenCounts::new(200, 100, 0, 0),
                total_cost: Some(0.02),
                project: None,
                instance_id: None,
            },
            // Entry 9 hours later (should create gap block and new block)
            UsageEntry {
                session_id: SessionId::new("s2"),
                timestamp: crate::types::ISOTimestamp::new(base_time + chrono::Duration::hours(9)),
                model: ModelName::new("claude-3-sonnet"),
                tokens: TokenCounts::new(150, 75, 0, 0),
                total_cost: Some(0.015),
                project: None,
                instance_id: None,
            },
        ];

        let stream = stream::iter(entries.into_iter().map(Ok));
        let blocks = aggregator
            .create_billing_blocks_from_entries(stream, CostMode::Auto, 5.0)
            .await
            .unwrap();

        // Should have 3 blocks: first block, gap block, second block
        assert_eq!(blocks.len(), 3);

        // First block: starts at 10:00 (floored from 10:30)
        assert_eq!(
            blocks[0].start_time,
            chrono::Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap()
        );
        assert_eq!(blocks[0].tokens.input_tokens, 300); // 100 + 200
        assert!(!blocks[0].is_gap);
        assert_eq!(blocks[0].models_used, vec!["claude-3-opus"]);

        // Gap block: starts at 13:30 + 5 hours = 18:30, ends at 19:30
        assert!(blocks[1].is_gap);
        assert_eq!(
            blocks[1].start_time,
            base_time + chrono::Duration::hours(3) + chrono::Duration::hours(5)
        ); // 18:30
        assert_eq!(blocks[1].end_time, base_time + chrono::Duration::hours(9)); // 19:30
        assert_eq!(blocks[1].tokens.input_tokens, 0);
        assert!(!blocks[1].is_active);

        // Second block: starts at 19:00 (floored from 19:30)
        assert_eq!(
            blocks[2].start_time,
            chrono::Utc.with_ymd_and_hms(2024, 1, 1, 19, 0, 0).unwrap()
        );
        assert_eq!(blocks[2].tokens.input_tokens, 150);
        assert!(!blocks[2].is_gap);
        assert_eq!(blocks[2].models_used, vec!["claude-3-sonnet"]);
    }

    #[tokio::test]
    async fn test_billing_blocks_active_determination() {
        use crate::pricing_fetcher::PricingFetcher;
        use futures::stream;

        // Create test infrastructure
        let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

        let now = chrono::Utc::now();

        // Test 1: Recent activity within block time = active
        let recent_entries = vec![
            UsageEntry {
                session_id: SessionId::new("recent"),
                timestamp: crate::types::ISOTimestamp::new(now - chrono::Duration::hours(2)),
                model: ModelName::new("claude-3-opus"),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: Some(0.01),
                project: None,
                instance_id: None,
            },
            UsageEntry {
                session_id: SessionId::new("recent"),
                timestamp: crate::types::ISOTimestamp::new(now - chrono::Duration::minutes(30)),
                model: ModelName::new("claude-3-opus"),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: Some(0.01),
                project: None,
                instance_id: None,
            },
        ];

        let stream = stream::iter(recent_entries.into_iter().map(Ok));
        let blocks = aggregator
            .create_billing_blocks_from_entries(stream, CostMode::Auto, 5.0)
            .await
            .unwrap();

        assert_eq!(blocks.len(), 1);
        assert!(
            blocks[0].is_active,
            "Block with recent activity (30 min ago) should be active"
        );

        // Test 2: No recent activity even if within block time = inactive
        let old_entries = vec![UsageEntry {
            session_id: SessionId::new("old"),
            timestamp: crate::types::ISOTimestamp::new(now - chrono::Duration::hours(4)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: Some(0.01),
            project: None,
            instance_id: None,
        }];

        let stream = stream::iter(old_entries.into_iter().map(Ok));
        let blocks = aggregator
            .create_billing_blocks_from_entries(stream, CostMode::Auto, 2.0) // 2 hour blocks
            .await
            .unwrap();

        assert_eq!(blocks.len(), 1);
        // Block started 4 hours ago, last activity 4 hours ago, 2-hour session duration
        // Both conditions fail: activity > 2 hours ago, and possibly outside block window
        assert!(
            !blocks[0].is_active,
            "Block with no recent activity should be inactive"
        );
    }
}
