//! Test fixtures and utilities for ccstat tests

use ccstat::types::{
    CostMode, ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry, DailyDate,
};
use chrono::{DateTime, NaiveDate, Utc, TimeZone};
use std::collections::HashMap;

/// Create a test UsageEntry with customizable fields
pub fn create_usage_entry(
    session_id: &str,
    timestamp: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> UsageEntry {
    UsageEntry {
        session_id: SessionId::new(session_id),
        timestamp: ISOTimestamp::new(
            DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
        ),
        model: ModelName::new(model),
        tokens: TokenCounts::new(input_tokens, output_tokens, 0, 0),
        total_cost: None,
        project: None,
        instance_id: None,
    }
}

/// Create a test UsageEntry with all fields populated
pub fn create_full_usage_entry(
    session_id: &str,
    timestamp: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_input: u64,
    cache_output: u64,
    cost: f64,
    project: &str,
    instance_id: &str,
) -> UsageEntry {
    UsageEntry {
        session_id: SessionId::new(session_id),
        timestamp: ISOTimestamp::new(
            DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
        ),
        model: ModelName::new(model),
        tokens: TokenCounts::new(input_tokens, output_tokens, cache_input, cache_output),
        total_cost: Some(cost),
        project: Some(project.to_string()),
        instance_id: Some(instance_id.to_string()),
    }
}

/// Create sample pricing data for tests
pub fn create_test_pricing() -> HashMap<String, f64> {
    let mut pricing = HashMap::new();

    // Claude 3 models
    pricing.insert("claude-3-opus:input".to_string(), 0.000015);
    pricing.insert("claude-3-opus:output".to_string(), 0.000075);
    pricing.insert("claude-3-opus:cache_input".to_string(), 0.0000075);
    pricing.insert("claude-3-opus:cache_output".to_string(), 0.000075);

    pricing.insert("claude-3-sonnet:input".to_string(), 0.000003);
    pricing.insert("claude-3-sonnet:output".to_string(), 0.000015);
    pricing.insert("claude-3-sonnet:cache_input".to_string(), 0.0000015);
    pricing.insert("claude-3-sonnet:cache_output".to_string(), 0.000015);

    pricing.insert("claude-3-haiku:input".to_string(), 0.00000025);
    pricing.insert("claude-3-haiku:output".to_string(), 0.00000125);
    pricing.insert("claude-3-haiku:cache_input".to_string(), 0.000000125);
    pricing.insert("claude-3-haiku:cache_output".to_string(), 0.00000125);

    pricing
}

/// Create sample entries for a typical day
pub fn create_daily_entries() -> Vec<UsageEntry> {
    vec![
        create_usage_entry(
            "session-1",
            "2024-01-01T09:00:00Z",
            "claude-3-opus",
            1000,
            500,
        ),
        create_usage_entry(
            "session-1",
            "2024-01-01T09:30:00Z",
            "claude-3-opus",
            2000,
            1000,
        ),
        create_usage_entry(
            "session-2",
            "2024-01-01T10:00:00Z",
            "claude-3-sonnet",
            3000,
            1500,
        ),
        create_usage_entry(
            "session-3",
            "2024-01-01T14:00:00Z",
            "claude-3-haiku",
            5000,
            2500,
        ),
    ]
}

/// Create sample entries spanning multiple days
pub fn create_multi_day_entries() -> Vec<UsageEntry> {
    let mut entries = Vec::new();

    // Day 1
    entries.push(create_usage_entry(
        "session-1",
        "2024-01-01T09:00:00Z",
        "claude-3-opus",
        1000,
        500,
    ));
    entries.push(create_usage_entry(
        "session-2",
        "2024-01-01T14:00:00Z",
        "claude-3-sonnet",
        2000,
        1000,
    ));

    // Day 2
    entries.push(create_usage_entry(
        "session-3",
        "2024-01-02T10:00:00Z",
        "claude-3-opus",
        3000,
        1500,
    ));
    entries.push(create_usage_entry(
        "session-4",
        "2024-01-02T15:00:00Z",
        "claude-3-haiku",
        4000,
        2000,
    ));

    // Day 3
    entries.push(create_usage_entry(
        "session-5",
        "2024-01-03T11:00:00Z",
        "claude-3-sonnet",
        5000,
        2500,
    ));

    entries
}

/// Create sample entries with various edge cases
pub fn create_edge_case_entries() -> Vec<UsageEntry> {
    vec![
        // Zero tokens
        create_usage_entry("session-1", "2024-01-01T09:00:00Z", "claude-3-opus", 0, 0),
        // Very large token counts
        create_usage_entry(
            "session-2",
            "2024-01-01T10:00:00Z",
            "claude-3-opus",
            1_000_000,
            500_000,
        ),
        // Unknown model
        create_usage_entry(
            "session-3",
            "2024-01-01T11:00:00Z",
            "unknown-model",
            1000,
            500,
        ),
        // With cache tokens
        create_full_usage_entry(
            "session-4",
            "2024-01-01T12:00:00Z",
            "claude-3-sonnet",
            1000,
            500,
            200,
            100,
            0.025,
            "test-project",
            "instance-1",
        ),
    ]
}

/// Create test dates
pub fn create_test_dates() -> Vec<DailyDate> {
    vec![
        DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
        DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()),
        DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 3).unwrap()),
        DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap()),
        DailyDate::new(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()),
    ]
}

