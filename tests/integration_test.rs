//! Integration tests for ccstat

use ccstat::{
    aggregation::Aggregator,
    cost_calculator::CostCalculator,
    filters::{MonthFilter, UsageFilter},
    pricing_fetcher::PricingFetcher,
    types::{CostMode, ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry},
};
use chrono::{DateTime, NaiveDate, Utc};
use futures::{StreamExt, stream};
use std::sync::Arc;

fn create_test_entry(
    session_id: &str,
    timestamp: &str,
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
        model: ModelName::new("claude-3-opus"),
        tokens: TokenCounts::new(input_tokens, output_tokens, 0, 0),
        total_cost: None,
        project: None,
        instance_id: None,
    }
}

#[tokio::test]
async fn test_date_filtering() {
    let entries = vec![
        create_test_entry("s1", "2024-01-01T10:00:00Z", 100, 50),
        create_test_entry("s2", "2024-01-15T10:00:00Z", 200, 100),
        create_test_entry("s3", "2024-02-01T10:00:00Z", 300, 150),
    ];

    let filter = UsageFilter::new()
        .with_since(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap())
        .with_until(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let filtered_entries: Vec<_> = filter
        .filter_stream(entries_stream)
        .await
        .collect::<Vec<_>>()
        .await;

    assert_eq!(filtered_entries.len(), 1);
    assert_eq!(
        filtered_entries[0].as_ref().unwrap().session_id.as_str(),
        "s2"
    );
}

#[tokio::test]
async fn test_monthly_aggregation_with_filter() {
    let entries = vec![
        create_test_entry("s1", "2024-01-01T10:00:00Z", 100, 50),
        create_test_entry("s2", "2024-02-15T10:00:00Z", 200, 100),
        create_test_entry("s3", "2024-03-01T10:00:00Z", 300, 150),
        create_test_entry("s4", "2024-04-01T10:00:00Z", 400, 200),
    ];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let daily_data = aggregator
        .aggregate_daily(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);

    // Apply month filter
    let month_filter = MonthFilter::new().with_since(2024, 2).with_until(2024, 3);

    monthly_data.retain(|monthly| {
        if let Some((year, month)) = monthly
            .month
            .split_once('-')
            .and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?)))
        {
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, 1) {
                return month_filter.matches_date(&date);
            }
        }
        false
    });

    assert_eq!(monthly_data.len(), 2);
    assert_eq!(monthly_data[0].month, "2024-02");
    assert_eq!(monthly_data[1].month, "2024-03");
}

#[tokio::test]
async fn test_token_limit_warnings() {
    use ccstat::aggregation::SessionUsage;

    let sessions = vec![
        SessionUsage {
            session_id: SessionId::new("s1"),
            start_time: chrono::Utc::now() - chrono::Duration::hours(4),
            end_time: chrono::Utc::now() - chrono::Duration::hours(3),
            tokens: TokenCounts::new(1_000_000, 500_000, 100_000, 50_000),
            total_cost: 25.0,
            model: ModelName::new("claude-3-opus"),
        },
        SessionUsage {
            session_id: SessionId::new("s2"),
            start_time: chrono::Utc::now() - chrono::Duration::hours(2),
            end_time: chrono::Utc::now() - chrono::Duration::hours(1),
            tokens: TokenCounts::new(7_000_000, 2_000_000, 200_000, 100_000),
            total_cost: 150.0,
            model: ModelName::new("claude-3-opus"),
        },
    ];

    let mut blocks = Aggregator::create_billing_blocks(&sessions);

    // Simulate applying token limit warning
    let threshold = 8_000_000.0;
    for block in &mut blocks {
        if block.is_active {
            let total_tokens = block.tokens.total();
            if total_tokens as f64 >= threshold {
                block.warning = Some(format!(
                    "⚠️  Block has used {} tokens, exceeding threshold of {} tokens",
                    total_tokens, threshold as u64
                ));
            }
        }
    }

    // Check that warning was applied
    let active_blocks: Vec<_> = blocks.iter().filter(|b| b.is_active).collect();
    assert!(!active_blocks.is_empty());

    let total_tokens = active_blocks[0].tokens.total();
    if total_tokens >= 8_000_000 {
        assert!(active_blocks[0].warning.is_some());
    }
}

