//! Codex data loader
//!
//! Discovers and parses Codex session JSONL files from `~/.codex/sessions/`.
//! Codex uses cumulative token counts that must be converted to per-event deltas.

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
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};

/// Default model name when no turn_context provides one.
const FALLBACK_MODEL: &str = "gpt-5";

/// Data loader for Codex usage data.
pub struct DataLoader {
    session_dir: PathBuf,
}

#[async_trait]
impl ProviderDataLoader for DataLoader {
    async fn new() -> Result<Self> {
        let base = if let Ok(home) = std::env::var("CODEX_HOME") {
            PathBuf::from(home)
        } else {
            dirs::home_dir()
                .ok_or_else(|| CcstatError::Config("Cannot determine home directory".into()))?
                .join(".codex")
        };

        let session_dir = base.join("sessions");
        if !session_dir.exists() {
            debug!(
                "Codex sessions directory not found: {}",
                session_dir.display()
            );
        }

        Ok(DataLoader { session_dir })
    }

    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>> {
        Box::pin(async_stream::try_stream! {
            if !self.session_dir.exists() {
                return;
            }

            let mut jsonl_files = Vec::new();
            for entry in walkdir::WalkDir::new(&self.session_dir)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                if path.extension().is_some_and(|ext| ext == "jsonl") {
                    jsonl_files.push(path);
                }
            }

            debug!("Found {} Codex session files", jsonl_files.len());

            for path in jsonl_files {
                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let entries = parse_session_file(&path, &session_id).await;
                match entries {
                    Ok(parsed) => {
                        for entry in parsed {
                            yield entry;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Codex session {}: {}", session_id, e);
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// JSONL event types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodexEvent {
    #[serde(rename = "type")]
    event_type: String,
    timestamp: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    payload: Option<EventPayload>,
}

#[derive(Deserialize)]
struct EventPayload {
    #[serde(rename = "type")]
    payload_type: Option<String>,
    #[serde(default)]
    info: Option<TokenInfo>,
}

#[derive(Deserialize)]
struct TokenInfo {
    total_token_usage: Option<CumulativeTokens>,
    last_token_usage: Option<CumulativeTokens>,
}

#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
struct CumulativeTokens {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(alias = "cache_read_input_tokens")]
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

// ---------------------------------------------------------------------------
// Session file parsing
// ---------------------------------------------------------------------------

async fn parse_session_file(path: &PathBuf, session_id: &str) -> Result<Vec<UsageEntry>> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        CcstatError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {}", path.display(), e),
        ))
    })?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut entries = Vec::new();
    let mut current_model: Option<String> = None;
    let mut prev_cumulative = HashMap::<String, CumulativeTokens>::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let event: CodexEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match event.event_type.as_str() {
            "turn_context" => {
                if let Some(model) = event.model_id {
                    current_model = Some(normalize_model(&model));
                }
            }
            "event_msg" => {
                let Some(payload) = &event.payload else {
                    continue;
                };
                if payload.payload_type.as_deref() != Some("token_count") {
                    continue;
                }
                let Some(info) = &payload.info else {
                    continue;
                };
                let Some(timestamp_str) = &event.timestamp else {
                    continue;
                };

                let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
                    Ok(dt) => ISOTimestamp::new(dt.to_utc()),
                    Err(_) => {
                        warn!(
                            "Invalid timestamp in Codex session {}: {}",
                            session_id, timestamp_str
                        );
                        continue;
                    }
                };

                let model_name = current_model
                    .as_deref()
                    .unwrap_or(FALLBACK_MODEL)
                    .to_string();

                // Compute delta tokens
                let delta = compute_delta(info, prev_cumulative.get(&model_name));

                // Update cumulative state
                if let Some(total) = &info.total_token_usage {
                    prev_cumulative.insert(model_name.clone(), total.clone());
                }

                // Skip zero-token entries
                if delta.input_tokens == 0 && delta.output_tokens == 0 {
                    continue;
                }

                entries.push(UsageEntry {
                    session_id: SessionId::new(session_id.to_string()),
                    timestamp,
                    model: ModelName::new(model_name),
                    tokens: delta,
                    total_cost: None,
                    project: None,
                    instance_id: None,
                });
            }
            _ => {}
        }
    }

    Ok(entries)
}

