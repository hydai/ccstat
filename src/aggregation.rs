//! Aggregation module for summarizing usage data
//!
//! This module provides functionality to aggregate raw usage entries into
//! meaningful summaries like daily usage, monthly rollups, session statistics,
//! and billing blocks.
//!
//! # Examples
//!
//! ```no_run
//! use ccstat::{
//!     aggregation::Aggregator,
//!     cost_calculator::CostCalculator,
//!     data_loader::DataLoader,
//!     pricing_fetcher::PricingFetcher,
//!     types::CostMode,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> ccstat::Result<()> {
//! let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
//! let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
//! let aggregator = Aggregator::new(cost_calculator);
//!
//! let data_loader = DataLoader::new().await?;
//! let entries = data_loader.load_usage_entries();
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
use crate::types::{CostMode, DailyDate, ModelName, SessionId, TokenCounts, UsageEntry};
use futures::stream::{Stream, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Daily usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    /// Date of usage
    pub date: DailyDate,
    /// Token counts for the day
    pub tokens: TokenCounts,
    /// Total cost for the day
    pub total_cost: f64,
    /// Models used during the day
    pub models_used: Vec<String>,
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
    /// Start timestamp
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End timestamp
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
    /// Year and month (YYYY-MM)
    pub month: String,
    /// Token counts for the month
    pub tokens: TokenCounts,
    /// Total cost for the month
    pub total_cost: f64,
    /// Number of days with usage
    pub active_days: usize,
}

/// 5-hour billing block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBlock {
    /// Block start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Block end time (5 hours later)
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Sessions in this block
    pub sessions: Vec<SessionUsage>,
    /// Total tokens in block
    pub tokens: TokenCounts,
    /// Total cost in block
    pub total_cost: f64,
    /// Whether block is currently active
    pub is_active: bool,
    /// Optional warning message if approaching token limit
    pub warning: Option<String>,
}

/// Accumulator for daily aggregation
struct DailyAccumulator {
    tokens: TokenCounts,
    cost: f64,
    models: HashSet<ModelName>,
}

impl DailyAccumulator {
    fn new() -> Self {
        Self {
            tokens: TokenCounts::default(),
            cost: 0.0,
            models: HashSet::new(),
        }
    }

    fn add_entry(&mut self, entry: UsageEntry, calculated_cost: f64) {
        self.tokens += entry.tokens;
        self.cost += calculated_cost;
        self.models.insert(entry.model);
    }

    fn into_daily_usage(self, date: DailyDate) -> DailyUsage {
        DailyUsage {
            date,
            tokens: self.tokens,
            total_cost: self.cost,
            models_used: self.models.into_iter().map(|m| m.to_string()).collect(),
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

    fn add_entry(&mut self, entry: UsageEntry, calculated_cost: f64) {
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
            self.primary_model = Some(entry.model);
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
}

impl Aggregator {
    /// Create a new Aggregator
    pub fn new(cost_calculator: Arc<CostCalculator>) -> Self {
        Self { 
            cost_calculator,
            show_progress: false,
        }
    }
    
    /// Enable or disable progress bars
    pub fn with_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
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
            let date = DailyDate::from_timestamp(&entry.timestamp);
            let instance_id = entry.instance_id.clone().unwrap_or_else(|| "default".to_string());

            // Calculate cost
            let cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            daily_map
                .entry((date, instance_id.clone()))
                .or_insert_with(DailyAccumulator::new)
                .add_entry(entry, cost);
                
            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }
        
        if let Some(pb) = progress {
            pb.finish_with_message(format!("Aggregated {} entries", count));
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
            let date = DailyDate::from_timestamp(&entry.timestamp);

            // Calculate cost
            let cost = self
                .cost_calculator
                .calculate_with_mode(&entry.tokens, &entry.model, entry.total_cost, cost_mode)
                .await?;

            daily_map
                .entry(date)
                .or_insert_with(DailyAccumulator::new)
                .add_entry(entry, cost);
                
            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }
        
        if let Some(pb) = progress {
            pb.finish_with_message(format!("Aggregated {} entries into {} days", count, daily_map.len()));
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
                .add_entry(entry, cost);
                
            count += 1;
            if let Some(ref pb) = progress {
                pb.set_position(count);
            }
        }
        
        if let Some(pb) = progress {
            pb.finish_with_message(format!("Aggregated {} entries into {} sessions", count, session_map.len()));
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
            if let Some(block_start) = current_block_start {
                if session.start_time >= block_start + five_hours {
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
            }

            // Start new block if needed
            if current_block_start.is_none() {
                current_block_start = Some(session.start_time);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_daily_accumulator() {
        let mut acc = DailyAccumulator::new();

        let entry = UsageEntry {
            session_id: SessionId::new("test"),
            timestamp: crate::types::ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 10, 5),
            total_cost: Some(0.01),
            project: None,
            instance_id: None,
        };

        acc.add_entry(entry, 0.01);
        assert_eq!(acc.tokens.input_tokens, 100);
        assert_eq!(acc.cost, 0.01);
        assert_eq!(acc.models.len(), 1);
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
}
