//! Common test utilities and helpers for ccstat tests
//!
//! This module provides reusable test utilities, mock data generators,
//! and helper functions to make testing easier and more consistent.

use ccstat::{
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    pricing_fetcher::PricingFetcher,
    types::{ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry},
};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use once_cell::sync::Lazy;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;
use tokio::io::AsyncWriteExt;

// Global mutex to serialize environment variable modifications in tests
pub static ENV_MUTEX: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

/// Common test models used across tests
pub const TEST_MODELS: &[&str] = &[
    "claude-3-opus-20240229",
    "claude-3-sonnet-20240229",
    "claude-3-haiku-20240307",
    "claude-3-opus-20240229", // Use opus again instead of 3.5 sonnet to avoid pricing issues
];

/// Common test projects
pub const TEST_PROJECTS: &[&str] = &[
    "project-alpha",
    "project-beta",
    "project-gamma",
    "project-delta",
];

/// Common test instances
#[allow(dead_code)]
pub const TEST_INSTANCES: &[&str] = &["default", "instance-1", "instance-2", "instance-prod"];

/// Builder for creating test UsageEntry instances
pub struct UsageEntryBuilder {
    session_id: String,
    timestamp: DateTime<Utc>,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_cost: Option<f64>,
    project: Option<String>,
    instance_id: Option<String>,
}