#[tokio::test]
async fn test_cost_calculation_modes() {
    let entries = vec![
        UsageEntry {
            session_id: SessionId::new("s1"),
            timestamp: ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(1000, 500, 0, 0),
            total_cost: Some(0.05), // Pre-calculated cost
            project: None,
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("s2"),
            timestamp: ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(2000, 1000, 0, 0),
            total_cost: None, // No pre-calculated cost
            project: None,
            instance_id: None,
        },
    ];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    // Test Auto mode
    let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
    let daily_data_auto = aggregator
        .aggregate_daily(entries_stream, CostMode::Auto)
        .await
        .unwrap();

    // Test Calculate mode
    let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
    let daily_data_calc = aggregator
        .aggregate_daily(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    // Test Display mode - should handle entries with and without pre-calculated cost
    let display_entries = vec![UsageEntry {
        session_id: SessionId::new("s1"),
        timestamp: ISOTimestamp::new(chrono::Utc::now()),
        model: ModelName::new("claude-3-opus"),
        tokens: TokenCounts::new(1000, 500, 0, 0),
        total_cost: Some(0.05), // This one has pre-calculated cost
        project: None,
        instance_id: None,
    }];
    let entries_stream = stream::iter(display_entries.into_iter().map(Ok));
    let daily_data_display = aggregator
        .aggregate_daily(entries_stream, CostMode::Display)
        .await
        .unwrap();

    // All modes should produce results
    assert!(!daily_data_auto.is_empty());
    assert!(!daily_data_calc.is_empty());
    assert!(!daily_data_display.is_empty());
}

#[tokio::test]
async fn test_project_filtering() {
    let entries = vec![
        UsageEntry {
            session_id: SessionId::new("s1"),
            timestamp: ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: Some("project-a".to_string()),
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("s2"),
            timestamp: ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(200, 100, 0, 0),
            total_cost: None,
            project: Some("project-b".to_string()),
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("s3"),
            timestamp: ISOTimestamp::new(chrono::Utc::now()),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(300, 150, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
    ];

    let filter = UsageFilter::new().with_project("project-a".to_string());

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let filtered_entries: Vec<_> = filter
        .filter_stream(entries_stream)
        .await
        .collect::<Vec<_>>()
        .await;

    assert_eq!(filtered_entries.len(), 1);
    assert_eq!(
        filtered_entries[0].as_ref().unwrap().session_id.as_str(),
        "s1"
    );
    assert_eq!(
        filtered_entries[0].as_ref().unwrap().project,
        Some("project-a".to_string())
    );
}

#[tokio::test]
async fn test_instance_grouping() {
    let entries = vec![
        UsageEntry {
            session_id: SessionId::new("s1"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T10:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: Some("instance-a".to_string()),
        },
        UsageEntry {
            session_id: SessionId::new("s2"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T11:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(200, 100, 0, 0),
            total_cost: None,
            project: None,
            instance_id: Some("instance-b".to_string()),
        },
        UsageEntry {
            session_id: SessionId::new("s3"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(300, 150, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None, // Will default to "default"
        },
    ];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let instance_data = aggregator
        .aggregate_daily_by_instance(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    // Should have 3 entries: one for each instance
    assert_eq!(instance_data.len(), 3);

    // Check instance IDs
    let instance_ids: Vec<_> = instance_data.iter().map(|d| &d.instance_id).collect();
    assert!(instance_ids.contains(&&"instance-a".to_string()));
    assert!(instance_ids.contains(&&"instance-b".to_string()));
    assert!(instance_ids.contains(&&"default".to_string()));

    // Verify tokens are correctly grouped
    for instance in &instance_data {
        match instance.instance_id.as_str() {
            "instance-a" => {
                assert_eq!(instance.tokens.input_tokens, 100);
                assert_eq!(instance.tokens.output_tokens, 50);
            }
            "instance-b" => {
                assert_eq!(instance.tokens.input_tokens, 200);
                assert_eq!(instance.tokens.output_tokens, 100);
            }
            "default" => {
                assert_eq!(instance.tokens.input_tokens, 300);
                assert_eq!(instance.tokens.output_tokens, 150);
            }
            _ => panic!("Unexpected instance ID"),
        }
    }
}

#[tokio::test]
async fn test_empty_data_handling() {
    let entries: Vec<UsageEntry> = vec![];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let daily_data = aggregator
        .aggregate_daily(entries_stream, CostMode::Auto)
        .await
        .unwrap();

    assert!(daily_data.is_empty());
}

#[tokio::test]
async fn test_session_duration_calculation() {
    let start_time = chrono::Utc::now() - chrono::Duration::hours(2);
    let entries = vec![
        UsageEntry {
            session_id: SessionId::new("session-1"),
            timestamp: ISOTimestamp::new(start_time),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("session-1"),
            timestamp: ISOTimestamp::new(start_time + chrono::Duration::minutes(30)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(200, 100, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("session-1"),
            timestamp: ISOTimestamp::new(start_time + chrono::Duration::hours(1)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(300, 150, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
    ];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let session_data = aggregator
        .aggregate_sessions(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    assert_eq!(session_data.len(), 1);
    assert_eq!(session_data[0].session_id.as_str(), "session-1");

    // Check duration is approximately 1 hour
    let duration = session_data[0].end_time - session_data[0].start_time;
    assert!(duration.num_minutes() >= 60);
    assert!(duration.num_minutes() <= 61);

    // Check tokens are accumulated
    assert_eq!(session_data[0].tokens.input_tokens, 600);
    assert_eq!(session_data[0].tokens.output_tokens, 300);
}

#[tokio::test]
async fn test_mixed_models_handling() {
    let entries = vec![
        UsageEntry {
            session_id: SessionId::new("s1"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T10:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::new(100, 50, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("s2"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T11:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-sonnet"),
            tokens: TokenCounts::new(200, 100, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
        UsageEntry {
            session_id: SessionId::new("s3"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-haiku"),
            tokens: TokenCounts::new(300, 150, 0, 0),
            total_cost: None,
            project: None,
            instance_id: None,
        },
    ];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let daily_data = aggregator
        .aggregate_daily(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    assert_eq!(daily_data.len(), 1);

    // Check that all models are tracked
    let models = &daily_data[0].models_used;
    assert_eq!(models.len(), 3);
    assert!(models.contains(&"claude-3-opus".to_string()));
    assert!(models.contains(&"claude-3-sonnet".to_string()));
    assert!(models.contains(&"claude-3-haiku".to_string()));
}

#[tokio::test]
async fn test_cache_tokens_handling() {
    let entries = vec![UsageEntry {
        session_id: SessionId::new("s1"),
        timestamp: ISOTimestamp::new(chrono::Utc::now()),
        model: ModelName::new("claude-3-opus"),
        tokens: TokenCounts::new(1000, 500, 200, 100),
        total_cost: None,
        project: None,
        instance_id: None,
    }];

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries_stream = stream::iter(entries.into_iter().map(Ok));
    let daily_data = aggregator
        .aggregate_daily(entries_stream, CostMode::Calculate)
        .await
        .unwrap();

    assert_eq!(daily_data.len(), 1);

    // Verify cache tokens are preserved
    assert_eq!(daily_data[0].tokens.cache_creation_tokens, 200);
    assert_eq!(daily_data[0].tokens.cache_read_tokens, 100);
}

#[tokio::test]
async fn test_billing_block_edge_cases() {
    use ccstat::aggregation::SessionUsage;

    // Create sessions that span exactly 5 hours
    let now = chrono::Utc::now();
    let sessions = vec![
        SessionUsage {
            session_id: SessionId::new("s1"),
            start_time: now - chrono::Duration::hours(5),
            end_time: now - chrono::Duration::hours(4),
            tokens: TokenCounts::new(1_000_000, 500_000, 0, 0),
            total_cost: 25.0,
            model: ModelName::new("claude-3-opus"),
        },
        SessionUsage {
            session_id: SessionId::new("s2"),
            start_time: now - chrono::Duration::hours(3),
            end_time: now - chrono::Duration::hours(2),
            tokens: TokenCounts::new(2_000_000, 1_000_000, 0, 0),
            total_cost: 50.0,
            model: ModelName::new("claude-3-opus"),
        },
        SessionUsage {
            session_id: SessionId::new("s3"),
            start_time: now - chrono::Duration::minutes(30),
            end_time: now,
            tokens: TokenCounts::new(500_000, 250_000, 0, 0),
            total_cost: 12.5,
            model: ModelName::new("claude-3-opus"),
        },
    ];

    let blocks = Aggregator::create_billing_blocks(&sessions);

    // Should have at least one block
    assert!(!blocks.is_empty());

    // Check that the active block contains the recent session
    let active_blocks: Vec<_> = blocks.iter().filter(|b| b.is_active).collect();

    // Since sessions are more than 5 hours apart, there might be no active blocks
    // or one active block depending on the current time
    assert!(active_blocks.len() <= 1);

    // Verify the active block's duration if there is one
    if !active_blocks.is_empty() {
        let duration = active_blocks[0].end_time - active_blocks[0].start_time;
        assert!(duration <= chrono::Duration::hours(5));
    }
}

#[tokio::test]
async fn test_error_handling() {
    use ccstat::error::CcstatError;

    // Test with invalid date filter
    let result = ccstat::cli::parse_date_filter("invalid-date");
    assert!(result.is_err());
    match result {
        Err(CcstatError::InvalidDate(_)) => {}
        _ => panic!("Expected InvalidDate error"),
    }

    // Test with invalid month filter
    let result = ccstat::cli::parse_month_filter("2024-13");
    assert!(result.is_err());
    match result {
        Err(CcstatError::InvalidDate(_)) => {}
        _ => panic!("Expected InvalidDate error"),
    }
}
