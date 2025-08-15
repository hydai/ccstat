//! Live monitoring functionality for ccstat
//!
//! This module provides file watching and periodic updates for real-time
//! usage monitoring. It watches for changes in JSONL files and refreshes
//! the display at specified intervals.

#[cfg(test)]
use crate::timezone::TimezoneConfig;
use crate::{
    aggregation::{
        Aggregator, SessionBlock, SessionUsage, Totals, apply_token_limit_warnings, filter_blocks,
        filter_blocks_by_date, filter_blocks_by_project, filter_monthly_data,
    },
    data_loader::DataLoader,
    error::{CcstatError, Result},
    filters::{MonthFilter, UsageFilter},
    output::get_formatter,
    types::{CostMode, UsageEntry},
};
use chrono::Local;
use futures::StreamExt;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::mpsc,
    time::{MissedTickBehavior, interval},
};

// Constants for watcher thread management
const WATCHER_POLL_INTERVAL: Duration = Duration::from_millis(100);
const WATCHER_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(200); // 2x poll interval

/// Approximate maximum tokens for a 5-hour billing block
/// This is an empirical value based on typical usage patterns
const APPROX_MAX_TOKENS_PER_BLOCK: f64 = 10_000_000.0;

/// Command type for live monitoring
#[derive(Debug, Clone)]
pub enum CommandType {
    Daily {
        instances: bool,
        detailed: bool,
    },
    Monthly,
    Session,
    Blocks {
        active: bool,
        recent: bool,
        token_limit: Option<String>,
        session_duration: f64,
    },
}

/// Live monitoring state
pub struct LiveMonitor {
    data_loader: Arc<DataLoader>,
    aggregator: Arc<Aggregator>,
    filter: UsageFilter,
    month_filter: Option<MonthFilter>,
    cost_mode: CostMode,
    json_output: bool,
    command_type: CommandType,
    interval_secs: u64,
    full_model_names: bool,
}

/// Data prepared for display
pub struct PreparedData {
    /// Filtered usage entries
    pub filtered_entries: Vec<UsageEntry>,
    /// Active session IDs (within last 5 minutes)
    pub active_sessions: Vec<String>,
    /// Daily aggregation by instance (if instances mode is enabled)
    pub instance_data: Option<Vec<crate::aggregation::DailyInstanceUsage>>,
    /// Daily aggregation (if instances mode is disabled)
    pub daily_data: Option<Vec<crate::aggregation::DailyUsage>>,
    /// Monthly aggregation
    pub monthly_data: Option<Vec<crate::aggregation::MonthlyUsage>>,
    /// Session aggregation
    pub session_data: Option<Vec<crate::aggregation::SessionUsage>>,
    /// Billing blocks
    pub blocks_data: Option<Vec<SessionBlock>>,
    /// Totals calculated from the aggregated data
    pub totals: Totals,
}

