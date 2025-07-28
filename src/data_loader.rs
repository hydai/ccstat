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
//! use ccusage::data_loader::DataLoader;
//! use futures::StreamExt;
//!
//! # async fn example() -> ccusage::Result<()> {
//! let data_loader = DataLoader::new().await?;
//!
//! // Stream usage entries
//! let entries = data_loader.load_usage_entries();
//! tokio::pin!(entries);
//! while let Some(result) = entries.next().await {
//!     let entry = result?;
//!     println!("Session: {}, Tokens: {}", entry.session_id, entry.tokens.total());
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{CcusageError, Result};
use crate::types::UsageEntry;
use futures::stream::Stream;
use futures::StreamExt;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};

/// Data loader for discovering and streaming JSONL files
///
/// The DataLoader is responsible for finding Claude usage data files on the
/// system and providing efficient streaming access to parse them.
pub struct DataLoader {
    /// Discovered Claude data paths
    claude_paths: Vec<PathBuf>,
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
            return Err(CcusageError::NoClaudeDirectory);
        }

        debug!("Discovered {} Claude data directories", paths.len());
        Ok(Self {
            claude_paths: paths,
        })
    }

    /// Discover Claude data directories on the system
    async fn discover_claude_paths() -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

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

    /// Find all JSONL files in the discovered directories
    ///
    /// Recursively searches for `.jsonl` files in all discovered Claude directories.
    ///
    /// # Returns
    ///
    /// A vector of paths to JSONL files found
    pub async fn find_jsonl_files(&self) -> Result<Vec<PathBuf>> {
        let mut jsonl_files = Vec::new();

        for base_path in &self.claude_paths {
            let _pattern = base_path.join("**/*.jsonl");

            // Walk directory recursively
            if let Ok(mut entries) = tokio::fs::read_dir(base_path).await {
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        jsonl_files.push(path);
                    }
                }
            }
        }

        debug!("Found {} JSONL files", jsonl_files.len());
        Ok(jsonl_files)
    }

    /// Load usage entries as an async stream
    ///
    /// This method provides a stream of usage entries parsed from all discovered
    /// JSONL files. It handles large files efficiently by streaming rather than
    /// loading everything into memory.
    ///
    /// # Returns
    ///
    /// An async stream of `Result<UsageEntry>` items
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ccusage::data_loader::DataLoader;
    /// # use futures::StreamExt;
    /// # async fn example() -> ccusage::Result<()> {
    /// let loader = DataLoader::new().await?;
    /// let entries = loader.load_usage_entries();
    /// tokio::pin!(entries);
    /// 
    /// while let Some(entry) = entries.next().await {
    ///     match entry {
    ///         Ok(usage) => println!("Loaded entry for session {}", usage.session_id),
    ///         Err(e) => eprintln!("Error loading entry: {}", e),
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_usage_entries(&self) -> impl Stream<Item = Result<UsageEntry>> + '_ {
        async_stream::stream! {
            let files = match self.find_jsonl_files().await {
                Ok(files) => files,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            for file_path in files {
                let entries = Self::parse_jsonl_stream(file_path);
                tokio::pin!(entries);
                while let Some(result) = entries.next().await {
                    yield result;
                }
            }
        }
    }

    /// Parse a single JSONL file as a stream
    fn parse_jsonl_stream(path: PathBuf) -> impl Stream<Item = Result<UsageEntry>> {
        async_stream::stream! {
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

            while let Ok(Some(line)) = lines.next_line().await {
                line_number += 1;

                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<UsageEntry>(&line) {
                    Ok(entry) => yield Ok(entry),
                    Err(e) => {
                        warn!(
                            "Failed to parse line {} in {}: {}",
                            line_number,
                            path.display(),
                            e
                        );
                        // Continue processing other lines
                    }
                }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_jsonl_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let jsonl_path = temp_dir.path().join("test.jsonl");

        let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
        file.write_all(br#"{"session_id":"test1","timestamp":"2024-01-01T00:00:00Z","model":"claude-3-opus","input_tokens":100,"output_tokens":50,"cache_creation_tokens":10,"cache_read_tokens":5,"project":"project-a"}"#).await.unwrap();
        file.write_all(b"\n").await.unwrap();
        file.write_all(br#"{"session_id":"test2","timestamp":"2024-01-01T01:00:00Z","model":"claude-3-sonnet","input_tokens":200,"output_tokens":100,"cache_creation_tokens":20,"cache_read_tokens":10}"#).await.unwrap();

        let stream = DataLoader::parse_jsonl_stream(jsonl_path);
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
}