impl UsageEntryBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            session_id: "test-session".to_string(),
            timestamp: Utc::now(),
            model: TEST_MODELS[0].to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_tokens: 10,
            cache_read_tokens: 5,
            total_cost: None,
            project: None,
            instance_id: None,
        }
    }

    pub fn with_session_id(mut self, id: &str) -> Self {
        self.session_id = id.to_string();
        self
    }

    #[allow(dead_code)]
    pub fn with_timestamp(mut self, ts: DateTime<Utc>) -> Self {
        self.timestamp = ts;
        self
    }

    pub fn with_date(mut self, year: i32, month: u32, day: u32, hour: u32) -> Self {
        self.timestamp = Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_tokens(mut self, input: u64, output: u64) -> Self {
        self.input_tokens = input;
        self.output_tokens = output;
        self
    }

    #[allow(dead_code)]
    pub fn with_cache_tokens(mut self, creation: u64, read: u64) -> Self {
        self.cache_creation_tokens = creation;
        self.cache_read_tokens = read;
        self
    }

    pub fn with_cost(mut self, cost: f64) -> Self {
        self.total_cost = Some(cost);
        self
    }

    pub fn with_project(mut self, project: &str) -> Self {
        self.project = Some(project.to_string());
        self
    }

    pub fn with_instance(mut self, instance: &str) -> Self {
        self.instance_id = Some(instance.to_string());
        self
    }

    /// Build the UsageEntry
    pub fn build(self) -> UsageEntry {
        UsageEntry {
            session_id: SessionId::new(&self.session_id),
            timestamp: ISOTimestamp::new(self.timestamp),
            model: ModelName::new(&self.model),
            tokens: TokenCounts::new(
                self.input_tokens,
                self.output_tokens,
                self.cache_creation_tokens,
                self.cache_read_tokens,
            ),
            total_cost: self.total_cost,
            project: self.project,
            instance_id: self.instance_id,
        }
    }

    /// Build as JSONL string
    #[allow(clippy::wrong_self_convention)]
    pub fn to_jsonl(self) -> String {
        let instance_field = self
            .instance_id
            .map(|id| format!(r#","uuid":"{}""#, id))
            .unwrap_or_default();

        let project_field = self
            .project
            .map(|p| format!(r#","cwd":"/home/user/{}""#, p))
            .unwrap_or_default();

        let cost_field = self
            .total_cost
            .map(|c| format!(r#","costUSD":{}"#, c))
            .unwrap_or_default();

        format!(
            r#"{{"sessionId":"{}","timestamp":"{}","type":"assistant","message":{{"model":"{}","usage":{{"input_tokens":{},"output_tokens":{},"cache_creation_input_tokens":{},"cache_read_input_tokens":{}}}}}{}{}{}}}"#,
            self.session_id,
            self.timestamp.to_rfc3339(),
            self.model,
            self.input_tokens,
            self.output_tokens,
            self.cache_creation_tokens,
            self.cache_read_tokens,
            project_field,
            cost_field,
            instance_field
        )
    }
}

/// Helper to create a test data directory with JSONL files
pub async fn create_test_data_dir(entries: Vec<String>) -> (TempDir, DataLoader) {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    // Write entries to JSONL file (no mutex needed for file operations)
    let jsonl_path = path.join("test_data.jsonl");
    let mut file = fs::File::create(&jsonl_path).await.unwrap();

    for entry in entries {
        file.write_all(entry.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
    }

    drop(file);

    // Now lock the mutex for environment variable modification and DataLoader creation
    // We need to do all the async work inside a block that releases the lock before returning
    let loader = {
        let _lock = ENV_MUTEX.lock().await;

        // Create DataLoader with test directory
        let original_home = std::env::var("HOME").ok();
        // Note: env functions are unsafe in Rust 1.82+ due to thread-safety concerns
        // We use a mutex to ensure thread safety, but the functions still require unsafe blocks
        unsafe {
            std::env::set_var("HOME", "/nonexistent");
            std::env::set_var("CLAUDE_DATA_PATH", path.to_str().unwrap());
        }

        let loader = DataLoader::new()
            .await
            .expect("Failed to create DataLoader");

        unsafe {
            std::env::remove_var("CLAUDE_DATA_PATH");
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            }
        }

        loader
    };

    (temp_dir, loader)
}

/// Generate test data for a specific date range
pub fn generate_date_range_data(
    start_date: NaiveDate,
    end_date: NaiveDate,
    sessions_per_day: usize,
) -> Vec<String> {
    let mut entries = Vec::new();
    let mut current_date = start_date;
    let mut days_generated = 0;

    let mut day_number = 0;
    while current_date <= end_date {
        days_generated += 1;
        for session in 0..sessions_per_day {
            let hour = 9 + (session * 4) % 15; // Distribute across working hours
            let model = TEST_MODELS[session % TEST_MODELS.len()];
            let project = TEST_PROJECTS[session % TEST_PROJECTS.len()];

            // Increase tokens and costs over time to simulate growing usage
            let day_multiplier = 1.0 + (day_number as f64 * 0.1); // 10% increase per day
            let base_input = 1000 + session as u64 * 100;
            let base_output = 500 + session as u64 * 50;

            // Add instance IDs to some entries for testing instance grouping
            let mut builder = UsageEntryBuilder::new()
                .with_session_id(&format!(
                    "session-{}-{}",
                    current_date.format("%Y%m%d"),
                    session
                ))
                .with_date(
                    current_date.year(),
                    current_date.month(),
                    current_date.day(),
                    hour as u32,
                )
                .with_model(model)
                .with_tokens(
                    (base_input as f64 * day_multiplier) as u64,
                    (base_output as f64 * day_multiplier) as u64,
                )
                .with_project(project)
                .with_cost(0.01 * (session + 1) as f64 * day_multiplier);

            // Add instance ID to some sessions (about 1/3 of them)
            if session % 3 == 1 {
                builder = builder.with_instance("instance-1");
            } else if session % 3 == 2 {
                builder = builder.with_instance("instance-2");
            }
            // else leave as default (no instance ID)

            let entry = builder.to_jsonl();

            entries.push(entry);
        }

        // Only increment if we haven't reached the end date
        if current_date >= end_date {
            break;
        }
        current_date = current_date.succ_opt().unwrap_or(current_date);
        day_number += 1;
    }

    eprintln!(
        "Generated {} days of data from {} to {}",
        days_generated, start_date, end_date
    );

    entries
}

/// Generate test data with specific patterns for testing aggregation
pub fn generate_pattern_data() -> Vec<String> {
    let mut entries = Vec::new();

    // Pattern 1: Heavy morning usage
    for hour in 6..12 {
        entries.push(
            UsageEntryBuilder::new()
                .with_session_id(&format!("morning-{}", hour))
                .with_date(2024, 1, 15, hour)
                .with_tokens(2000, 1000)
                .with_model(TEST_MODELS[0])
                .with_project(TEST_PROJECTS[0])
                .to_jsonl(),
        );
    }

    // Pattern 2: Distributed afternoon usage
    for hour in 13..18 {
        entries.push(
            UsageEntryBuilder::new()
                .with_session_id(&format!("afternoon-{}", hour))
                .with_date(2024, 1, 15, hour)
                .with_tokens(500, 250)
                .with_model(TEST_MODELS[1])
                .with_project(TEST_PROJECTS[1])
                .to_jsonl(),
        );
    }

    // Pattern 3: Late night batch processing
    for hour in 22..24 {
        entries.push(
            UsageEntryBuilder::new()
                .with_session_id(&format!("batch-{}", hour))
                .with_date(2024, 1, 15, hour)
                .with_tokens(10000, 5000)
                .with_model(TEST_MODELS[2])
                .with_project(TEST_PROJECTS[2])
                .with_instance("instance-batch")
                .to_jsonl(),
        );
    }

    entries
}

/// Create a standard test environment with cost calculator
#[allow(dead_code)]
pub async fn create_test_environment() -> Arc<CostCalculator> {
    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    Arc::new(CostCalculator::new(pricing_fetcher))
}

/// Assert that two float values are approximately equal
pub fn assert_approx_eq(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "Values are not approximately equal: {} != {} (tolerance: {})",
        a,
        b,
        tolerance
    );
}

/// Helper to verify token count aggregation
#[allow(dead_code)]
pub fn verify_token_totals(entries: &[UsageEntry], expected_input: u64, expected_output: u64) {
    let total_input: u64 = entries.iter().map(|e| e.tokens.input_tokens).sum();
    let total_output: u64 = entries.iter().map(|e| e.tokens.output_tokens).sum();

    assert_eq!(
        total_input, expected_input,
        "Input token mismatch: got {}, expected {}",
        total_input, expected_input
    );

    assert_eq!(
        total_output, expected_output,
        "Output token mismatch: got {}, expected {}",
        total_output, expected_output
    );
}

/// Generate test data for billing block testing (5-hour windows)
pub fn generate_billing_block_data() -> Vec<String> {
    let mut entries = Vec::new();

    // Create sessions spanning multiple billing blocks
    for block in 0..5 {
        let start_hour = block * 5;

        // Add sessions at the start, middle, and end of each block
        for offset in [0, 2, 4] {
            let hour = start_hour + offset;
            if hour < 24 {
                entries.push(
                    UsageEntryBuilder::new()
                        .with_session_id(&format!("block-{}-{}", block, offset))
                        .with_date(2024, 1, 20, hour as u32)
                        .with_tokens(1000, 500)
                        .with_model(TEST_MODELS[block % TEST_MODELS.len()])
                        .to_jsonl(),
                );
            }
        }
    }

    entries
}

/// Mock data for testing error conditions
#[allow(dead_code)]
pub fn generate_invalid_data() -> Vec<String> {
    vec![
        // Invalid JSON
        "not valid json".to_string(),

        // Missing required fields
        r#"{"timestamp":"2024-01-01T00:00:00Z"}"#.to_string(),

        // Wrong type (not assistant)
        r#"{"sessionId":"test","timestamp":"2024-01-01T00:00:00Z","type":"user","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#.to_string(),

        // Malformed timestamp
        r#"{"sessionId":"test","timestamp":"not-a-date","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50}}}"#.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_entry_builder() {
        let entry = UsageEntryBuilder::new()
            .with_session_id("test-123")
            .with_model(TEST_MODELS[0])
            .with_tokens(200, 100)
            .with_project(TEST_PROJECTS[0])
            .build();

        assert_eq!(entry.session_id.as_str(), "test-123");
        assert_eq!(entry.model.as_str(), TEST_MODELS[0]);
        assert_eq!(entry.tokens.input_tokens, 200);
        assert_eq!(entry.tokens.output_tokens, 100);
        assert_eq!(entry.project, Some(TEST_PROJECTS[0].to_string()));
    }

    #[test]
    fn test_jsonl_generation() {
        let jsonl = UsageEntryBuilder::new()
            .with_session_id("test-session")
            .with_tokens(100, 50)
            .to_jsonl();

        assert!(jsonl.contains(r#""sessionId":"test-session""#));
        assert!(jsonl.contains(r#""input_tokens":100"#));
        assert!(jsonl.contains(r#""output_tokens":50"#));
        assert!(jsonl.contains(r#""type":"assistant""#));
    }

    #[test]
    fn test_date_range_generation() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        let entries = generate_date_range_data(start, end, 2);

        // Should have 3 days * 2 sessions = 6 entries
        assert_eq!(entries.len(), 6);

        // Verify date progression
        assert!(entries[0].contains("session-20240101-0"));
        assert!(entries[5].contains("session-20240103-1"));
    }

    #[test]
    fn test_pattern_data_generation() {
        let entries = generate_pattern_data();

        // Should have morning (6) + afternoon (5) + late night (2) = 13 entries
        assert_eq!(entries.len(), 13);

        // Verify patterns
        assert!(entries[0].contains("morning-"));
        assert!(entries[6].contains("afternoon-"));
        assert!(entries[11].contains("batch-"));
    }

    #[test]
    fn test_billing_block_data() {
        let entries = generate_billing_block_data();

        // 5 blocks * 3 sessions each (but last block might be truncated)
        assert!(!entries.is_empty());

        // Verify block structure
        assert!(entries[0].contains("block-0-0"));
    }

    #[test]
    fn test_approx_eq() {
        assert_approx_eq(1.0, 1.0001, 0.001);
        assert_approx_eq(100.5, 100.49, 0.1);
    }

    #[test]
    #[should_panic(expected = "Values are not approximately equal")]
    fn test_approx_eq_fails() {
        assert_approx_eq(1.0, 2.0, 0.9); // 1.0 difference is > 0.9 tolerance
    }
}
