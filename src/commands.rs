//! Command execution module for ccstat
//!
//! This module contains the implementation logic for each CLI command,
//! extracted from main.rs to enable better testing.

use crate::{
    aggregation::{Aggregator, Totals},
    cli::{parse_date_filter, parse_month_filter},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::{CcstatError, Result},
    filters::{MonthFilter, UsageFilter},
    live_monitor::LiveMonitor,
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    types::CostMode,
};
use std::sync::Arc;
use tracing::info;

/// Configuration for daily command execution
pub struct DailyConfig {
    pub mode: CostMode,
    pub json: bool,
    pub since: Option<String>,
    pub until: Option<String>,
    pub instances: bool,
    pub project: Option<String>,
    pub watch: bool,
    pub interval: u64,
    pub parallel: bool,
    pub intern: bool,
    pub arena: bool,
    pub verbose: bool,
}

/// Execute the daily command
pub async fn execute_daily(config: DailyConfig) -> Result<()> {
    info!("Running daily usage report");

    // Initialize components with progress bars enabled for terminal output
    let show_progress = !config.json && !config.watch && is_terminal::is_terminal(std::io::stdout());
    let data_loader = Arc::new(
        DataLoader::new()
            .await?
            .with_progress(show_progress)
            .with_interning(config.intern)
            .with_arena(config.arena),
    );
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(Aggregator::new(cost_calculator).with_progress(show_progress));

    // Build filter
    let mut filter = UsageFilter::new();

    if let Some(since_str) = &config.since {
        let since_date = parse_date_filter(since_str)?;
        filter = filter.with_since(since_date);
    }
    if let Some(until_str) = &config.until {
        let until_date = parse_date_filter(until_str)?;
        filter = filter.with_until(until_date);
    }
    if let Some(project_name) = &config.project {
        filter = filter.with_project(project_name.clone());
    }

    // Check if we're in watch mode
    if config.watch {
        info!("Starting live monitoring mode");
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            config.mode,
            config.json,
            config.instances,
            config.interval,
        );
        monitor.run().await?;
    } else {
        // Handle instances flag
        if config.instances {
            // Load and filter entries, then group by instance
            if config.parallel {
                let entries = data_loader.load_usage_entries_parallel();
                let filtered_entries = filter.filter_stream(entries).await;
                let instance_data = aggregator
                    .aggregate_daily_by_instance(filtered_entries, config.mode)
                    .await?;
                let totals = Totals::from_daily_instances(&instance_data);
                let formatter = get_formatter(config.json);
                println!(
                    "{}",
                    formatter.format_daily_by_instance(&instance_data, &totals)
                );
            } else {
                let entries = data_loader.load_usage_entries();
                let filtered_entries = filter.filter_stream(entries).await;
                let instance_data = aggregator
                    .aggregate_daily_by_instance(filtered_entries, config.mode)
                    .await?;
                let totals = Totals::from_daily_instances(&instance_data);
                let formatter = get_formatter(config.json);
                println!(
                    "{}",
                    formatter.format_daily_by_instance(&instance_data, &totals)
                );
            }
        } else {
            // Load and filter entries, then aggregate normally
            if config.parallel {
                let entries = data_loader.load_usage_entries_parallel();
                let filtered_entries = filter.filter_stream(entries).await;
                let daily_data = aggregator
                    .aggregate_daily_verbose(filtered_entries, config.mode, config.verbose)
                    .await?;
                let totals = Totals::from_daily(&daily_data);
                let formatter = get_formatter(config.json);
                println!("{}", formatter.format_daily(&daily_data, &totals));
            } else {
                let entries = data_loader.load_usage_entries();
                let filtered_entries = filter.filter_stream(entries).await;
                let daily_data = aggregator
                    .aggregate_daily_verbose(filtered_entries, config.mode, config.verbose)
                    .await?;
                let totals = Totals::from_daily(&daily_data);
                let formatter = get_formatter(config.json);
                println!("{}", formatter.format_daily(&daily_data, &totals));
            }
        }
    }

    Ok(())
}

