//! Pi data loader
//!
//! Discovers and parses Pi session JSONL files from
//! `~/.pi/agent/sessions/{project}/{session_id}.jsonl`.

use async_trait::async_trait;
use ccstat_core::error::{CcstatError, Result};
use ccstat_core::provider::ProviderDataLoader;
use ccstat_core::types::{ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry};
use chrono::DateTime;
use futures::stream::Stream;
use serde::Deserialize;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};

/// Data loader for Pi usage data.
pub struct DataLoader {
    sessions_dir: PathBuf,
}

#[async_trait]
impl ProviderDataLoader for DataLoader {
    async fn new() -> Result<Self> {
        let base = if let Ok(agent_dir) = std::env::var("PI_AGENT_DIR") {
            PathBuf::from(agent_dir)
        } else {
            dirs::home_dir()
                .ok_or_else(|| CcstatError::Config("Cannot determine home directory".into()))?
                .join(".pi")
                .join("agent")
        };

        let sessions_dir = base.join("sessions");
        if !sessions_dir.exists() {
            debug!(
                "Pi sessions directory not found: {}",
                sessions_dir.display()
            );
        }

        Ok(DataLoader { sessions_dir })
    }

    fn load_entries(&self) -> Pin<Box<dyn Stream<Item = Result<UsageEntry>> + Send + '_>> {
        Box::pin(async_stream::try_stream! {
            if !self.sessions_dir.exists() {
                return;
            }

            let mut jsonl_files = Vec::new();
            for entry in walkdir::WalkDir::new(&self.sessions_dir)
                .min_depth(2)
                .max_depth(2)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                if path.extension().is_some_and(|ext| ext == "jsonl") {
                    jsonl_files.push(path);
                }
            }

            debug!("Found {} Pi session files", jsonl_files.len());

            for path in jsonl_files {
                // Extract project from parent directory name
                let project = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());

                // Extract session ID from filename
                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let entries = parse_session_file(&path, &session_id, project.as_deref()).await;
                match entries {
                    Ok(parsed) => {
                        for entry in parsed {
                            yield entry;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Pi session {}: {}", session_id, e);
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Entry schema
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PiEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<PiMessage>,
}

#[derive(Deserialize)]
struct PiMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<PiUsage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PiUsage {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache_read: u64,
    #[serde(default)]
    cache_write: u64,
    #[serde(default)]
    cost: Option<PiCost>,
}

#[derive(Deserialize)]
struct PiCost {
    #[serde(default)]
    total: Option<f64>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

async fn parse_session_file(
    path: &PathBuf,
    session_id: &str,
    project: Option<&str>,
) -> Result<Vec<UsageEntry>> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        CcstatError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {}", path.display(), e),
        ))
    })?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut entries = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let entry: PiEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let Some(message) = &entry.message else {
            continue;
        };

        // Only process assistant messages with usage data
        if message.role.as_deref() != Some("assistant") {
            continue;
        }
        let Some(usage) = &message.usage else {
            continue;
        };

        // Skip zero-token entries
        if usage.input == 0 && usage.output == 0 {
            continue;
        }

        let timestamp = entry
            .timestamp
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| ISOTimestamp::new(dt.to_utc()));

        let Some(timestamp) = timestamp else {
            continue;
        };

        // Add [pi] prefix to model name
        let model_name = message
            .model
            .as_deref()
            .map(|m| format!("[pi] {}", m))
            .unwrap_or_else(|| "[pi] unknown".to_string());

        let total_cost = usage.cost.as_ref().and_then(|c| c.total);

        entries.push(UsageEntry {
            session_id: SessionId::new(session_id.to_string()),
            timestamp,
            model: ModelName::new(model_name),
            tokens: TokenCounts::new(
                usage.input,
                usage.output,
                usage.cache_write,
                usage.cache_read,
            ),
            total_cost,
            project: project.map(|s| s.to_string()),
            instance_id: None,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_pi_entry(
        ts: &str,
        role: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        cost: Option<f64>,
    ) -> String {
        let cost_json = match cost {
            Some(c) => format!(r#","cost":{{"total":{}}}"#, c),
            None => String::new(),
        };
        format!(
            r#"{{"timestamp":"{}","message":{{"role":"{}","model":"{}","usage":{{"input":{},"output":{},"cacheRead":{},"cacheWrite":{}{}}}}}}}"#,
            ts, role, model, input, output, cache_read, cache_write, cost_json
        )
    }

    #[tokio::test]
    async fn test_parse_assistant_entry() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        let project_dir = sessions_dir.join("my-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let session_file = project_dir.join("sess1.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();
        writeln!(
            f,
            "{}",
            make_pi_entry(
                "2025-01-01T10:00:00Z",
                "assistant",
                "claude-opus-4",
                500,
                200,
                50,
                10,
                Some(0.05)
            )
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "sess1", Some("my-project"))
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model.as_str(), "[pi] claude-opus-4");
        assert_eq!(entries[0].tokens.input_tokens, 500);
        assert_eq!(entries[0].tokens.output_tokens, 200);
        assert_eq!(entries[0].tokens.cache_read_tokens, 50);
        assert_eq!(entries[0].tokens.cache_creation_tokens, 10);
        assert_eq!(entries[0].total_cost, Some(0.05));
        assert_eq!(entries[0].project.as_deref(), Some("my-project"));
    }

    #[tokio::test]
    async fn test_skip_non_assistant() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        let project_dir = sessions_dir.join("proj");
        std::fs::create_dir_all(&project_dir).unwrap();

        let session_file = project_dir.join("sess2.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();
        // User message should be skipped
        writeln!(
            f,
            "{}",
            make_pi_entry(
                "2025-01-01T10:00:00Z",
                "user",
                "claude-opus-4",
                100,
                50,
                0,
                0,
                None
            )
        )
        .unwrap();
        // Assistant message should be included
        writeln!(
            f,
            "{}",
            make_pi_entry(
                "2025-01-01T10:01:00Z",
                "assistant",
                "claude-opus-4",
                200,
                100,
                0,
                0,
                None
            )
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "sess2", Some("proj"))
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tokens.input_tokens, 200);
    }

    #[tokio::test]
    async fn test_model_prefix() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        let project_dir = sessions_dir.join("proj");
        std::fs::create_dir_all(&project_dir).unwrap();

        let session_file = project_dir.join("sess3.jsonl");
        let mut f = std::fs::File::create(&session_file).unwrap();
        writeln!(
            f,
            "{}",
            make_pi_entry(
                "2025-01-01T10:00:00Z",
                "assistant",
                "claude-sonnet-4",
                100,
                50,
                0,
                0,
                None
            )
        )
        .unwrap();

        let entries = parse_session_file(&session_file, "sess3", Some("proj"))
            .await
            .unwrap();
        assert_eq!(entries[0].model.as_str(), "[pi] claude-sonnet-4");
    }

    #[tokio::test]
    async fn test_no_dir() {
        let loader = DataLoader {
            sessions_dir: PathBuf::from("/tmp/nonexistent-pi-dir"),
        };
        let entries: Vec<_> = futures::StreamExt::collect::<Vec<_>>(loader.load_entries()).await;
        assert!(entries.is_empty());
    }
}
