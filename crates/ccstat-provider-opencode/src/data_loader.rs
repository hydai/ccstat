//! OpenCode data loader
//!
//! Discovers and parses OpenCode per-message JSON files from
//! `~/.local/share/opencode/storage/message/`.

use async_trait::async_trait;
use ccstat_core::error::{CcstatError, Result};
use ccstat_core::provider::ProviderDataLoader;
use ccstat_core::types::{ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry};
use chrono::{TimeZone, Utc};
use futures::stream::Stream;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{debug, warn};

/// Data loader for OpenCode usage data.
pub struct DataLoader {
    message_dir: PathBuf,
}

#[async_trait]
impl ProviderDataLoader for DataLoader {
    async fn new() -> Result<Self> {
        let base = if let Ok(data_dir) = std::env::var("OPENCODE_DATA_DIR") {
            PathBuf::from(data_dir)
        } else {
            dirs::data_dir()
                .ok_or_else(|| CcstatError::Config("Cannot determine data directory".into()))?
                .join("opencode")
        };

        let message_dir = base.join("storage").join("message");
        if !message_dir.exists() {
            debug!(
                "OpenCode message directory not found: {}",
                message_dir.display()
            );
        }

        Ok(DataLoader { message_dir })
    }

    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>> {
        Box::pin(async_stream::try_stream! {
            if !self.message_dir.exists() {
                return;
            }

            let mut json_files = Vec::new();
            for entry in walkdir::WalkDir::new(&self.message_dir)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                if path.extension().is_some_and(|ext| ext == "json") {
                    json_files.push(path);
                }
            }

            debug!("Found {} OpenCode message files", json_files.len());

            let mut seen_ids = HashSet::new();

            for path in json_files {
                let content = match tokio::fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read OpenCode message file {}: {}", path.display(), e);
                        continue;
                    }
                };

                let msg: OpenCodeMessage = match serde_json::from_str(&content) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to parse OpenCode message {}: {}", path.display(), e);
                        continue;
                    }
                };

                // Dedup by message id
                if !seen_ids.insert(msg.id.clone()) {
                    continue;
                }

                if let Some(entry) = convert_message(msg) {
                    yield entry;
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Message schema
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeMessage {
    id: String,
    #[serde(rename = "sessionID")]
    session_id: String,
    #[serde(rename = "modelID")]
    model_id: String,
    time: Option<MessageTime>,
    tokens: Option<MessageTokens>,
    cost: Option<f64>,
}

#[derive(Deserialize)]
struct MessageTime {
    created: Option<f64>,
}

#[derive(Deserialize)]
struct MessageTokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache: Option<CacheTokens>,
}

#[derive(Deserialize)]
struct CacheTokens {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    write: u64,
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn convert_message(msg: OpenCodeMessage) -> Option<UsageEntry> {
    let tokens = msg.tokens.as_ref()?;

    // Skip zero-token messages
    if tokens.input == 0 && tokens.output == 0 {
        return None;
    }

    let timestamp = msg
        .time
        .as_ref()
        .and_then(|t| t.created)
        .and_then(|ts| {
            let secs = ts as i64;
            let nanos = ((ts - secs as f64) * 1_000_000_000.0) as u32;
            Utc.timestamp_opt(secs, nanos).single()
        })
        .unwrap_or_else(Utc::now);

    let (cache_read, cache_write) = match &tokens.cache {
        Some(c) => (c.read, c.write),
        None => (0, 0),
    };

    let model_name = normalize_model(&msg.model_id);

    Some(UsageEntry {
        session_id: SessionId::new(msg.session_id),
        timestamp: ISOTimestamp::new(timestamp),
        model: ModelName::new(model_name),
        tokens: TokenCounts::new(tokens.input, tokens.output, cache_write, cache_read),
        total_cost: msg.cost,
        project: None,
        instance_id: None,
    })
}

/// Normalize OpenCode model names.
fn normalize_model(model: &str) -> String {
    if model == "gemini-3-pro-high" {
        "gemini-3-pro-preview".to_string()
    } else {
        model.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::TempDir;

    fn make_message_json(
        id: &str,
        session_id: &str,
        model: &str,
        created: f64,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        cost: Option<f64>,
    ) -> String {
        let cost_json = match cost {
            Some(c) => format!("{}", c),
            None => "null".to_string(),
        };
        format!(
            r#"{{"id":"{}","sessionID":"{}","modelID":"{}","time":{{"created":{}}},"tokens":{{"input":{},"output":{},"cache":{{"read":{},"write":{}}}}},"cost":{}}}"#,
            id, session_id, model, created, input, output, cache_read, cache_write, cost_json
        )
    }

    #[tokio::test]
    async fn test_parse_message() {
        let dir = TempDir::new().unwrap();
        let msg_dir = dir.path().join("storage").join("message");
        std::fs::create_dir_all(&msg_dir).unwrap();

        let msg_file = msg_dir.join("msg1.json");
        let mut f = std::fs::File::create(&msg_file).unwrap();
        write!(
            f,
            "{}",
            make_message_json(
                "msg1",
                "sess1",
                "claude-sonnet-4",
                1735689600.0,
                100,
                50,
                10,
                5,
                Some(0.01)
            )
        )
        .unwrap();

        let loader = DataLoader {
            message_dir: msg_dir,
        };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert_eq!(entries.len(), 1);
        let entry = entries[0].as_ref().unwrap();
        assert_eq!(entry.tokens.input_tokens, 100);
        assert_eq!(entry.tokens.output_tokens, 50);
        assert_eq!(entry.tokens.cache_read_tokens, 10);
        assert_eq!(entry.tokens.cache_creation_tokens, 5);
        assert_eq!(entry.total_cost, Some(0.01));
    }

    #[tokio::test]
    async fn test_dedup_by_id() {
        let dir = TempDir::new().unwrap();
        let msg_dir = dir.path().join("storage").join("message");
        std::fs::create_dir_all(&msg_dir).unwrap();

        // Two files with same message id
        for i in 0..2 {
            let msg_file = msg_dir.join(format!("msg_dup_{}.json", i));
            let mut f = std::fs::File::create(&msg_file).unwrap();
            write!(
                f,
                "{}",
                make_message_json(
                    "same-id",
                    "sess1",
                    "gpt-5",
                    1735689600.0,
                    100,
                    50,
                    0,
                    0,
                    None
                )
            )
            .unwrap();
        }

        let loader = DataLoader {
            message_dir: msg_dir,
        };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        // Should be deduped to 1
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn test_model_alias() {
        assert_eq!(normalize_model("gemini-3-pro-high"), "gemini-3-pro-preview");
        assert_eq!(normalize_model("claude-sonnet-4"), "claude-sonnet-4");
    }

    #[tokio::test]
    async fn test_skip_zero_tokens() {
        let msg = OpenCodeMessage {
            id: "zero".to_string(),
            session_id: "s1".to_string(),
            model_id: "gpt-5".to_string(),
            time: Some(MessageTime {
                created: Some(1735689600.0),
            }),
            tokens: Some(MessageTokens {
                input: 0,
                output: 0,
                cache: None,
            }),
            cost: None,
        };
        assert!(convert_message(msg).is_none());
    }

    #[tokio::test]
    async fn test_no_dir() {
        let loader = DataLoader {
            message_dir: PathBuf::from("/tmp/nonexistent-opencode-dir"),
        };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert!(entries.is_empty());
    }
}