/// Compute per-event delta tokens from cumulative or last_token_usage data.
fn compute_delta(info: &TokenInfo, prev: Option<&CumulativeTokens>) -> TokenCounts {
    // Prefer last_token_usage when available (already a delta)
    if let Some(last) = &info.last_token_usage {
        let cache_read = if last.cached_input_tokens > 0 {
            last.cached_input_tokens
        } else {
            last.cache_read_tokens
        };
        return TokenCounts::new(last.input_tokens, last.output_tokens, 0, cache_read);
    }

    // Fall back to cumulative-to-delta conversion
    if let Some(total) = &info.total_token_usage {
        let cache_read_total = if total.cached_input_tokens > 0 {
            total.cached_input_tokens
        } else {
            total.cache_read_tokens
        };

        let (prev_input, prev_output, prev_cache_read) = match prev {
            Some(p) => {
                let p_cache = if p.cached_input_tokens > 0 {
                    p.cached_input_tokens
                } else {
                    p.cache_read_tokens
                };
                (p.input_tokens, p.output_tokens, p_cache)
            }
            None => (0, 0, 0),
        };

        TokenCounts::new(
            total.input_tokens.saturating_sub(prev_input),
            total.output_tokens.saturating_sub(prev_output),
            0,
            cache_read_total.saturating_sub(prev_cache_read),
        )
    } else {
        TokenCounts::new(0, 0, 0, 0)
    }
}

/// Normalize Codex model names (gpt-5-codex → gpt-5).
fn normalize_model(model: &str) -> String {
    if model == "gpt-5-codex" {
        "gpt-5".to_string()
    } else {
        model.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_token_event(ts: &str, input: u64, output: u64, cached: u64) -> String {
        format!(
            r#"{{"type":"event_msg","timestamp":"{}","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{},"cached_input_tokens":{},"output_tokens":{},"total_tokens":{}}}}}}}}}"#,
            ts,
            input,
            cached,
            output,
            input + output
        )
    }

    fn make_token_event_with_last(
        ts: &str,
        total_input: u64,
        total_output: u64,
        last_input: u64,
        last_output: u64,
    ) -> String {
        format!(
            r#"{{"type":"event_msg","timestamp":"{}","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{},"output_tokens":{},"total_tokens":{}}},"last_token_usage":{{"input_tokens":{},"output_tokens":{},"total_tokens":{}}}}}}}}}"#,
            ts,
            total_input,
            total_output,
            total_input + total_output,
            last_input,
            last_output,
            last_input + last_output,
        )
    }

    fn make_turn_context(model: &str) -> String {
        format!(r#"{{"type":"turn_context","model_id":"{}"}}"#, model)
    }

    #[tokio::test]
    async fn test_cumulative_to_delta() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_file = sessions_dir.join("test-session.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();

        writeln!(f, "{}", make_turn_context("gpt-5")).unwrap();
        // First event: cumulative 100 input, 50 output
        writeln!(
            f,
            "{}",
            make_token_event("2025-01-01T10:00:00Z", 100, 50, 0)
        )
        .unwrap();
        // Second event: cumulative 300 input, 150 output → delta = 200 input, 100 output
        writeln!(
            f,
            "{}",
            make_token_event("2025-01-01T10:05:00Z", 300, 150, 0)
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "test-session")
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tokens.input_tokens, 100);
        assert_eq!(entries[0].tokens.output_tokens, 50);
        assert_eq!(entries[1].tokens.input_tokens, 200);
        assert_eq!(entries[1].tokens.output_tokens, 100);
    }

    #[tokio::test]
    async fn test_last_token_usage_preferred() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_file = sessions_dir.join("test-last.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();

        writeln!(f, "{}", make_turn_context("gpt-5")).unwrap();
        writeln!(
            f,
            "{}",
            make_token_event_with_last("2025-01-01T10:00:00Z", 500, 200, 50, 20)
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "test-last")
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        // Should use last_token_usage (50, 20), not cumulative (500, 200)
        assert_eq!(entries[0].tokens.input_tokens, 50);
        assert_eq!(entries[0].tokens.output_tokens, 20);
    }

    #[tokio::test]
    async fn test_model_fallback() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let session_file = sessions_dir.join("no-model.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();

        // No turn_context → should use fallback model
        writeln!(
            f,
            "{}",
            make_token_event("2025-01-01T10:00:00Z", 100, 50, 0)
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "no-model").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model.as_str(), FALLBACK_MODEL);
    }

    #[tokio::test]
    async fn test_model_alias() {
        assert_eq!(normalize_model("gpt-5-codex"), "gpt-5");
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_model("o3-mini"), "o3-mini");
    }

    #[tokio::test]
    async fn test_data_loader_no_dir() {
        // When CODEX_HOME points to nonexistent dir, new() should succeed
        // but load_entries should return empty stream
        unsafe {
            std::env::set_var("CODEX_HOME", "/tmp/nonexistent-codex-test-dir");
        }
        let loader = DataLoader::new().await.unwrap();
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert!(entries.is_empty());
        unsafe {
            std::env::remove_var("CODEX_HOME");
        }
    }
}
