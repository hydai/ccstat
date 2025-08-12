//! Test fixture data and property-based testing
//!
//! This module provides comprehensive fixture data and property-based tests
//! to ensure robustness across various edge cases and data patterns.

mod common;

use ccstat::{
    aggregation::Aggregator,
    timezone::TimezoneConfig,
    types::{CostMode, UsageEntry},
};
use chrono::{Datelike, NaiveDate, Timelike};
use futures::StreamExt;
use proptest::prelude::*;

// Property-based test for UsageEntry builder
proptest! {
    #[test]
    fn test_usage_entry_builder_properties(
        session_id in "[a-z0-9]{8}",
        year in 2020i32..=2025,
        month in 1u32..=12,
        day in 1u32..=28,  // Use 28 to avoid month boundary issues
        hour in 0u32..=23,
        input_tokens in 0u64..=1000000,
        output_tokens in 0u64..=500000,
        cache_creation in 0u64..=100000,
        cache_read in 0u64..=50000,
        cost in 0.0f64..=1000.0,
        project_num in 0usize..4
    ) {
        let projects = ["alpha", "beta", "gamma", "delta"];
        let entry = common::UsageEntryBuilder::new()
            .with_session_id(&session_id)
            .with_date(year, month, day, hour)
            .with_tokens(input_tokens, output_tokens)
            .with_cache_tokens(cache_creation, cache_read)
            .with_cost(cost)
            .with_project(projects[project_num])
            .build();

        // Verify all properties are set correctly
        prop_assert_eq!(entry.session_id.as_str(), session_id);
        prop_assert_eq!(entry.tokens.input_tokens, input_tokens);
        prop_assert_eq!(entry.tokens.output_tokens, output_tokens);
        prop_assert_eq!(entry.tokens.cache_creation_tokens, cache_creation);
        prop_assert_eq!(entry.tokens.cache_read_tokens, cache_read);
        prop_assert_eq!(entry.total_cost, Some(cost));
        prop_assert_eq!(entry.project, Some(projects[project_num].to_string()));

        // Verify date is correct
        let date = entry.timestamp.inner().date_naive();
        prop_assert_eq!(date.year(), year);
        prop_assert_eq!(date.month(), month);
        prop_assert_eq!(date.day(), day);
    }
}

// Property-based test for date range data generation
proptest! {
    #[test]
    fn test_date_range_generation_properties(
        start_day in 1u32..=15,
        end_day in 16u32..=28,
        sessions in 1usize..=10
    ) {
        let start = NaiveDate::from_ymd_opt(2024, 1, start_day).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, end_day).unwrap();
        let entries = common::generate_date_range_data(start, end, sessions);

        // Calculate expected number of entries
        let days = (end - start).num_days() + 1;
        let expected_entries = (days as usize) * sessions;

        prop_assert_eq!(entries.len(), expected_entries);

        // Verify all entries are valid JSONL
        for entry in &entries {
            prop_assert!(entry.contains("\"sessionId\""));
            prop_assert!(entry.contains("\"timestamp\""));
            prop_assert!(entry.contains("\"type\":\"assistant\""));
            prop_assert!(entry.contains("\"model\""));
            prop_assert!(entry.contains("\"usage\""));
        }
    }
}

/// Test fixture: Edge case data patterns
#[tokio::test]
async fn test_edge_case_fixtures() {
    // Test with empty sessions
    let empty_entries = vec![];
    let (_temp_dir, loader) = common::create_test_data_dir(empty_entries).await;

    let entries = loader.load_usage_entries();
    let all_entries: Vec<UsageEntry> = entries
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(all_entries.len(), 0);

    // Test with single entry
    let single_entry = vec![
        common::UsageEntryBuilder::new()
            .with_session_id("single")
            .with_tokens(100, 50)
            .to_jsonl(),
    ];

    let (_temp_dir2, loader2) = common::create_test_data_dir(single_entry).await;
    let entries2 = loader2.load_usage_entries();
    let all_entries2: Vec<UsageEntry> = entries2
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(all_entries2.len(), 1);
    assert_eq!(all_entries2[0].session_id.as_str(), "single");
}

/// Test fixture: Large dataset performance
#[tokio::test]
async fn test_large_dataset_fixture() {
    // Generate a year's worth of data
    let start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();

    // Generate with fewer sessions per day for performance
    let entries = common::generate_date_range_data(start, end, 2);

    let start_time = std::time::Instant::now();
    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    // Test loading performance
    let load_entries = loader.load_usage_entries();
    let all_entries: Vec<UsageEntry> = load_entries
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let duration = start_time.elapsed();

    // Should have 365 days * 2 sessions = 730 entries
    assert_eq!(all_entries.len(), 730);

    // Should process within reasonable time (5 seconds for large dataset)
    assert!(
        duration.as_secs() < 5,
        "Processing took too long: {:?}",
        duration
    );
}

/// Test fixture: Billing block patterns
#[tokio::test]
async fn test_billing_block_fixtures() {
    let entries = common::generate_billing_block_data();
    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    let cost_calculator = common::create_test_environment().await;
    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Load and aggregate by sessions
    let load_entries = loader.load_usage_entries();
    let session_data = aggregator
        .aggregate_sessions(load_entries, CostMode::Auto)
        .await
        .unwrap();

    // Create billing blocks
    let blocks = Aggregator::create_billing_blocks(&session_data);

    // Verify blocks are properly formed
    assert!(!blocks.is_empty());

    for block in &blocks {
        let duration = block.end_time - block.start_time;
        // Each block should be at most 5 hours
        assert!(duration.num_hours() <= 5);

        // Blocks should start at hour boundaries
        assert_eq!(block.start_time.minute(), 0);
        assert_eq!(block.start_time.second(), 0);
    }
}