/// Configuration for monthly command execution
pub struct MonthlyConfig {
    pub mode: CostMode,
    pub json: bool,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// Execute the monthly command
pub async fn execute_monthly(config: MonthlyConfig) -> Result<()> {
    info!("Running monthly usage report");

    // Initialize components with progress bars enabled for terminal output
    let show_progress = !config.json && is_terminal::is_terminal(std::io::stdout());
    let data_loader = DataLoader::new().await?.with_progress(show_progress);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

    // Build month filter
    let mut month_filter = MonthFilter::new();

    if let Some(since_str) = &config.since {
        let (year, month) = parse_month_filter(since_str)?;
        month_filter = month_filter.with_since(year, month);
    }
    if let Some(until_str) = &config.until {
        let (year, month) = parse_month_filter(until_str)?;
        month_filter = month_filter.with_until(year, month);
    }

    // Load entries
    let entries = data_loader.load_usage_entries();

    // Aggregate data
    let daily_data = aggregator.aggregate_daily(entries, config.mode).await?;
    let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);

    // Apply month filter to aggregated monthly data
    monthly_data.retain(|monthly| {
        // Parse month string (YYYY-MM) to check filter
        if let Ok((year, month)) = monthly
            .month
            .split_once('-')
            .and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?)))
            .ok_or(())
        {
            // Create a date for the first day of the month to check filter
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, 1) {
                return month_filter.matches_date(&date);
            }
        }
        false
    });

    let mut totals = Totals::default();
    for monthly in &monthly_data {
        totals.tokens += monthly.tokens;
        totals.total_cost += monthly.total_cost;
    }

    // Format and output
    let formatter = get_formatter(config.json);
    println!("{}", formatter.format_monthly(&monthly_data, &totals));

    Ok(())
}

