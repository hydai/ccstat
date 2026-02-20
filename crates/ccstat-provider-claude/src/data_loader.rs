//! Data loader module for discovering and parsing JSONL files
//!
//! This module handles platform-specific discovery of Claude usage data files
//! and provides streaming access to parse large JSONL files efficiently.
//!
//! # Platform Support
//!
//! The data loader automatically discovers Claude data directories on:
//! - macOS: `~/Library/Application Support/Claude`
//! - Linux: `~/.config/Claude` or `$XDG_CONFIG_HOME/Claude`
//! - Windows: `%APPDATA%\Claude`
//!
//! You can override the search path using the `CLAUDE_DATA_PATH` environment variable.
//!
//! # Examples
//!
//! ```no_run
//! use ccstat_provider_claude::data_loader::DataLoader;
//! use futures::StreamExt;
//!
//! # async fn example() -> ccstat_core::Result<()> {
//! let data_loader = DataLoader::new().await?;
//!
//! // Stream usage entries
//! let entries = data_loader.load_usage_entries_parallel();
//! tokio::pin!(entries);
//! while let Some(result) = entries.next().await {
//!     let entry = result?;
//!     println!("Session: {}, Tokens: {}", entry.session_id, entry.tokens.total());
//! }
//! # Ok(())
//! # }
//! ```

use ccstat_core::error::{CcstatError, Result};
use ccstat_core::memory_pool::MemoryPool;
use ccstat_core::string_pool::{InternedModel, InternedSession};
use ccstat_core::types::{ModelName, RawJsonlEntry, SessionId, UsageEntry};
use futures::StreamExt;
use futures::stream::Stream;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, info, trace};

/// Data loader for discovering and streaming JSONL files
///
/// The DataLoader is responsible for finding Claude usage data files on the
/// system and providing efficient streaming access to parse them.
pub struct DataLoader {
    /// Discovered Claude data paths
    claude_paths: Vec<PathBuf>,
    /// Whether to show progress bars
    show_progress: bool,
    /// Whether to use string interning for memory optimization
    use_interning: bool,
    /// Whether to use arena allocation for parsing
    use_arena: bool,
}

impl DataLoader {
    /// Create a new DataLoader by discovering Claude paths
    ///
    /// This method automatically searches for Claude data directories
    /// in platform-specific locations.
    ///
    /// # Errors
    ///
    /// Returns an error if no Claude data directories are found
    pub async fn new() -> Result<Self> {
        let paths = Self::discover_claude_paths().await?;
        if paths.is_empty() {
            return Err(CcstatError::NoClaudeDirectory);
        }

        debug!("Discovered {} Claude data directories", paths.len());
        Ok(Self {
            claude_paths: paths,
            show_progress: false,
            use_interning: false,
            use_arena: false,
        })
    }

    /// Discover Claude data directories on the system
    async fn discover_claude_paths() -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        // Check ~/.claude first (common location for Claude Code)
        if let Some(home) = dirs::home_dir() {
            let claude_path = home.join(".claude");
            if claude_path.exists() {
                paths.push(claude_path);
            }
        }