/// Create a test timestamp
pub fn create_test_timestamp(datetime_str: &str) -> ISOTimestamp {
    ISOTimestamp::new(
        DateTime::parse_from_rfc3339(datetime_str)
            .unwrap()
            .with_timezone(&Utc),
    )
}

/// Create test session IDs
pub fn create_test_session_ids() -> Vec<SessionId> {
    vec![
        SessionId::new("session-1"),
        SessionId::new("session-2"),
        SessionId::new("test-session-abc123"),
        SessionId::new("00000000-0000-0000-0000-000000000000"),
    ]
}

/// Create test model names
pub fn create_test_model_names() -> Vec<ModelName> {
    vec![
        ModelName::new("claude-3-opus"),
        ModelName::new("claude-3-sonnet"),
        ModelName::new("claude-3-haiku"),
        ModelName::new("claude-3.5-sonnet"),
        ModelName::new("unknown-model"),
    ]
}

/// Create test token counts
pub fn create_test_token_counts() -> Vec<TokenCounts> {
    vec![
        TokenCounts::new(0, 0, 0, 0),
        TokenCounts::new(100, 50, 0, 0),
        TokenCounts::new(1000, 500, 200, 100),
        TokenCounts::new(1_000_000, 500_000, 100_000, 50_000),
        TokenCounts::new(u64::MAX / 2, u64::MAX / 2, 0, 0), // Test large values
    ]
}

/// Create JSONL test data as string
pub fn create_jsonl_test_data() -> String {
    let entries = vec![
        r#"{"session_id":"s1","timestamp":"2024-01-01T10:00:00Z","model":"claude-3-opus","input_tokens":1000,"output_tokens":500}"#,
        r#"{"session_id":"s2","timestamp":"2024-01-01T11:00:00Z","model":"claude-3-sonnet","input_tokens":2000,"output_tokens":1000,"project":"test-project"}"#,
        r#"{"session_id":"s3","timestamp":"2024-01-02T10:00:00Z","model":"claude-3-haiku","input_tokens":3000,"output_tokens":1500,"instance_id":"instance-1"}"#,
        r#"{"session_id":"s4","timestamp":"2024-01-02T11:00:00Z","model":"claude-3-opus","input_tokens":4000,"output_tokens":2000,"cache_input_tokens":400,"cache_output_tokens":200,"total_cost":0.15}"#,
    ];
    entries.join("\n")
}

/// Create corrupted JSONL test data
pub fn create_corrupted_jsonl_data() -> String {
    let entries = vec![
        r#"{"session_id":"s1","timestamp":"2024-01-01T10:00:00Z","model":"claude-3-opus","input_tokens":1000,"output_tokens":500}"#,
        r#"invalid json line"#,
        r#"{"session_id":"s2","timestamp":"invalid-date","model":"claude-3-sonnet","input_tokens":2000,"output_tokens":1000}"#,
        r#"{"session_id":"s3","model":"claude-3-haiku","input_tokens":"not-a-number","output_tokens":1500}"#,
        r#"{"session_id":"s4","timestamp":"2024-01-02T11:00:00Z","model":"claude-3-opus","input_tokens":4000,"output_tokens":2000}"#,
    ];
    entries.join("\n")
}