/// Configuration for session command execution
pub struct SessionConfig {
    pub mode: CostMode,
    pub json: bool,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// Execute the session command
pub async fn execute_session(config: SessionConfig) -> Result<()> {
    info!("Running session usage report");

    // Initialize components with progress bars enabled for terminal output
    let show_progress = !config.json && is_terminal::is_terminal(std::io::stdout());
    let data_loader = DataLoader::new().await?.with_progress(show_progress);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

    // Build filter
    let mut filter = UsageFilter::new();

    if let Some(since_str) = &config.since {
        let since_date = parse_date_filter(since_str)?;
        filter = filter.with_since(since_date);
    }
    if let Some(until_str) = &config.until {
        let until_date = parse_date_filter(until_str)?;
        filter = filter.with_until(until_date);
    }

    // Load and filter entries
    let entries = data_loader.load_usage_entries();
    let filtered_entries = filter.filter_stream(entries).await;

    // Aggregate data
    let session_data = aggregator
        .aggregate_sessions(filtered_entries, config.mode)
        .await?;
    let totals = Totals::from_sessions(&session_data);

    // Format and output
    let formatter = get_formatter(config.json);
    println!("{}", formatter.format_sessions(&session_data, &totals));

    Ok(())
}

/// Configuration for blocks command execution
pub struct BlocksConfig {
    pub mode: CostMode,
    pub json: bool,
    pub active: bool,
    pub recent: bool,
    pub token_limit: Option<String>,
}

/// Execute the blocks command
pub async fn execute_blocks(config: BlocksConfig) -> Result<()> {
    info!("Running billing blocks report");

    // Initialize components with progress bars enabled for terminal output
    let show_progress = !config.json && is_terminal::is_terminal(std::io::stdout());
    let data_loader = DataLoader::new().await?.with_progress(show_progress);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

    // Load entries
    let entries = data_loader.load_usage_entries();

    // Aggregate sessions first
    let session_data = aggregator.aggregate_sessions(entries, config.mode).await?;

    // Create billing blocks
    let mut blocks = Aggregator::create_billing_blocks(&session_data);

    // Apply filters
    if config.active {
        blocks.retain(|b| b.is_active);
    }

    if config.recent {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
        blocks.retain(|b| b.start_time > cutoff);
    }

    // Apply token limit warnings
    if let Some(limit_str) = &config.token_limit {
        apply_token_limit_warnings(&mut blocks, &limit_str)?;
    }

    // Format and output
    let formatter = get_formatter(config.json);
    println!("{}", formatter.format_blocks(&blocks));

    Ok(())
}

/// Apply token limit warnings to blocks
fn apply_token_limit_warnings(blocks: &mut [crate::aggregation::SessionBlock], limit_str: &str) -> Result<()> {
    // Parse token limit (can be a number or percentage like "80%")
    let (limit_value, is_percentage) = if limit_str.ends_with('%') {
        let value = limit_str
            .trim_end_matches('%')
            .parse::<f64>()
            .map_err(|_| {
                CcstatError::InvalidDate(format!(
                    "Invalid token limit: {limit_str}"
                ))
            })?;
        (value / 100.0, true)
    } else {
        let value = limit_str.parse::<u64>().map_err(|_| {
            CcstatError::InvalidDate(format!(
                "Invalid token limit: {limit_str}"
            ))
        })?;
        (value as f64, false)
    };

    // Apply warnings to blocks
    for block in blocks {
        if block.is_active {
            let total_tokens = block.tokens.total();
            let threshold = if is_percentage {
                // Assuming 5-hour block has a typical max of ~10M tokens
                10_000_000.0 * limit_value
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
    }

    Ok(())
}

/// Execute the default command (daily report with no filters)
pub async fn execute_default() -> Result<()> {
    info!("No command specified, running daily report");

    // Initialize components with progress bars enabled for terminal output
    let show_progress = is_terminal::is_terminal(std::io::stdout());
    let data_loader = DataLoader::new().await?.with_progress(show_progress);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

    // Load entries
    let entries = data_loader.load_usage_entries();

    // Aggregate data
    let daily_data = aggregator
        .aggregate_daily(entries, Default::default())
        .await?;
    let totals = Totals::from_daily(&daily_data);

    // Format and output
    let formatter = get_formatter(false);
    println!("{}", formatter.format_daily(&daily_data, &totals));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;

    async fn setup_test_environment() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        unsafe {
            env::set_var("CLAUDE_DATA_PATH", temp_dir.path());
        }
        temp_dir
    }

    #[tokio::test]
    async fn test_execute_default_no_data() {
        let _temp_dir = setup_test_environment().await;

        // Should succeed even with no data
        let result = execute_default().await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_daily_config_creation() {
        let config = DailyConfig {
            mode: CostMode::Auto,
            json: true,
            since: None,
            until: None,
            instances: false,
            project: None,
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: false,
        };

        assert_eq!(config.mode, CostMode::Auto);
        assert!(config.json);
        assert_eq!(config.interval, 5);
    }

    #[tokio::test]
    async fn test_apply_token_limit_warnings() {
        use crate::aggregation::{SessionBlock, SessionUsage};
        use crate::types::{TokenCounts, ModelName, SessionId};
        use chrono::Utc;

        let now = Utc::now();
        let session = SessionUsage {
            session_id: SessionId::new("test"),
            start_time: now - chrono::Duration::hours(1),
            end_time: now,
            tokens: TokenCounts::new(9_000_000, 0, 0, 0),
            total_cost: 100.0,
            model: ModelName::new("claude-3-opus"),
        };

        let mut blocks = vec![
            SessionBlock {
                start_time: now - chrono::Duration::hours(1),
                end_time: now,
                sessions: vec![session],
                tokens: TokenCounts::new(9_000_000, 0, 0, 0),
                total_cost: 100.0,
                is_active: true,
                warning: None,
            },
        ];

        // Test percentage limit
        let result = apply_token_limit_warnings(&mut blocks, "80%");
        assert!(result.is_ok());
        assert!(blocks[0].warning.is_some());
        assert!(blocks[0].warning.as_ref().unwrap().contains("exceeding threshold"));

        // Test absolute limit
        blocks[0].warning = None;
        let result = apply_token_limit_warnings(&mut blocks, "8000000");
        assert!(result.is_ok());
        assert!(blocks[0].warning.is_some());

        // Test invalid limit
        let result = apply_token_limit_warnings(&mut blocks, "invalid");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_daily_with_instances() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: true,
            since: None,
            until: None,
            instances: true,
            project: None,
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_with_verbose() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: false,
            since: None,
            until: None,
            instances: false,
            project: None,
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: true,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_with_date_filters() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Calculate,
            json: true,
            since: Some("2024-01-01".to_string()),
            until: Some("2024-12-31".to_string()),
            instances: false,
            project: None,
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_invalid_date() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: false,
            since: Some("invalid-date".to_string()),
            until: None,
            instances: false,
            project: None,
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_err());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_monthly_with_filters() {
        let _temp_dir = setup_test_environment().await;

        let config = MonthlyConfig {
            mode: CostMode::Calculate,
            json: true,
            since: Some("2024-01".to_string()),
            until: Some("2024-12".to_string()),
        };

        let result = execute_monthly(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_monthly_invalid_month() {
        let _temp_dir = setup_test_environment().await;

        let config = MonthlyConfig {
            mode: CostMode::Auto,
            json: false,
            since: Some("2024-13".to_string()),
            until: None,
        };

        let result = execute_monthly(config).await;
        assert!(result.is_err());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_session_with_filters() {
        let _temp_dir = setup_test_environment().await;

        let config = SessionConfig {
            mode: CostMode::Display,
            json: true,
            since: Some("2024-01-01".to_string()),
            until: Some("2024-12-31".to_string()),
        };

        let result = execute_session(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_blocks_with_filters() {
        let _temp_dir = setup_test_environment().await;

        let config = BlocksConfig {
            mode: CostMode::Calculate,
            json: true,
            active: true,
            recent: true,
            token_limit: Some("80%".to_string()),
        };

        let result = execute_blocks(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_blocks_invalid_token_limit() {
        let _temp_dir = setup_test_environment().await;

        let config = BlocksConfig {
            mode: CostMode::Auto,
            json: false,
            active: false,
            recent: false,
            token_limit: Some("not-a-number".to_string()),
        };

        let result = execute_blocks(config).await;
        assert!(result.is_err());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_with_project_filter() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: false,
            since: None,
            until: None,
            instances: false,
            project: Some("test-project".to_string()),
            watch: false,
            interval: 5,
            parallel: false,
            intern: false,
            arena: false,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_parallel_with_instances() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: true,
            since: None,
            until: None,
            instances: true,
            project: None,
            watch: false,
            interval: 5,
            parallel: true,
            intern: false,
            arena: false,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[tokio::test]
    async fn test_execute_daily_with_memory_opts() {
        let _temp_dir = setup_test_environment().await;

        let config = DailyConfig {
            mode: CostMode::Auto,
            json: false,
            since: None,
            until: None,
            instances: false,
            project: None,
            watch: false,
            interval: 5,
            parallel: true,
            intern: true,
            arena: true,
            verbose: false,
        };

        let result = execute_daily(config).await;
        assert!(result.is_ok());

        unsafe {
            env::remove_var("CLAUDE_DATA_PATH");
        }
    }

    #[test]
    fn test_apply_token_limit_warnings_percentage() {
        use crate::aggregation::{SessionBlock, SessionUsage};
        use crate::types::{TokenCounts, ModelName, SessionId};
        use chrono::Utc;

        let now = Utc::now();
        let session = SessionUsage {
            session_id: SessionId::new("test"),
            start_time: now - chrono::Duration::hours(1),
            end_time: now,
            tokens: TokenCounts::new(8_500_000, 0, 0, 0),
            total_cost: 100.0,
            model: ModelName::new("claude-3-opus"),
        };

        let mut blocks = vec![
            SessionBlock {
                start_time: now - chrono::Duration::hours(1),
                end_time: now,
                sessions: vec![session],
                tokens: TokenCounts::new(8_500_000, 0, 0, 0),
                total_cost: 100.0,
                is_active: true,
                warning: None,
            },
        ];

        // Test percentage approaching threshold
        let result = apply_token_limit_warnings(&mut blocks, "90%");
        assert!(result.is_ok());
        assert!(blocks[0].warning.is_some());
        assert!(blocks[0].warning.as_ref().unwrap().contains("approaching limit"));
    }

    #[test]
    fn test_apply_token_limit_warnings_inactive_block() {
        use crate::aggregation::{SessionBlock, SessionUsage};
        use crate::types::{TokenCounts, ModelName, SessionId};
        use chrono::Utc;

        let now = Utc::now();
        let session = SessionUsage {
            session_id: SessionId::new("test"),
            start_time: now - chrono::Duration::hours(6),
            end_time: now - chrono::Duration::hours(5),
            tokens: TokenCounts::new(12_000_000, 0, 0, 0),
            total_cost: 100.0,
            model: ModelName::new("claude-3-opus"),
        };

        let mut blocks = vec![
            SessionBlock {
                start_time: now - chrono::Duration::hours(6),
                end_time: now - chrono::Duration::hours(1),
                sessions: vec![session],
                tokens: TokenCounts::new(12_000_000, 0, 0, 0),
                total_cost: 100.0,
                is_active: false, // Inactive block
                warning: None,
            },
        ];

        // Inactive blocks should not get warnings
        let result = apply_token_limit_warnings(&mut blocks, "80%");
        assert!(result.is_ok());
        assert!(blocks[0].warning.is_none());
    }
}
