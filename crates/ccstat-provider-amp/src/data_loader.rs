//! Amp data loader
//!
//! Discovers and parses Amp thread JSON files from
//! `~/.local/share/amp/threads/`. Extracts tokens from usageLedger
//! events with cache breakdown from messages.

use async_trait::async_trait;
use ccstat_core::error::{CcstatError, Result};
use ccstat_core::provider::ProviderDataLoader;
use ccstat_core::types::{ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry};
use chrono::DateTime;
use futures::stream::Stream;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{debug, warn};

/// Data loader for Amp usage data.
pub struct DataLoader {
    threads_dir: PathBuf,
}

#[async_trait]
impl ProviderDataLoader for DataLoader {
    async fn new() -> Result<Self> {
        let base = if let Ok(data_dir) = std::env::var("AMP_DATA_DIR") {
            PathBuf::from(data_dir)
        } else {
            dirs::data_dir()
                .ok_or_else(|| CcstatError::Config("Cannot determine data directory".into()))?
                .join("amp")
        };

        let threads_dir = base.join("threads");
        if !threads_dir.exists() {
            debug!("Amp threads directory not found: {}", threads_dir.display());
        }

        Ok(DataLoader { threads_dir })
    }

    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>> {
        Box::pin(async_stream::try_stream! {
            if !self.threads_dir.exists() {
                return;
            }

            let mut json_files = Vec::new();
            for entry in walkdir::WalkDir::new(&self.threads_dir)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                let is_thread_file = path
                    .extension()
                    .is_some_and(|ext| ext == "json")
                    && path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| name.starts_with("T-"));
                if is_thread_file {
                    json_files.push(path);
                }
            }

            debug!("Found {} Amp thread files", json_files.len());

            for path in json_files {
                let content = match tokio::fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read Amp thread file {}: {}", path.display(), e);
                        continue;
                    }
                };

                let thread: AmpThread = match serde_json::from_str(&content) {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("Failed to parse Amp thread {}: {}", path.display(), e);
                        continue;
                    }
                };

                let entries = extract_entries_from_thread(&thread);
                for entry in entries {
                    yield entry;
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Thread schema
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AmpThread {
    id: String,
    #[serde(default)]
    messages: Vec<AmpMessage>,
    #[serde(rename = "usageLedger")]
    usage_ledger: Option<UsageLedger>,
}

#[derive(Deserialize)]
struct AmpMessage {
    id: String,
    #[serde(default)]
    usage: Option<MessageUsage>,
}

#[derive(Deserialize)]
struct MessageUsage {
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

#[derive(Deserialize)]
struct UsageLedger {
    #[serde(default)]
    events: Vec<LedgerEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LedgerEvent {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    credits: Option<f64>,
    #[serde(default)]
    created_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

fn extract_entries_from_thread(thread: &AmpThread) -> Vec<UsageEntry> {
    let ledger = match &thread.usage_ledger {
        Some(l) => l,
        None => return Vec::new(),
    };

    // Build a lookup from message ID → cache tokens
    let cache_lookup: HashMap<&str, &MessageUsage> = thread
        .messages
        .iter()
        .filter_map(|m| m.usage.as_ref().map(|u| (m.id.as_str(), u)))
        .collect();

    let mut entries = Vec::new();

    for event in &ledger.events {
        // Skip zero-token events
        if event.input_tokens == 0 && event.output_tokens == 0 {
            continue;
        }

        let timestamp = event
            .created_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| ISOTimestamp::new(dt.to_utc()));

        let Some(timestamp) = timestamp else {
            warn!(
                "Skipping Amp ledger event with invalid timestamp in thread {}",
                thread.id
            );
            continue;
        };

        let model = event.model.as_deref().unwrap_or("unknown").to_string();

        // Get cache breakdown from messages array
        let (cache_creation, cache_read) = event
            .message_id
            .as_deref()
            .and_then(|mid| cache_lookup.get(mid))
            .map(|u| (u.cache_creation_input_tokens, u.cache_read_input_tokens))
            .unwrap_or((0, 0));

        entries.push(UsageEntry {
            session_id: SessionId::new(thread.id.clone()),
            timestamp,
            model: ModelName::new(model),
            tokens: TokenCounts::new(
                event.input_tokens,
                event.output_tokens,
                cache_creation,
                cache_read,
            ),
            total_cost: event.credits,
            project: None,
            instance_id: None,
        });
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_thread_json(
        id: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_create: u64,
        cache_read: u64,
        credits: f64,
    ) -> String {
        format!(
            r#"{{
  "id": "{}",
  "messages": [
    {{
      "id": "msg-1",
      "type": "assistant",
      "created_at": "2025-01-01T10:00:00Z",
      "model": "{}",
      "usage": {{
        "input_tokens": {},
        "output_tokens": {},
        "cache_creation_input_tokens": {},
        "cache_read_input_tokens": {}
      }}
    }}
  ],
  "usageLedger": {{
    "events": [
      {{
        "messageId": "msg-1",
        "model": "{}",
        "inputTokens": {},
        "outputTokens": {},
        "totalTokens": {},
        "credits": {},
        "createdAt": "2025-01-01T10:00:00Z"
      }}
    ]
  }}
}}"#,
            id,
            model,
            input,
            output,
            cache_create,
            cache_read,
            model,
            input,
            output,
            input + output,
            credits
        )
    }

    #[tokio::test]
    async fn test_parse_thread() {
        let dir = TempDir::new().unwrap();
        let threads_dir = dir.path().join("threads");
        std::fs::create_dir_all(&threads_dir).unwrap();

        let thread_file = threads_dir.join("T-abc123.json");
        let mut f = std::fs::File::create(&thread_file).unwrap();
        write!(
            f,
            "{}",
            make_thread_json("T-abc123", "claude-sonnet-4", 500, 200, 50, 100, 0.05)
        )
        .unwrap();

        let loader = DataLoader { threads_dir };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert_eq!(entries.len(), 1);
        let entry = entries[0].as_ref().unwrap();
        assert_eq!(entry.tokens.input_tokens, 500);
        assert_eq!(entry.tokens.output_tokens, 200);
        assert_eq!(entry.tokens.cache_creation_tokens, 50);
        assert_eq!(entry.tokens.cache_read_tokens, 100);
        assert_eq!(entry.total_cost, Some(0.05));
        assert_eq!(entry.session_id.as_str(), "T-abc123");
    }

    #[tokio::test]
    async fn test_no_ledger() {
        let thread = AmpThread {
            id: "T-1".to_string(),
            messages: vec![],
            usage_ledger: None,
        };
        let entries = extract_entries_from_thread(&thread);
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_no_cache_match() {
        // Ledger event with no matching message → cache tokens should be 0
        let thread_json = r#"{
            "id": "T-2",
            "messages": [],
            "usageLedger": {
                "events": [{
                    "messageId": "nonexistent",
                    "model": "claude-sonnet-4",
                    "inputTokens": 100,
                    "outputTokens": 50,
                    "totalTokens": 150,
                    "credits": 0.01,
                    "createdAt": "2025-01-01T10:00:00Z"
                }]
            }
        }"#;
        let thread: AmpThread = serde_json::from_str(thread_json).unwrap();
        let entries = extract_entries_from_thread(&thread);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tokens.cache_creation_tokens, 0);
        assert_eq!(entries[0].tokens.cache_read_tokens, 0);
    }

    #[tokio::test]
    async fn test_no_dir() {
        let loader = DataLoader {
            threads_dir: PathBuf::from("/tmp/nonexistent-amp-dir"),
        };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert!(entries.is_empty());
    }
}