/// Test fixture: Pattern-based data
#[tokio::test]
async fn test_pattern_fixtures() {
    let entries = common::generate_pattern_data();
    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    let cost_calculator = common::create_test_environment().await;
    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Load and aggregate daily
    let load_entries = loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(load_entries, CostMode::Auto)
        .await
        .unwrap();

    // Should have data for January 15, 2024
    assert_eq!(daily_data.len(), 1);
    let day_data = &daily_data[0];
    assert_eq!(
        *day_data.date.inner(),
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
    );

    // Verify token patterns (morning heavy, afternoon light, night batch)
    assert!(day_data.tokens.input_tokens > 0);

    // Verify model distribution
    assert!(!day_data.models_used.is_empty());
}

/// Test fixture: Invalid data handling
#[tokio::test]
async fn test_invalid_data_fixtures() {
    let entries = common::generate_invalid_data();
    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    // Should handle invalid entries gracefully
    let load_entries = loader.load_usage_entries();
    let all_entries: Vec<UsageEntry> = load_entries
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    // All invalid entries should be filtered out
    assert_eq!(all_entries.len(), 0);
}

// Property-based test for token aggregation
proptest! {
    #[test]
    fn test_token_aggregation_properties(
        entries in prop::collection::vec(
            (100u64..=10000, 50u64..=5000, 10u64..=1000, 5u64..=500),
            1..=100
        )
    ) {
        let mut total_input = 0u64;
        let mut total_output = 0u64;
        let mut total_cache_creation = 0u64;
        let mut total_cache_read = 0u64;

        let usage_entries: Vec<UsageEntry> = entries
            .iter()
            .enumerate()
            .map(|(i, (input, output, cache_creation, cache_read))| {
                total_input += input;
                total_output += output;
                total_cache_creation += cache_creation;
                total_cache_read += cache_read;

                common::UsageEntryBuilder::new()
                    .with_session_id(&format!("session-{}", i))
                    .with_tokens(*input, *output)
                    .with_cache_tokens(*cache_creation, *cache_read)
                    .build()
            })
            .collect();

        // Verify token totals
        common::verify_token_totals(&usage_entries, total_input, total_output);

        // Verify total calculation
        let grand_total: u64 = usage_entries
            .iter()
            .map(|e| e.tokens.total())
            .sum();

        prop_assert_eq!(
            grand_total,
            total_input + total_output + total_cache_creation + total_cache_read
        );
    }
}

/// Test fixture: Timezone-aware data
#[tokio::test]
async fn test_timezone_fixtures() {
    // Generate data that crosses timezone boundaries
    let mut entries = Vec::new();

    // Add entries at UTC midnight (which is different times in other zones)
    for hour in 22..=26 {
        entries.push(
            common::UsageEntryBuilder::new()
                .with_session_id(&format!("tz-test-{}", hour))
                .with_date(2024, 1, if hour >= 24 { 2 } else { 1 }, hour % 24)
                .with_tokens(1000, 500)
                .to_jsonl(),
        );
    }

    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    // Test with different timezones
    let timezones = vec![
        TimezoneConfig::from_cli(None, true).unwrap(), // UTC
        TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap(),
        TimezoneConfig::from_cli(Some("Asia/Tokyo"), false).unwrap(),
    ];

    for tz_config in timezones {
        let cost_calculator = common::create_test_environment().await;
        let aggregator = Aggregator::new(cost_calculator, tz_config);

        let load_entries = loader.load_usage_entries();
        let daily_data = aggregator
            .aggregate_daily(load_entries, CostMode::Auto)
            .await
            .unwrap();

        // Data should be aggregated according to timezone
        assert!(!daily_data.is_empty());
    }
}

/// Test fixture: Concurrent data processing
#[tokio::test]
async fn test_concurrent_fixtures() {
    use tokio::task;

    // Generate test data
    let entries = common::generate_pattern_data();

    // Create multiple data directories concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let entries_clone = entries.clone();
        let handle = task::spawn(async move {
            let (_temp_dir, loader) = common::create_test_data_dir(entries_clone).await;

            // Load and count entries
            let load_entries = loader.load_usage_entries();
            let count = load_entries
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .filter_map(Result::ok)
                .count();

            (i, count)
        });
        handles.push(handle);
    }

    // All tasks should complete successfully
    for handle in handles {
        let (task_id, count) = handle.await.unwrap();
        assert!(count > 0, "Task {} failed to load entries", task_id);
    }
}

/// Test fixture: Memory efficiency
#[tokio::test]
async fn test_memory_efficiency_fixture() {
    // Test that we can handle large datasets without excessive memory usage
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();

    // Generate data with many sessions
    let entries = common::generate_date_range_data(start, end, 50); // 31 * 50 = 1550 entries

    let (_temp_dir, loader) = common::create_test_data_dir(entries).await;

    // Stream processing should handle this efficiently
    let stream = loader.load_usage_entries();
    let count = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter(Result::is_ok)
        .count();

    assert_eq!(count, 1550);
}

/// Test approximate equality helper
#[test]
fn test_approx_eq_fixture() {
    // Test various precision levels
    common::assert_approx_eq(1.0, 1.0, 0.0);
    common::assert_approx_eq(1.0, 1.00001, 0.0001);
    common::assert_approx_eq(100.123, 100.124, 0.01);
    common::assert_approx_eq(-5.5, -5.49, 0.1);
}

#[test]
#[should_panic]
fn test_approx_eq_fixture_fails() {
    common::assert_approx_eq(1.0, 2.0, 0.9); // 1.0 difference > 0.9 tolerance
}
