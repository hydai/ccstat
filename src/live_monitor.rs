//! Live monitoring functionality for ccstat
//!
//! This module provides file watching and periodic updates for real-time
//! usage monitoring. It watches for changes in JSONL files and refreshes
//! the display at specified intervals.

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

/// Live monitoring state
pub struct LiveMonitor {
    data_loader: Arc<DataLoader>,
    aggregator: Arc<Aggregator>,
    filter: UsageFilter,
    cost_mode: CostMode,
    json_output: bool,
    instances: bool,
    interval_secs: u64,
}

impl LiveMonitor {
    /// Create a new live monitor
    pub fn new(
        data_loader: Arc<DataLoader>,
        aggregator: Arc<Aggregator>,
        filter: UsageFilter,
        cost_mode: CostMode,
        json_output: bool,
        instances: bool,
        interval_secs: u64,
    ) -> Self {
        Self {
            data_loader,
            aggregator,
            filter,
            cost_mode,
            json_output,
            instances,
            interval_secs,
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
        let watcher_handle = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut watcher = RecommendedWatcher::new(
                move |result: notify::Result<Event>| {
                    if let Ok(event) = result {
                        if matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) {
                            // Check if any path is a JSONL file
                            for path in &event.paths {
                                if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                                    should_refresh_watcher.store(true, Ordering::Relaxed);
                                    let _ = tx.blocking_send(());
                                    break;
                                }
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
            while !should_stop_watcher.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
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
                    if should_refresh.load(Ordering::Relaxed) {
                        self.refresh_display().await?;
                        should_refresh.store(false, Ordering::Relaxed);
                    }
                }
                _ = rx.recv() => {
                    // File change detected, wait a bit for writes to complete
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    self.refresh_display().await?;
                    should_refresh.store(false, Ordering::Relaxed);
                }
                _ = tokio::signal::ctrl_c() => {
                    // Graceful shutdown
                    println!("\nExiting live monitoring mode...");
                    break;
                }
            }
        }

        // Signal the watcher thread to stop
        should_stop.store(true, Ordering::Relaxed);

        // Give the watcher thread a chance to exit cleanly
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Now abort the handle if it's still running
        if !watcher_handle.is_finished() {
            watcher_handle.abort();
        }

        // Wait for it to finish and check for panics
        match watcher_handle.await {
            Ok(result) => {
                if let Err(e) = result {
                    tracing::warn!("Watcher task exited with an error: {}", e);
                }
            }
            Err(e) => {
                if e.is_panic() {
                    tracing::warn!("Watcher task panicked: {:?}", e);
                } else if e.is_cancelled() {
                    tracing::warn!(
                        "Watcher task was aborted because it did not shut down gracefully in time"
                    );
                }
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
            let formatter = get_formatter(self.json_output);

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
            let formatter = get_formatter(self.json_output);

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
        let aggregator = Arc::new(Aggregator::new(cost_calculator));
        let filter = UsageFilter::new();

        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            CostMode::Auto,
            false,
            false,
            5,
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
}
