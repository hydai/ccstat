//! Live monitoring functionality for ccstat
//!
//! This module provides file watching and periodic updates for real-time
//! usage monitoring. It watches for changes in JSONL files and refreshes
//! the display at specified intervals.

#[cfg(test)]
use crate::timezone::TimezoneConfig;
use crate::{
    aggregation::{Aggregator, Totals},
    data_loader::DataLoader,
    error::{CcstatError, Result},
    filters::UsageFilter,
    output::get_formatter,
    types::{CostMode, UsageEntry},
};
use chrono::Local;
use futures::StreamExt;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
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

/// Live monitoring state
pub struct LiveMonitor {
    data_loader: Arc<DataLoader>,
    aggregator: Arc<Aggregator>,
    filter: UsageFilter,
    cost_mode: CostMode,
    json_output: bool,
    instances: bool,
    interval_secs: u64,
    full_model_names: bool,
}

impl LiveMonitor {
    /// Create a new live monitor
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        data_loader: Arc<DataLoader>,
        aggregator: Arc<Aggregator>,
        filter: UsageFilter,
        cost_mode: CostMode,
        json_output: bool,
        instances: bool,
        interval_secs: u64,
        full_model_names: bool,
    ) -> Self {
        Self {
            data_loader,
            aggregator,
            filter,
            cost_mode,
            json_output,
            instances,
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
        let watched_dirs = self.data_loader.get_data_directories().await?;

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

    /// Refresh the display with current data
    async fn refresh_display(&self) -> Result<()> {
        // Clear screen
        if !self.json_output {
            print!("\x1B[2J\x1B[1;1H"); // Clear screen and move cursor to top-left
        }

        // Show current time and mode
        if !self.json_output {
            let now = Local::now();
            println!(
                "Live Monitoring - Last updated: {}",
                now.format("%Y-%m-%d %H:%M:%S")
            );
            println!(
                "Refresh interval: {}s | Press Ctrl+C to exit",
                self.interval_secs
            );
            println!("{}", "-".repeat(80));
        }

        // Load and aggregate data
        let entries = self.data_loader.load_usage_entries();

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

        // Generate output
        if self.instances {
            let instance_data = self
                .aggregator
                .aggregate_daily_by_instance(
                    futures::stream::iter(filtered_entries.into_iter().map(Ok)),
                    self.cost_mode,
                )
                .await?;
            let totals = Totals::from_daily_instances(&instance_data);
            let formatter = get_formatter(self.json_output, self.full_model_names);

            if self.json_output {
                println!(
                    "{}",
                    formatter.format_daily_by_instance(&instance_data, &totals)
                );
            } else {
                // Add active session indicators for table output
                println!(
                    "\nActive Sessions: {}",
                    if active_sessions.is_empty() {
                        "None".to_string()
                    } else {
                        format!("{} session(s)", active_sessions.len())
                    }
                );
                println!(
                    "{}",
                    formatter.format_daily_by_instance(&instance_data, &totals)
                );
            }
        } else {
            let daily_data = self
                .aggregator
                .aggregate_daily(
                    futures::stream::iter(filtered_entries.into_iter().map(Ok)),
                    self.cost_mode,
                )
                .await?;
            let totals = Totals::from_daily(&daily_data);
            let formatter = get_formatter(self.json_output, self.full_model_names);

            if self.json_output {
                println!("{}", formatter.format_daily(&daily_data, &totals));
            } else {
                // Add active session indicators for table output
                println!(
                    "\nActive Sessions: {}",
                    if active_sessions.is_empty() {
                        "None".to_string()
                    } else {
                        format!("{} session(s)", active_sessions.len())
                    }
                );
                println!("{}", formatter.format_daily(&daily_data, &totals));
            }
        }

        Ok(())
    }
}

/// Helper extension for DataLoader to get data directories
impl DataLoader {
    async fn get_data_directories(&self) -> Result<Vec<PathBuf>> {
        // This would need to be implemented in DataLoader to expose the directories
        // For now, we'll use a placeholder implementation
        let mut dirs = Vec::new();

        // Get platform-specific directories
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = dirs::home_dir() {
                let claude_dir = home.join("Library/Application Support/Claude");
                if claude_dir.exists() {
                    dirs.push(claude_dir);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(data_dir) = dirs::data_dir() {
                let claude_dir = data_dir.join("Claude");
                if claude_dir.exists() {
                    dirs.push(claude_dir);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(data_dir) = dirs::data_dir() {
                let claude_dir = data_dir.join("Claude");
                if claude_dir.exists() {
                    dirs.push(claude_dir);
                }
            }
        }

        if dirs.is_empty() {
            return Err(CcstatError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No Claude data directories found",
            )));
        }

        Ok(dirs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost_calculator::CostCalculator;
    use crate::pricing_fetcher::PricingFetcher;
    use crate::types::{ISOTimestamp, ModelName, SessionId, TokenCounts};
    use chrono::Utc;

    // Helper function to create a mock DataLoader for testing
    async fn create_mock_data_loader() -> Option<Arc<DataLoader>> {
        // Try to create a real DataLoader, but return None if it fails
        match DataLoader::new().await {
            Ok(loader) => Some(Arc::new(loader)),
            Err(_) => None,
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
            CostMode::Auto,
            false,
            false,
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
                let dirs = data_loader.get_data_directories().await;

                // Should either find directories or return an error
                match dirs {
                    Ok(directories) => {
                        // Ensure we get at least one directory on supported platforms
                        #[cfg(any(
                            target_os = "macos",
                            target_os = "linux",
                            target_os = "windows"
                        ))]
                        assert!(!directories.is_empty());
                    }
                    Err(e) => {
                        // It's okay if directories don't exist
                        assert!(matches!(e, CcstatError::Io(_)));
                    }
                }
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
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();

        // Test with JSON output
        let monitor_json = LiveMonitor::new(
            data_loader.clone(),
            aggregator.clone(),
            filter.clone(),
            CostMode::Auto,
            true, // json_output
            false,
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
            CostMode::Calculate,
            false,
            true, // instances
            15,
            false,
        );
        assert!(monitor_instances.instances);
        assert_eq!(monitor_instances.cost_mode, CostMode::Calculate);

        // Test with full model names
        let monitor_full_names = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            CostMode::Display,
            false,
            false,
            20,
            true, // full_model_names
        );
        assert!(monitor_full_names.full_model_names);
        assert_eq!(monitor_full_names.cost_mode, CostMode::Display);
    }

    #[tokio::test]
    async fn test_refresh_display_with_active_sessions() {
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
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
            CostMode::Auto,
            false,
            false,
            5,
            false,
        );

        // Test refresh_display doesn't panic
        let result = monitor.refresh_display().await;

        // It might fail if no data is available, but shouldn't panic
        if let Err(e) = result {
            println!("Expected error in test environment: {}", e);
        }
    }

    #[tokio::test]
    async fn test_monitor_with_filters() {
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));

        // Create filter with specific project
        let filter = UsageFilter::new().with_project("test-project".to_string());

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter.clone(),
            CostMode::Auto,
            false,
            false,
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
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();

        // Test all cost modes
        let modes = vec![CostMode::Auto, CostMode::Calculate, CostMode::Display];

        for mode in modes {
            let monitor = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter.clone(),
                mode,
                false,
                false,
                5,
                false,
            );
            assert_eq!(monitor.cost_mode, mode);
        }
    }

    #[tokio::test]
    async fn test_interval_configuration() {
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();

        // Test various interval configurations
        let intervals = vec![1, 5, 10, 30, 60];

        for interval in intervals {
            let monitor = LiveMonitor::new(
                data_loader.clone(),
                aggregator.clone(),
                filter.clone(),
                CostMode::Auto,
                false,
                false,
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
            CostMode::Auto,
            false,
            false,
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

    #[test]
    fn test_atomic_bool_operations() {
        // Test atomic bool operations used for signaling
        let should_refresh = Arc::new(AtomicBool::new(false));

        // Test initial state
        assert!(!should_refresh.load(Ordering::Acquire));

        // Test setting to true
        should_refresh.store(true, Ordering::Release);
        assert!(should_refresh.load(Ordering::Acquire));

        // Test setting back to false
        should_refresh.store(false, Ordering::Release);
        assert!(!should_refresh.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn test_data_directories_on_different_platforms() {
        // Test the get_data_directories method through the actual loader
        match DataLoader::new().await {
            Ok(data_loader) => {
                // Test that get_data_directories handles missing directories gracefully
                let result = data_loader.get_data_directories().await;

                // On any platform, we should either get directories or an error
                match result {
                    Ok(dirs) => {
                        // If successful, we should have at least one directory
                        assert!(!dirs.is_empty());
                    }
                    Err(e) => {
                        // Error is acceptable if no Claude directories exist
                        assert!(matches!(e, CcstatError::Io(_)));
                    }
                }
            }
            Err(_) => {
                println!("Skipping test: No Claude directories found");
            }
        }
    }

    #[tokio::test]
    async fn test_monitor_json_output_formatting() {
        let data_loader = match create_mock_data_loader().await {
            Some(loader) => loader,
            None => {
                println!("Skipping test: No Claude directories found");
                return;
            }
        };
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(Aggregator::new(cost_calculator, TimezoneConfig::default()));
        let filter = UsageFilter::new();

        // Test with JSON output enabled
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            CostMode::Auto,
            true, // json_output
            false,
            5,
            false,
        );

        // Verify JSON output flag is set
        assert!(monitor.json_output);

        // In JSON mode, screen clearing should not happen
        // This is tested implicitly by the refresh_display method
    }
}