impl LiveMonitor {
    /// Create a new live monitor
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        data_loader: Arc<DataLoader>,
        aggregator: Arc<Aggregator>,
        filter: UsageFilter,
        month_filter: Option<MonthFilter>,
        cost_mode: CostMode,
        json_output: bool,
        command_type: CommandType,
        interval_secs: u64,
        full_model_names: bool,
    ) -> Self {
        Self {
            data_loader,
            aggregator,
            filter,
            month_filter,
            cost_mode,
            json_output,
            command_type,
            interval_secs,
            full_model_names,
        }
    }

    /// Start the live monitoring loop
    pub async fn run(self) -> Result<()> {
        // Track if we need to refresh
        let should_refresh = Arc::new(AtomicBool::new(true));
        let should_refresh_watcher = should_refresh.clone();

        // Track if we should stop
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_watcher = should_stop.clone();

        // Set up file watching
        let (tx, mut rx) = mpsc::channel(10);
        let watched_dirs = self.data_loader.paths().to_vec();

        // Create watcher in a separate task
        let mut watcher_handle = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut watcher = RecommendedWatcher::new(
                move |result: notify::Result<Event>| {
                    if let Ok(event) = result
                        && matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        )
                    {
                        // Check if any path is a JSONL file
                        for path in &event.paths {
                            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                                should_refresh_watcher.store(true, Ordering::Release);
                                let _ = tx.blocking_send(());
                                break;
                            }
                        }
                    }
                },
                Config::default(),
            )
            .map_err(|e| {
                CcstatError::Io(std::io::Error::other(format!(
                    "Failed to create file watcher: {e}"
                )))
            })?;

            // Watch all data directories
            for dir in watched_dirs {
                if dir.exists() {
                    watcher.watch(&dir, RecursiveMode::Recursive).map_err(|e| {
                        CcstatError::Io(std::io::Error::other(format!(
                            "Failed to watch directory {}: {e}",
                            dir.display()
                        )))
                    })?;
                }
            }

            // Keep the watcher alive until we're told to stop
            while !should_stop_watcher.load(Ordering::Acquire) {
                std::thread::sleep(WATCHER_POLL_INTERVAL);
            }

            drop(watcher);
            Ok(())
        });

        // Set up interval timer
        let mut interval = interval(Duration::from_secs(self.interval_secs));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        // Initial display
        self.refresh_display().await?;

        // Main monitoring loop
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Regular interval refresh
                    if should_refresh.load(Ordering::Acquire) {
                        self.refresh_display().await?;
                        should_refresh.store(false, Ordering::Release);
                    }
                }
                _ = rx.recv() => {
                    // File change detected, wait a bit for writes to complete
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    self.refresh_display().await?;
                    should_refresh.store(false, Ordering::Release);
                }
                _ = tokio::signal::ctrl_c() => {
                    // Graceful shutdown
                    println!("\nExiting live monitoring mode...");
                    break;
                }
            }
        }

        // Signal the watcher thread to stop
        should_stop.store(true, Ordering::Release);

        // Wait for the watcher to finish with a timeout
        tokio::select! {
            res = &mut watcher_handle => {
                match res {
                    Ok(Ok(_)) => {
                        tracing::debug!("Watcher task exited gracefully");
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Watcher task exited with an error: {}", e);
                    }
                    Err(e) if e.is_panic() => {
                        tracing::warn!("Watcher task panicked: {:?}", e);
                    }
                    Err(e) => {
                        // This is unexpected in our logic since we don't cancel elsewhere
                        tracing::warn!("Watcher task was cancelled: {}", e);
                    }
                }
            }
            _ = tokio::time::sleep(WATCHER_SHUTDOWN_TIMEOUT) => {
                watcher_handle.abort();
                // The aborted task still needs to be awaited to free resources
                if let Err(e) = watcher_handle.await
                    && e.is_panic()
                {
                    tracing::warn!("Watcher task panicked during forced shutdown: {:?}", e);
                }
                tracing::warn!(
                    "Watcher task was aborted because it did not shut down gracefully in time"
                );
            }
        }

        Ok(())
    }

    /// Helper method to aggregate session data from filtered entries
    async fn aggregate_sessions_for_watch(
        &self,
        filtered_entries: &[UsageEntry],
    ) -> Result<Vec<SessionUsage>> {
        self.aggregator
            .aggregate_sessions(
                futures::stream::iter(filtered_entries).map(|e| Ok(e.clone())),
                self.cost_mode,
            )
            .await
    }

    /// Prepare data for display by loading, filtering and aggregating
    pub async fn prepare_data(&self) -> Result<PreparedData> {
        // Load and aggregate data
        let entries = self.data_loader.load_usage_entries_parallel();

        // Apply filters and collect entries
        let filtered_entries: Vec<UsageEntry> = entries
            .filter_map(|result| async {
                match result {
                    Ok(entry) if self.filter.matches(&entry) => Some(entry),
                    _ => None,
                }
            })
            .collect()
            .await;

        // Highlight active sessions (within last 5 minutes)
        let now = chrono::Utc::now();
        let active_cutoff = now - chrono::Duration::minutes(5);

        let active_sessions: Vec<String> = filtered_entries
            .iter()
            .filter(|entry| entry.timestamp.as_ref() > &active_cutoff)
            .map(|entry| entry.session_id.as_ref().to_string())
            .collect();

        // Generate aggregated data based on command type
        let mut prepared_data = PreparedData {
            filtered_entries: filtered_entries.clone(),
            active_sessions,
            instance_data: None,
            daily_data: None,
            monthly_data: None,
            session_data: None,
            blocks_data: None,
            totals: Totals::default(),
        };

        match &self.command_type {
            CommandType::Daily {
                instances,
                detailed,
            } => {
                if *instances {
                    let instance_data = self
                        .aggregator
                        .aggregate_daily_by_instance(
                            futures::stream::iter(&filtered_entries).map(|e| Ok(e.clone())),
                            self.cost_mode,
                        )
                        .await?;
                    prepared_data.totals = Totals::from_daily_instances(&instance_data);
                    prepared_data.instance_data = Some(instance_data);
                } else {
                    let daily_data = self
                        .aggregator
                        .aggregate_daily_detailed(
                            futures::stream::iter(&filtered_entries).map(|e| Ok(e.clone())),
                            self.cost_mode,
                            *detailed,
                        )
                        .await?;
                    prepared_data.totals = Totals::from_daily(&daily_data);
                    prepared_data.daily_data = Some(daily_data);
                }
            }
            CommandType::Monthly => {
                // First aggregate daily, then monthly
                let daily_data = self
                    .aggregator
                    .aggregate_daily(
                        futures::stream::iter(&filtered_entries).map(|e| Ok(e.clone())),
                        self.cost_mode,
                    )
                    .await?;
                let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);

                // Apply month filter if present
                if let Some(month_filter) = &self.month_filter {
                    filter_monthly_data(&mut monthly_data, month_filter);
                }

                prepared_data.totals = Totals::from_monthly(&monthly_data);
                prepared_data.monthly_data = Some(monthly_data);
            }
            CommandType::Session => {
                let session_data = self.aggregate_sessions_for_watch(&filtered_entries).await?;
                prepared_data.totals = Totals::from_sessions(&session_data);
                prepared_data.session_data = Some(session_data);
            }
            CommandType::Blocks {
                active,
                recent,
                token_limit,
                session_duration,
            } => {
                // For blocks, we need ALL entries to properly detect gaps
                // Load all entries without filtering
                let all_entries = self.data_loader.load_usage_entries_parallel();
                let entries_stream = Box::pin(all_entries);

                // Create billing blocks from ALL entries to respect session_duration and gaps
                let mut blocks = self
                    .aggregator
                    .create_billing_blocks_from_entries(
                        entries_stream,
                        self.cost_mode,
                        *session_duration,
                    )
                    .await?;

                // Apply project filter if present
                if let Some(project) = self.filter.get_project() {
                    filter_blocks_by_project(&mut blocks, project);
                }

                // Apply date filters from the filter object
                filter_blocks_by_date(&mut blocks, self.filter.since_date, self.filter.until_date);

                // Apply other filters
                filter_blocks(&mut blocks, *active, *recent);

                // Apply token limit warnings
                if let Some(limit_str) = token_limit {
                    apply_token_limit_warnings(
                        &mut blocks,
                        limit_str,
                        APPROX_MAX_TOKENS_PER_BLOCK,
                    )?;
                }

                // Calculate totals from blocks
                prepared_data.totals = Totals::from_blocks(&blocks);
                prepared_data.blocks_data = Some(blocks);
            }
        }

        Ok(prepared_data)
    }

    /// Refresh the display with current data
    async fn refresh_display(&self) -> Result<()> {
        // Clear screen
        if !self.json_output {
            print!("\x1B[2J\x1B[1;1H"); // Clear screen and move cursor to top-left
        }

        // Show current time and mode
        if !self.json_output {
            let now = Local::now();
            let command_name = match &self.command_type {
                CommandType::Daily { .. } => "Daily Usage",
                CommandType::Monthly => "Monthly Usage",
                CommandType::Session => "Session Usage",
                CommandType::Blocks { .. } => "Billing Blocks",
            };

            println!(
                "Live Monitoring - {} - Last updated: {}",
                command_name,
                now.format("%Y-%m-%d %H:%M:%S")
            );
            println!(
                "Refresh interval: {}s | Press Ctrl+C to exit",
                self.interval_secs
            );
            println!("{}", "-".repeat(80));
        }

        // Prepare data for display
        let prepared_data = self.prepare_data().await?;

        // Generate output
        let formatter = get_formatter(self.json_output, self.full_model_names);

        if !self.json_output {
            // Add active session indicators for table output (relevant for daily and session views)
            match &self.command_type {
                CommandType::Daily { .. } | CommandType::Session => {
                    println!(
                        "\nActive Sessions: {}",
                        if prepared_data.active_sessions.is_empty() {
                            "None".to_string()
                        } else {
                            format!("{} session(s)", prepared_data.active_sessions.len())
                        }
                    );
                }
                _ => {}
            }
        }

        // Format output based on command type
        match &self.command_type {
            CommandType::Daily { instances, .. } => {
                if *instances {
                    if let Some(ref instance_data) = prepared_data.instance_data {
                        println!(
                            "{}",
                            formatter
                                .format_daily_by_instance(instance_data, &prepared_data.totals)
                        );
                    }
                } else if let Some(ref daily_data) = prepared_data.daily_data {
                    println!(
                        "{}",
                        formatter.format_daily(daily_data, &prepared_data.totals)
                    );
                }
            }
            CommandType::Monthly => {
                if let Some(ref monthly_data) = prepared_data.monthly_data {
                    println!(
                        "{}",
                        formatter.format_monthly(monthly_data, &prepared_data.totals)
                    );
                }
            }
            CommandType::Session => {
                if let Some(ref session_data) = prepared_data.session_data {
                    println!(
                        "{}",
                        formatter.format_sessions(
                            session_data,
                            &prepared_data.totals,
                            &self.aggregator.timezone_config().tz
                        )
                    );
                }
            }
            CommandType::Blocks { .. } => {
                if let Some(ref blocks_data) = prepared_data.blocks_data {
                    println!(
                        "{}",
                        formatter.format_blocks(blocks_data, &self.aggregator.timezone_config().tz)
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost_calculator::CostCalculator;
    use crate::pricing_fetcher::PricingFetcher;
    use crate::types::{ISOTimestamp, ModelName, SessionId, TokenCounts};
    use chrono::{TimeZone, Utc};

    /// Helper function to create a mock DataLoader for testing
    ///
    /// Attempts to create a real DataLoader for integration testing.
    /// Returns None if Claude directories are not found, allowing tests to be skipped gracefully.
    async fn create_mock_data_loader() -> Option<Arc<DataLoader>> {
        // Try to create a real DataLoader, but return None if it fails
        match DataLoader::new().await {
            Ok(loader) => Some(Arc::new(loader)),
            Err(_) => None,
        }
    }

    /// Helper function to set up test environment with common components
    ///
    /// Creates a test environment with mock data loader, pricing fetcher, cost calculator,
    /// and aggregator. Returns None if Claude directories are not found.
    ///
    /// # Returns
    /// * `Some((data_loader, aggregator, filter))` - Test components if setup succeeds
    /// * `None` - If Claude directories are not found (for graceful test skipping)
    async fn setup_test_environment() -> Option<(Arc<DataLoader>, Arc<Aggregator>, UsageFilter)> {
        let data_loader = create_mock_data_loader().await?;
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();
        Some((data_loader, aggregator, filter))
    }

    // Helper function to handle UnknownModel errors in tests
    fn handle_test_result<T>(result: crate::error::Result<T>, context: &str) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(e) => match e {
                crate::error::CcstatError::UnknownModel(_) => {
                    println!(
                        "Test data contains unknown model in '{}' (expected in test environment): {}",
                        context, e
                    );
                    None
                }
                _ => panic!("Unexpected error in {}: {}", context, e),
            },
        }
    }

    #[tokio::test]
    async fn test_live_monitor_creation() {
        // Create data loader - it's okay if it fails in CI
        let data_loader_result = DataLoader::new().await;
        let data_loader = match data_loader_result {
            Ok(loader) => Arc::new(loader),
            Err(_) => {
                // Skip test if no Claude directories exist (e.g., in CI)
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Just verify it can be created
        assert_eq!(monitor.interval_secs, 5);
    }

    #[tokio::test]
    async fn test_data_directories_discovery() {
        // Try to create data loader
        match DataLoader::new().await {
            Ok(data_loader) => {
                let dirs = data_loader.paths();

                // Ensure we get at least one directory on supported platforms
                #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
                assert!(!dirs.is_empty());
            }
            Err(CcstatError::NoClaudeDirectory) => {
                // This is expected in CI environments
                println!("No Claude directories found - this is expected in CI");
            }
            Err(e) => {
                panic!("Unexpected error creating DataLoader: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_monitor_with_different_modes() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test with JSON output
        let monitor_json = LiveMonitor::new(
            data_loader.clone(),
            aggregator.clone(),
            filter.clone(),
            None,
            CostMode::Auto,
            true, // json_output
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            10,
            false,
        );
        assert!(monitor_json.json_output);
        assert_eq!(monitor_json.interval_secs, 10);

        // Test with instances mode
        let monitor_instances = LiveMonitor::new(
            data_loader.clone(),
            aggregator.clone(),
            filter.clone(),
            None,
            CostMode::Calculate,
            false,
            CommandType::Daily {
                instances: true,
                detailed: false,
            },
            15,
            false,
        );
        // Check command type instead of instances field
        match monitor_instances.command_type {
            CommandType::Daily { instances, .. } => assert!(instances),
            _ => panic!("Expected Daily command"),
        }
        assert_eq!(monitor_instances.cost_mode, CostMode::Calculate);

        // Test with full model names
        let monitor_full_names = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Display,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            20,
            true, // full_model_names
        );
        assert!(monitor_full_names.full_model_names);
        assert_eq!(monitor_full_names.cost_mode, CostMode::Display);
    }

    #[tokio::test]
    async fn test_refresh_display_with_active_sessions() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Test refresh_display doesn't panic
        let result = monitor.refresh_display().await;

        // refresh_display might fail with unknown models in test data, but shouldn't panic
        // We accept UnknownModel errors since test data might contain future model names
        handle_test_result(result, "refresh_display with active sessions");
    }

    #[tokio::test]
    async fn test_monitor_with_filters() {
        let (data_loader, aggregator, _) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Create filter with specific project
        let filter = UsageFilter::new().with_project("test-project".to_string());

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter.clone(),
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Verify filter is applied
        assert_eq!(monitor.filter.project, Some("test-project".to_string()));
    }

    #[tokio::test]
    async fn test_watcher_constants() {
        // Test that watcher constants are properly configured
        assert_eq!(WATCHER_POLL_INTERVAL, Duration::from_millis(100));
        assert_eq!(WATCHER_SHUTDOWN_TIMEOUT, Duration::from_millis(200));

        // Ensure shutdown timeout is greater than poll interval
        assert!(WATCHER_SHUTDOWN_TIMEOUT > WATCHER_POLL_INTERVAL);
    }

    #[tokio::test]
    async fn test_different_cost_modes() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test all cost modes
        let modes = vec![CostMode::Auto, CostMode::Calculate, CostMode::Display];

        for mode in modes {
            let monitor = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter.clone(),
                None,
                mode,
                false,
                CommandType::Daily {
                    instances: false,
                    detailed: false,
                },
                5,
                false,
            );
            assert_eq!(monitor.cost_mode, mode);
        }
    }

    #[tokio::test]
    async fn test_interval_configuration() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test various interval configurations
        let intervals = vec![1, 5, 10, 30, 60];

        for interval in intervals {
            let monitor = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter.clone(),
                None,
                CostMode::Auto,
                false,
                CommandType::Daily {
                    instances: false,
                    detailed: false,
                },
                interval,
                false,
            );
            assert_eq!(monitor.interval_secs, interval);
        }
    }

    #[tokio::test]
    async fn test_monitor_creation_with_timezone() {
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));

        // Test with specific timezone
        let tz_config = TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap();
        let aggregator = Arc::new(Aggregator::new(cost_calculator, tz_config));
        let filter = UsageFilter::new();

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        assert_eq!(monitor.interval_secs, 5);
    }

    #[tokio::test]
    async fn test_active_session_detection() {
        // Create entries with different timestamps
        let now = Utc::now();
        let recent_entry = UsageEntry {
            session_id: SessionId::new("recent"),
            timestamp: ISOTimestamp::new(now - chrono::Duration::minutes(2)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        let old_entry = UsageEntry {
            session_id: SessionId::new("old"),
            timestamp: ISOTimestamp::new(now - chrono::Duration::hours(1)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        // Test that recent entry is considered active
        let active_cutoff = now - chrono::Duration::minutes(5);
        assert!(recent_entry.timestamp.as_ref() > &active_cutoff);
        assert!(old_entry.timestamp.as_ref() <= &active_cutoff);
    }

    #[tokio::test]
    async fn test_monitor_json_output_formatting() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test with JSON output enabled
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            true, // json_output
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Verify JSON output flag is set
        assert!(monitor.json_output);

        // In JSON mode, screen clearing should not happen
        // This is tested implicitly by the refresh_display method
    }

    #[tokio::test]
    async fn test_refresh_display_json_mode() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test with JSON output
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            true, // json_output
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Test that refresh_display works in JSON mode
        let result = monitor.refresh_display().await;

        // Handle potential unknown model errors in test data
        handle_test_result(result, "JSON mode");
    }

    #[tokio::test]
    async fn test_refresh_display_instances_mode() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test with instances mode enabled
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: true, // instances mode
                detailed: false,
            },
            5,
            false,
        );

        // Test that refresh_display works in instances mode
        let result = monitor.refresh_display().await;

        // Handle potential unknown model errors in test data
        handle_test_result(result, "instances mode");
    }

    #[tokio::test]
    async fn test_refresh_display_with_full_model_names() {
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test with full model names enabled
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            true, // full_model_names
        );

        // Verify the setting
        assert!(monitor.full_model_names);

        // Test that refresh_display works with full model names
        let result = monitor.refresh_display().await;

        handle_test_result(result, "full model names");
    }

    #[tokio::test]
    async fn test_monitor_with_date_filters() {
        let (data_loader, aggregator, _) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Create filter with date range using fixed dates for deterministic testing
        // Use fixed dates to ensure tests are reproducible regardless of when they're run
        // This avoids failures due to system clock differences or date changes
        let today = chrono::NaiveDate::from_ymd_opt(2024, 7, 15).unwrap();
        let week_ago = today - chrono::Duration::days(7);
        let filter = UsageFilter::new().with_since(week_ago).with_until(today);

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter.clone(),
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Test that prepare_data properly applies the date filters
        if let Some(prepared_data) = handle_test_result(
            monitor.prepare_data().await,
            "prepare_data with date filters",
        ) {
            // Verify that all filtered entries are within the specified date range
            for entry in &prepared_data.filtered_entries {
                let entry_date = entry.timestamp.as_ref().date_naive();
                assert!(
                    entry_date >= week_ago && entry_date <= today,
                    "Entry date {} is outside the filter range {} to {}",
                    entry_date,
                    week_ago,
                    today
                );
            }

            // If we have data, verify aggregation works correctly
            if !prepared_data.filtered_entries.is_empty() {
                // Check that daily_data (when not in instances mode) only contains dates within range
                if let Some(daily_data) = prepared_data.daily_data {
                    for daily in &daily_data {
                        let date = daily.date.inner();
                        assert!(
                            date >= &week_ago && date <= &today,
                            "Aggregated date {:?} is outside the filter range {} to {}",
                            daily.date,
                            week_ago,
                            today
                        );
                    }
                }

                // Verify totals are calculated (check if any token counts are non-zero)
                let has_tokens = prepared_data.totals.tokens.total() > 0;
                assert!(has_tokens || prepared_data.filtered_entries.is_empty());
            }

            println!(
                "Date filter test passed: {} entries filtered within date range",
                prepared_data.filtered_entries.len()
            );
        }

        // Also test that refresh_display still works with date filters
        let result = monitor.refresh_display().await;
        handle_test_result(result, "refresh_display with date filters");
    }

    #[tokio::test]
    async fn test_monitor_refresh_display_combinations() {
        let (data_loader, aggregator, _) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Test various combinations of settings
        let test_cases = vec![
            (true, true, CostMode::Auto, false),      // JSON + instances
            (true, false, CostMode::Calculate, true), // JSON + full names
            (false, true, CostMode::Auto, true), // Table + instances + full names (use Auto instead of Display)
        ];

        for (json_output, instances, cost_mode, full_model_names) in test_cases {
            let filter = UsageFilter::new();
            let monitor = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter,
                None,
                cost_mode,
                json_output,
                CommandType::Daily {
                    instances,
                    detailed: false,
                },
                5,
                full_model_names,
            );

            // Test refresh_display with each combination
            let result = monitor.refresh_display().await;

            handle_test_result(
                result,
                &format!(
                    "json={}, instances={}, mode={:?}, full_names={}",
                    json_output, instances, cost_mode, full_model_names
                ),
            );
        }
    }

    #[test]
    fn test_should_refresh_atomics() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // Test AtomicBool operations used in the monitor
        let should_refresh = Arc::new(AtomicBool::new(true));

        // Test initial state
        assert!(should_refresh.load(Ordering::Acquire));

        // Test store and load
        should_refresh.store(false, Ordering::Release);
        assert!(!should_refresh.load(Ordering::Acquire));

        // Test multiple threads accessing the atomic
        let should_refresh_clone = should_refresh.clone();
        std::thread::spawn(move || {
            should_refresh_clone.store(true, Ordering::Release);
        })
        .join()
        .expect("Failed to join thread");

        assert!(should_refresh.load(Ordering::Acquire));
    }

    #[test]
    fn test_watcher_constants_validity() {
        // Verify watcher constants make sense
        assert!(WATCHER_POLL_INTERVAL < WATCHER_SHUTDOWN_TIMEOUT);
        assert!(WATCHER_POLL_INTERVAL.as_millis() > 0);
        assert!(WATCHER_SHUTDOWN_TIMEOUT.as_millis() > 0);

        // Verify shutdown timeout is reasonable (not too long)
        assert!(WATCHER_SHUTDOWN_TIMEOUT.as_millis() <= 1000);
    }

    #[tokio::test]
    async fn test_active_session_cutoff_time() {
        // Test the 5-minute active session cutoff logic using fixed time for deterministic testing
        let now = chrono::Utc.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();
        let active_cutoff = now - chrono::Duration::minutes(5);

        // Create test entries with different timestamps
        let recent_entry = UsageEntry {
            session_id: SessionId::new("recent"),
            timestamp: ISOTimestamp::new(now - chrono::Duration::minutes(2)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        let old_entry = UsageEntry {
            session_id: SessionId::new("old"),
            timestamp: ISOTimestamp::new(now - chrono::Duration::minutes(10)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        let boundary_entry = UsageEntry {
            session_id: SessionId::new("boundary"),
            timestamp: ISOTimestamp::new(active_cutoff + chrono::Duration::seconds(1)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        // Test active session detection
        assert!(recent_entry.timestamp.as_ref() > &active_cutoff);
        assert!(old_entry.timestamp.as_ref() <= &active_cutoff);
        assert!(boundary_entry.timestamp.as_ref() > &active_cutoff);
    }

    #[tokio::test]
    async fn test_monitor_error_recovery() {
        // Test that the monitor can handle and recover from errors
        let (data_loader, aggregator, _) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        // Create a filter that might cause issues (e.g., future dates)
        let future_date = chrono::NaiveDate::from_ymd_opt(2099, 1, 1).unwrap();
        let filter = UsageFilter::new().with_since(future_date);

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            CostMode::Auto,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // This should not panic even with a future date filter
        let result = monitor.refresh_display().await;

        // The result should be Ok (possibly with no data) or an UnknownModel error
        match result {
            Ok(_) => {
                // Successfully handled empty/filtered data
            }
            Err(crate::error::CcstatError::UnknownModel(_)) => {
                // This is acceptable
            }
            Err(e) => {
                panic!("Unexpected error type: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_monitor_error_and_edge_case_handling() {
        // Test that the monitor can handle various error conditions and edge cases
        // This simulates scenarios like empty data, concurrent refreshes, and invalid filters
        let (data_loader, aggregator, filter) = match setup_test_environment().await {
            Some(env) => env,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };

        let _monitor = LiveMonitor::new(
            data_loader.clone(),
            aggregator.clone(),
            filter.clone(),
            None,
            CostMode::Display,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        // Test 1: Monitor should handle empty data gracefully
        let empty_filter =
            UsageFilter::new().with_since(chrono::NaiveDate::from_ymd_opt(3000, 1, 1).unwrap()); // Far future date

        let monitor_empty = LiveMonitor::new(
            data_loader.clone(),
            aggregator.clone(),
            empty_filter,
            None,
            CostMode::Display,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        let result = monitor_empty.prepare_data().await;
        match result {
            Ok(data) => {
                assert!(
                    data.filtered_entries.is_empty(),
                    "Should have no entries for future date"
                );
                assert!(
                    data.active_sessions.is_empty(),
                    "Should have no active sessions"
                );
            }
            Err(crate::error::CcstatError::UnknownModel(_)) => {
                // This is acceptable if no data exists
            }
            Err(e) => {
                panic!("Unexpected error: {}", e);
            }
        }

        // Test 2: Monitor should handle rapid refresh calls without panic
        let mut handles = vec![];
        for _ in 0..3 {
            let monitor_clone = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter.clone(),
                None,
                CostMode::Display,
                false,
                CommandType::Daily {
                    instances: false,
                    detailed: false,
                },
                5,
                false,
            );

            let handle = tokio::spawn(async move { monitor_clone.refresh_display().await });
            handles.push(handle);
        }

        // All concurrent refreshes should complete without panic
        for handle in handles {
            let result = handle.await;
            assert!(result.is_ok(), "Concurrent refresh should not panic");
        }

        // Test 3: Monitor should handle invalid date ranges gracefully
        let invalid_filter = UsageFilter::new()
            .with_since(chrono::NaiveDate::from_ymd_opt(2024, 7, 20).unwrap())
            .with_until(chrono::NaiveDate::from_ymd_opt(2024, 7, 10).unwrap()); // Until before since

        let monitor_invalid = LiveMonitor::new(
            data_loader,
            aggregator,
            invalid_filter,
            None,
            CostMode::Display,
            false,
            CommandType::Daily {
                instances: false,
                detailed: false,
            },
            5,
            false,
        );

        let result = monitor_invalid.prepare_data().await;
        match result {
            Ok(data) => {
                // Should handle invalid range by returning empty data
                assert!(
                    data.filtered_entries.is_empty(),
                    "Invalid date range should yield no entries"
                );
            }
            Err(crate::error::CcstatError::UnknownModel(_)) => {
                // This is also acceptable
            }
            Err(e) => {
                panic!("Should handle invalid date range gracefully: {}", e);
            }
        }
    }
}