        // Platform-specific path discovery
        #[cfg(target_os = "macos")]
        {
            // macOS paths
            if let Some(home) = dirs::home_dir() {
                let claude_path = home.join("Library/Application Support/Claude");
                if claude_path.exists() {
                    paths.push(claude_path);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Linux paths
            if let Some(config_dir) = dirs::config_dir() {
                let claude_path = config_dir.join("Claude");
                if claude_path.exists() {
                    paths.push(claude_path);
                }
            }

            if let Some(home) = dirs::home_dir() {
                let claude_path = home.join(".config/Claude");
                if claude_path.exists() {
                    paths.push(claude_path);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Windows paths
            if let Some(app_data) = dirs::data_dir() {
                let claude_path = app_data.join("Claude");
                if claude_path.exists() {
                    paths.push(claude_path);
                }
            }
        }

        // Check environment variable override
        if let Ok(custom_path) = std::env::var("CLAUDE_DATA_PATH") {
            let path = PathBuf::from(custom_path);
            if path.exists() {
                paths.push(path);
            }
        }

        Ok(paths)
    }

    /// Helper to find JSONL files with optional filtering
    ///
    /// This internal method handles the common logic for finding JSONL files,
    /// with an optional filter function for additional criteria like modification time.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional filter closure to apply additional criteria to files
    ///
    /// # Returns
    ///
    /// A vector of paths to JSONL files that match the filter criteria
    async fn find_jsonl_files_with_filter<F>(&self, filter: F) -> Result<Vec<PathBuf>>
    where
        F: Fn(&std::path::Path) -> bool + Send + Sync + 'static + Clone,
    {
        let mut jsonl_files = Vec::new();

        for base_path in &self.claude_paths {
            let path_clone = base_path.clone();
            let filter_clone = filter.clone();
            let files = tokio::task::spawn_blocking(move || {
                use walkdir::WalkDir;
                let mut files = Vec::new();

                for entry in WalkDir::new(path_clone).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("jsonl")
                        && filter_clone(path)
                    {
                        files.push(path.to_path_buf());
                    }
                }
                files
            })
            .await
            .map_err(|e| CcstatError::Io(std::io::Error::other(e.to_string())))?;

            jsonl_files.extend(files);
        }

        Ok(jsonl_files)
    }

    /// Find all JSONL files in the discovered directories
    ///
    /// Recursively searches for `.jsonl` files in all discovered Claude directories.
    ///
    /// # Returns
    ///
    /// A vector of paths to JSONL files found
    pub async fn find_jsonl_files(&self) -> Result<Vec<PathBuf>> {
        let files = self.find_jsonl_files_with_filter(|_| true).await?;
        info!("Found {} JSONL files to process", files.len());
        Ok(files)
    }

    /// Find JSONL files modified since a given date
    ///
    /// This is useful for performance optimization when you only need recent data,
    /// such as for statusline generation which only needs today's data.
    ///
    /// # Arguments
    ///
    /// * `since` - Only include files modified after this time
    ///
    /// # Returns
    ///
    /// A vector of paths to JSONL files modified since the given date
    pub async fn find_recent_jsonl_files(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<PathBuf>> {
        let since_std = std::time::SystemTime::from(since);

        let files = self
            .find_jsonl_files_with_filter(move |path| {
                // Check modification time
                if let Ok(metadata) = path.metadata()
                    && let Ok(modified) = metadata.modified()
                {
                    modified >= since_std
                } else {
                    false
                }
            })
            .await?;

        info!(
            "Found {} recent JSONL files to process (since {})",
            files.len(),
            since
        );
        Ok(files)
    }

    /// Enable or disable progress bars
    pub fn with_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
    }

    /// Enable string interning for memory optimization
    pub fn with_interning(mut self, use_interning: bool) -> Self {
        self.use_interning = use_interning;
        self
    }

    /// Enable arena allocation for parsing
    pub fn with_arena(mut self, use_arena: bool) -> Self {
        self.use_arena = use_arena;
        self
    }

    /// Load usage entries in parallel for better performance
    ///
    /// This method uses Rayon to process multiple JSONL files concurrently,
    /// providing significant performance improvements for large datasets.
    ///
    /// # Returns
    ///
    /// An async stream of `Result<UsageEntry>` items processed in parallel
    pub fn load_usage_entries_parallel(&self) -> impl Stream<Item = Result<UsageEntry>> + '_ {
        async_stream::stream! {
            let files = match self.find_jsonl_files().await {
                Ok(files) => files,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let num_files = files.len();
            if num_files == 0 {
                return;
            }

            // Create progress bar if enabled
            let progress = if self.show_progress {
                let pb = ProgressBar::new(num_files as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} files")
                        .unwrap()
                        .progress_chars("#>-"),
                );
                pb.set_message("Loading usage data (parallel)");
                Some(Arc::new(pb))
            } else {
                None
            };

            // Use channel to collect results from parallel processing
            let (tx, mut rx) = mpsc::channel::<Result<Vec<UsageEntry>>>(num_files);

            // Shared deduplication set
            let seen_entries = Arc::new(Mutex::new(HashSet::new()));

            // Process files in parallel using Rayon
            let files_clone = files.clone();
            let progress_clone = progress.clone();
            let seen_entries_clone = seen_entries.clone();
            let use_interning = self.use_interning;
            let use_arena = self.use_arena;

            tokio::task::spawn_blocking(move || {
                files_clone.par_iter().for_each(|file_path| {
                    let tx = tx.clone();
                    if let Some(ref pb) = progress_clone {
                        pb.inc(1);
                    }

                    // Read file synchronously in the thread pool
                    let result = std::fs::read_to_string(file_path)
                        .map_err(CcstatError::Io)
                        .map(|content| {
                            let mut entries = Vec::new();
                            let mut local_duplicates = 0;

                            // Common logic for processing a line
                            let mut process_line = |line: &str| {
                                match serde_json::from_str::<RawJsonlEntry>(line) {
                                    Ok(raw_entry) => {
                                        // Check for deduplication key
                                        if let Some(dedup_key) = UsageEntry::dedup_key(&raw_entry) {
                                            let mut seen = seen_entries_clone.lock().unwrap();
                                            if seen.contains(&dedup_key) {
                                                local_duplicates += 1;
                                                trace!("Skipping duplicate entry with key: {}", dedup_key);
                                                return;
                                            }
                                            seen.insert(dedup_key);
                                        }

                                        if let Some(mut entry) = UsageEntry::from_raw(raw_entry) {
                                            if use_interning {
                                                // Apply string interning
                                                let interned_model = InternedModel::new(entry.model.as_str());
                                                entry.model = ModelName::new(interned_model.as_str());
                                                let interned_session = InternedSession::new(entry.session_id.as_str());
                                                entry.session_id = SessionId::new(interned_session.as_str());
                                            }
                                            entries.push(entry);
                                        }
                                    }
                                    Err(e) => {
                                        trace!("Skipping non-usage entry in {}: {}", file_path.display(), e);
                                    }
                                }
                            };

                            if use_arena {
                                // TODO: Consider processing in chunks to reduce memory usage for large files
                                // Currently collecting all lines into memory for arena allocation
                                let lines: Vec<String> = content.lines()
                                    .filter(|line| !line.trim().is_empty())
                                    .map(|s| s.to_string())
                                    .collect();

                                let pool = MemoryPool::new();
                                for line in &lines {
                                    let arena_line = pool.alloc_string(line);
                                    process_line(arena_line);
                                }
                            } else {
                                // Normal processing without arena
                                for line in content.lines() {
                                    if line.trim().is_empty() {
                                        continue;
                                    }
                                    process_line(line);
                                }
                            }

                            if local_duplicates > 0 {
                                debug!("Skipped {} duplicate entries in {}", local_duplicates, file_path.display());
                            }

                            entries
                        });

                    let _ = tx.blocking_send(result);
                });
            });

            // Yield all results
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(entries) => {
                        for entry in entries {
                            yield Ok(entry);
                        }
                    }
                    Err(e) => yield Err(e),
                }
            }

            // Log deduplication stats
            let final_seen_count = seen_entries.lock().unwrap().len();
            if final_seen_count > 0 {
                info!("Processed {} unique entries after deduplication", final_seen_count);
            }

            if let Some(pb) = progress {
                pb.finish_with_message("Loading complete (parallel)");
            }
        }
    }

    /// Helper method to process JSONL files as a stream
    ///
    /// This internal method handles the common logic for loading and processing
    /// JSONL files, used by both load_usage_entries and load_recent_usage_entries.
    ///
    /// # Arguments
    ///
    /// * `files` - Vector of paths to JSONL files to process
    /// * `progress_message` - Message to display in the progress bar
    ///
    /// # Returns
    ///
    /// An async stream of `Result<UsageEntry>` items
    fn process_jsonl_files(
        &self,
        files: Vec<PathBuf>,
        progress_message: &str,
    ) -> impl Stream<Item = Result<UsageEntry>> + '_ {
        let progress_msg = progress_message.to_string();
        async_stream::stream! {
            // Create progress bar if enabled
            let progress = if self.show_progress {
                let pb = ProgressBar::new(files.len() as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} files")
                        .unwrap()
                        .progress_chars("#>-"),
                );
                pb.set_message(progress_msg.clone());
                Some(pb)
            } else {
                None
            };

            // Deduplication set
            let mut seen_entries = HashSet::new();
            let mut total_duplicates = 0;

            for (idx, file_path) in files.into_iter().enumerate() {
                if let Some(ref pb) = progress {
                    pb.set_position(idx as u64);
                }

                let entries = self.parse_jsonl_stream(file_path, progress.as_ref(), &mut seen_entries);
                tokio::pin!(entries);
                while let Some(result) = entries.next().await {
                    match &result {
                        Ok(_) => yield result,
                        Err(e) => {
                            if let CcstatError::DuplicateEntry = e {
                                total_duplicates += 1;
                            } else {
                                yield result;
                            }
                        }
                    }
                }
            }

            if total_duplicates > 0 {
                info!("Skipped {} duplicate entries", total_duplicates);
            }

            if let Some(pb) = progress {
                pb.finish_with_message("Loading complete");
            }
        }
    }

    /// Load recent usage entries as an async stream
    ///
    /// This method provides a stream of usage entries parsed from JSONL files
    /// modified since the given date. It's optimized for scenarios where you
    /// only need recent data, such as statusline generation.
    ///
    /// # Arguments
    ///
    /// * `since` - Only load entries from files modified after this time
    ///
    /// # Returns
    ///
    /// An async stream of `Result<UsageEntry>` items from recent files
    pub fn load_recent_usage_entries(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> impl Stream<Item = Result<UsageEntry>> + '_ {
        async_stream::stream! {
            let files = match self.find_recent_jsonl_files(since).await {
                Ok(files) => files,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            if files.is_empty() {
                debug!("No recent files found since {}", since);
                return;
            }

            let entries = self.process_jsonl_files(files, "Loading recent data");
            tokio::pin!(entries);
            while let Some(result) = entries.next().await {
                yield result;
            }
        }
    }

    /// Parse a single JSONL file as a stream
    fn parse_jsonl_stream<'a>(
        &'a self,
        path: PathBuf,
        _progress: Option<&'a ProgressBar>,
        seen_entries: &'a mut HashSet<String>,
    ) -> impl Stream<Item = Result<UsageEntry>> + 'a {
        async_stream::stream! {
            // Get file size for progress tracking
            let _file_size = match tokio::fs::metadata(&path).await {
                Ok(metadata) => metadata.len(),
                Err(_) => 0,
            };

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    yield Err(e.into());
                    return;
                }
            };

            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut line_number = 0;
            let mut file_duplicates = 0;

            while let Ok(Some(line)) = lines.next_line().await {
                line_number += 1;

                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<RawJsonlEntry>(&line) {
                    Ok(raw_entry) => {
                        // Check for deduplication key
                        if let Some(dedup_key) = UsageEntry::dedup_key(&raw_entry) {
                            if seen_entries.contains(&dedup_key) {
                                file_duplicates += 1;
                                trace!("Skipping duplicate entry with key: {}", dedup_key);
                                yield Err(CcstatError::DuplicateEntry);
                                continue;
                            }
                            seen_entries.insert(dedup_key);
                        }

                        if let Some(entry) = self.convert_entry(raw_entry) {
                            yield Ok(entry);
                        }
                        // Skip non-assistant entries silently
                    },
                    Err(e) => {
                        trace!(
                            "Skipping non-usage entry at line {} in {}: {}",
                            line_number,
                            path.display(),
                            e
                        );
                        // Continue processing other lines
                    }
                }
            }

            if file_duplicates > 0 {
                debug!("Skipped {} duplicate entries in {}", file_duplicates, path.display());
            }
        }
    }

    /// Get the discovered Claude paths
    ///
    /// Returns a slice of all discovered Claude data directories.
    /// Useful for debugging or displaying where data is being loaded from.
    pub fn paths(&self) -> &[PathBuf] {
        &self.claude_paths
    }

    /// Convert raw entry to UsageEntry with optional string interning
    fn convert_entry(&self, raw: RawJsonlEntry) -> Option<UsageEntry> {
        UsageEntry::from_raw(raw).map(|mut entry| {
            if self.use_interning {
                // Intern the model name
                let interned_model = InternedModel::new(entry.model.as_str());
                entry.model = ModelName::new(interned_model.as_str());

                // Intern the session ID
                let interned_session = InternedSession::new(entry.session_id.as_str());
                entry.session_id = SessionId::new(interned_session.as_str());
            }
            entry
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{ENV_MUTEX, EnvVarGuard};
    use tempfile::TempDir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_jsonl_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(br#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5}},"cwd":"/home/user/project-a"}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        file.write_all(br#"{"sessionId":"test2","timestamp":"2024-01-01T01:00:00Z","type":"assistant","message":{"model":"claude-3-sonnet","usage":{"input_tokens":200,"output_tokens":100,"cache_creation_input_tokens":20,"cache_read_input_tokens":10}}}"#).await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let entry1 = stream.next().await.unwrap().unwrap();
        assert_eq!(entry1.session_id.as_str(), "test1");
        assert_eq!(entry1.tokens.input_tokens, 100);
        assert_eq!(entry1.project, Some("project-a".to_string()));

        let entry2 = stream.next().await.unwrap().unwrap();
        assert_eq!(entry2.session_id.as_str(), "test2");
        assert_eq!(entry2.tokens.input_tokens, 200);
        assert_eq!(entry2.project, None);
    }

    #[tokio::test]
    async fn test_parallel_loading() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple JSONL files
        for i in 0..3 {
            let content = format!(
                r#"{{"sessionId":"test{i}","timestamp":"2024-01-01T0{i}:00:00Z","type":"assistant","message":{{"model":"claude-3-opus","usage":{{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":5}}}},"cost_usd":0.1}}"#
            );
            let file_path = temp_dir.path().join(format!("test{i}.jsonl"));
            let mut file = tokio::fs::File::create(&file_path).await.unwrap();
            file.write_all(content.as_bytes()).await.unwrap();
        }

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        // Test parallel loading
        let entries: Vec<_> = loader
            .load_usage_entries_parallel()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(entries.len(), 3);

        // Verify all entries are loaded (order may vary due to parallel processing)
        let session_ids: Vec<_> = entries.iter().map(|e| e.session_id.as_str()).collect();
        assert!(session_ids.contains(&"test0"));
        assert!(session_ids.contains(&"test1"));
        assert!(session_ids.contains(&"test2"));
    }

    #[tokio::test]
    async fn test_discover_claude_paths_with_env_override() {
        let _lock = ENV_MUTEX.lock().await;

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().to_path_buf();

        // Use RAII guard for safe environment variable manipulation
        let mut env_guard = EnvVarGuard::new();
        env_guard.set("CLAUDE_DATA_PATH", custom_path.to_str().unwrap());

        let paths = DataLoader::discover_claude_paths().await.unwrap();
        assert!(paths.contains(&custom_path));

        // Environment variables will be automatically restored when env_guard drops
    }

    #[tokio::test]
    async fn test_find_jsonl_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create a subdirectory with JSONL files
        let sub_dir = temp_dir.path().join("subdir");
        tokio::fs::create_dir(&sub_dir).await.unwrap();

        // Create JSONL files at different levels
        tokio::fs::write(temp_dir.path().join("test1.jsonl"), "")
            .await
            .unwrap();
        tokio::fs::write(sub_dir.join("test2.jsonl"), "")
            .await
            .unwrap();
        tokio::fs::write(temp_dir.path().join("not_jsonl.txt"), "")
            .await
            .unwrap();

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        let files = loader.find_jsonl_files().await.unwrap();
        assert_eq!(files.len(), 2);

        // Check that only JSONL files are found
        for file in &files {
            assert_eq!(file.extension().and_then(|s| s.to_str()), Some("jsonl"));
        }
    }

    #[tokio::test]
    async fn test_find_recent_jsonl_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create old and new JSONL files
        let old_file = temp_dir.path().join("old.jsonl");
        let new_file = temp_dir.path().join("new.jsonl");

        tokio::fs::write(&old_file, "").await.unwrap();
        tokio::fs::write(&new_file, "").await.unwrap();

        // Set the old file's modification time to 2 days ago
        let two_days_ago = chrono::Utc::now() - chrono::Duration::days(2);
        let old_time =
            filetime::FileTime::from_system_time(std::time::SystemTime::from(two_days_ago));
        filetime::set_file_mtime(&old_file, old_time).unwrap();

        // For test purposes, we'll use the current time minus 1 hour as the filter
        let one_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        // This should find the new file but not the old one
        let files = loader.find_recent_jsonl_files(one_hour_ago).await.unwrap();
        assert_eq!(files.len(), 1); // Only the new file should be found
    }

    #[tokio::test]
    async fn test_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        // Create a file with duplicate entries
        let duplicate_content = r#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"id":"msg_123","model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"requestId":"req_456"}"#;

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(duplicate_content.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        file.write_all(duplicate_content.as_bytes()).await.unwrap(); // Same entry again
        file.write_all(b"\n").await.unwrap();
        file.write_all(br#"{"sessionId":"test2","timestamp":"2024-01-01T01:00:00Z","type":"assistant","message":{"id":"msg_789","model":"claude-3-opus","usage":{"input_tokens":200,"output_tokens":100}},"requestId":"req_012"}"#).await.unwrap();

        // Run the test in two parts: first collect entries, then check deduplication separately
        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path.clone(), None, &mut seen);
        tokio::pin!(stream);

        let mut entries = Vec::new();
        let mut error_count = 0;
        while let Some(result) = stream.next().await {
            match result {
                Ok(entry) => entries.push(entry),
                Err(CcstatError::DuplicateEntry) => error_count += 1,
                Err(e) => panic!("Unexpected error in stream: {:?}", e),
            }
        }

        // Should have only 2 unique entries (test1 appears twice, so 1 duplicate error)
        assert_eq!(entries.len(), 2);
        assert_eq!(error_count, 1); // One duplicate was found

        // Verify the deduplication worked by checking unique session IDs
        let session_ids: HashSet<_> = entries.iter().map(|e| e.session_id.as_str()).collect();
        assert_eq!(session_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_empty_file_handling() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("empty.jsonl");

        // Create an empty file
        tokio::fs::write(&jsonl_path, "").await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_malformed_json_handling() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("malformed.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(b"not valid json\n").await.unwrap();
        file.write_all(br#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();
        file.write_all(b"\n{broken json").await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let mut valid_entries = Vec::new();
        while let Some(result) = stream.next().await {
            if let Ok(entry) = result {
                valid_entries.push(entry);
            }
        }

        // Should have parsed only the valid entry
        assert_eq!(valid_entries.len(), 1);
        assert_eq!(valid_entries[0].session_id.as_str(), "test1");
    }

    #[tokio::test]
    async fn test_non_assistant_entries_filtered() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("mixed.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        // User type entry - should be filtered
        file.write_all(br#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"user","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        // Assistant type entry - should be included
        file.write_all(br#"{"sessionId":"test2","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        // System type entry - should be filtered
        file.write_all(br#"{"sessionId":"test3","timestamp":"2024-01-01T00:00:00Z","type":"system","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let mut entries = Vec::new();
        while let Some(result) = stream.next().await {
            if let Ok(entry) = result {
                entries.push(entry);
            }
        }

        // Should only have the assistant entry
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id.as_str(), "test2");
    }

    #[tokio::test]
    async fn test_with_progress_flag() {
        let temp_dir = TempDir::new().unwrap();

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        let loader_with_progress = loader.with_progress(true);
        assert!(loader_with_progress.show_progress);

        let loader_without_progress = loader_with_progress.with_progress(false);
        assert!(!loader_without_progress.show_progress);
    }

    #[tokio::test]
    async fn test_with_interning_flag() {
        let temp_dir = TempDir::new().unwrap();

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        let loader_with_interning = loader.with_interning(true);
        assert!(loader_with_interning.use_interning);
    }

    #[tokio::test]
    async fn test_with_arena_flag() {
        let temp_dir = TempDir::new().unwrap();

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        let loader_with_arena = loader.with_arena(true);
        assert!(loader_with_arena.use_arena);
    }

    #[tokio::test]
    async fn test_string_interning_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        // Create test data with repeated model names and session IDs
        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(br#"{"sessionId":"test-session","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        file.write_all(br#"{"sessionId":"test-session","timestamp":"2024-01-01T01:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":200,"output_tokens":100}}}"#).await.unwrap();

        // Test with interning enabled
        let loader_with_interning = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: true,
            use_arena: false,
        };

        let entries: Vec<_> = loader_with_interning
            .load_usage_entries_parallel()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(entries.len(), 2);
        // Both entries should have the same model and session ID
        assert_eq!(entries[0].model.as_str(), "claude-3-opus");
        assert_eq!(entries[1].model.as_str(), "claude-3-opus");
        assert_eq!(entries[0].session_id.as_str(), "test-session");
        assert_eq!(entries[1].session_id.as_str(), "test-session");
    }

    #[tokio::test]
    async fn test_arena_allocation_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        // Create test data
        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(br#"{"sessionId":"arena-test","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-sonnet","usage":{"input_tokens":150,"output_tokens":75}}}"#).await.unwrap();

        // Test with arena allocation enabled
        let loader_with_arena = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: true,
        };

        let entries: Vec<_> = loader_with_arena
            .load_usage_entries_parallel()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id.as_str(), "arena-test");
        assert_eq!(entries[0].model.as_str(), "claude-3-sonnet");
        assert_eq!(entries[0].tokens.input_tokens, 150);
    }

    #[tokio::test]
    async fn test_interning_and_arena_together() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        // Create test data with repeated values
        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        for i in 0..3 {
            let line = format!(
                r#"{{"sessionId":"combined-test","timestamp":"2024-01-01T0{}:00:00Z","type":"assistant","message":{{"model":"claude-3-opus","usage":{{"input_tokens":{},"output_tokens":50}}}}}}"#,
                i,
                (i + 1) * 100
            );
            file.write_all(line.as_bytes()).await.unwrap();
            file.write_all(b"\n").await.unwrap();
        }

        // Test with both interning and arena enabled
        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: true,
            use_arena: true,
        };

        let entries: Vec<_> = loader
            .load_usage_entries_parallel()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(entries.len(), 3);
        // All entries should have the same interned values
        for entry in &entries {
            assert_eq!(entry.model.as_str(), "claude-3-opus");
            assert_eq!(entry.session_id.as_str(), "combined-test");
        }
    }

    #[tokio::test]
    async fn test_get_paths() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let loader = DataLoader {
            claude_paths: vec![path.clone()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        let paths = loader.paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], path);
    }

    #[tokio::test]
    async fn test_empty_lines_ignored() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("with_empty.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(b"\n\n").await.unwrap(); // Empty lines
        file.write_all(br#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();
        file.write_all(b"\n\n\n").await.unwrap(); // More empty lines
        file.write_all(br#"{"sessionId":"test2","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":200,"output_tokens":100}}}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let mut entries = Vec::new();
        while let Some(result) = stream.next().await {
            if let Ok(entry) = result {
                entries.push(entry);
            }
        }

        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_api_error_messages_filtered() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("with_errors.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        // Entry with API error flag - should be filtered
        file.write_all(br#"{"sessionId":"test1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}},"isApiErrorMessage":true}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        // Normal entry - should be included
        file.write_all(br#"{"sessionId":"test2","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };
        let mut seen = HashSet::new();
        let stream = loader.parse_jsonl_stream(jsonl_path, None, &mut seen);
        tokio::pin!(stream);

        let mut entries = Vec::new();
        while let Some(result) = stream.next().await {
            if let Ok(entry) = result {
                entries.push(entry);
            }
        }

        // Should only have the non-error entry
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id.as_str(), "test2");
    }

    #[tokio::test]
    async fn test_load_recent_usage_entries() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("recent.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(br#"{"sessionId":"recent1","timestamp":"2024-01-01T00:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#).await.unwrap();

        let loader = DataLoader {
            claude_paths: vec![temp_dir.path().to_path_buf()],
            show_progress: false,
            use_interning: false,
            use_arena: false,
        };

        // Load entries from files modified in the last hour
        let one_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
        let entries: Vec<_> = loader
            .load_recent_usage_entries(one_hour_ago)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id.as_str(), "recent1");
    }

    #[tokio::test]
    async fn test_no_claude_directory_error() {
        let _lock = ENV_MUTEX.lock().await;

        // Use RAII guard for safe environment variable manipulation
        let mut env_guard = EnvVarGuard::new();
        env_guard.set("HOME", "/nonexistent");
        env_guard.remove("CLAUDE_DATA_PATH");

        let result = DataLoader::new().await;
        assert!(result.is_err());

        // Environment variables will be automatically restored when env_guard drops
    }
}
