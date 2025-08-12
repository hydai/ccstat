//! End-to-end integration tests for ccstat
//!
//! These tests verify complete workflows from data loading through aggregation
//! to final output, ensuring all components work together correctly.

mod common;

use ccstat::{
    aggregation::Aggregator,
    cli::{parse_date_filter, parse_month_filter},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::CcstatError,
    filters::UsageFilter,
    output::get_formatter,
    timezone::TimezoneConfig,
    types::{CostMode, UsageEntry},
};
use chrono::NaiveDate;
use futures::StreamExt;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test environment with sample data using common utilities
async fn setup_test_environment() -> Option<(DataLoader, TempDir, Arc<CostCalculator>)> {
    // Generate test data using common utilities
    let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let end_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
    let entries = common::generate_date_range_data(start_date, end_date, 3);

    // Create test data directory
    let (temp_dir, loader) = common::create_test_data_dir(entries).await;

    // Create cost calculator
    let cost_calculator = common::create_test_environment().await;

    Some((loader, temp_dir, cost_calculator))
}

#[tokio::test]
async fn test_full_month_workflow() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Load all data
    let entries = data_loader.load_usage_entries();
    let all_entries: Vec<UsageEntry> = entries
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Verify we loaded the expected amount of data
    assert!(!all_entries.is_empty());

    // Test daily aggregation
    let daily_stream = futures::stream::iter(all_entries.clone().into_iter().map(Ok));
    let daily_data = aggregator
        .aggregate_daily(daily_stream, CostMode::Auto)
        .await
        .unwrap();

    // Debug: print the number of days we actually got
    if daily_data.len() != 31 {
        eprintln!("Expected 31 days, got {}", daily_data.len());
        eprintln!("Total entries: {}", all_entries.len());
        for day in &daily_data {
            eprintln!("  Day: {}", day.date.format("%Y-%m-%d"));
        }
    }

    assert_eq!(daily_data.len(), 31); // Should have data for all 31 days

    // Verify daily totals increase over time
    let first_day = daily_data
        .iter()
        .find(|d| d.date.format("%Y-%m-%d") == "2024-01-01")
        .unwrap();
    let last_day = daily_data
        .iter()
        .find(|d| d.date.format("%Y-%m-%d") == "2024-01-31")
        .unwrap();
    assert!(last_day.tokens.input_tokens > first_day.tokens.input_tokens);

    // Test monthly aggregation
    let monthly_data = Aggregator::aggregate_monthly(&daily_data);
    assert_eq!(monthly_data.len(), 1); // Should have one month
    assert_eq!(monthly_data[0].month, "2024-01");

    // Test session aggregation
    let session_stream = futures::stream::iter(all_entries.clone().into_iter().map(Ok));
    let session_data = aggregator
        .aggregate_sessions(session_stream, CostMode::Auto)
        .await
        .unwrap();

    assert!(!session_data.is_empty());

    // Test billing blocks
    let blocks = Aggregator::create_billing_blocks(&session_data);
    assert!(!blocks.is_empty());

    // Verify blocks are within 5-hour windows
    for block in &blocks {
        let duration = block.end_time - block.start_time;
        assert!(duration.num_hours() <= 5);
    }
}

#[tokio::test]
async fn test_filtering_workflow() {
    let (data_loader, _temp_dir, _cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Test date range filtering with UTC timezone for deterministic behavior
    let filter = UsageFilter::new()
        .with_timezone(TimezoneConfig::from_cli(None, true).unwrap().tz)
        .with_since(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap())
        .with_until(NaiveDate::from_ymd_opt(2024, 1, 20).unwrap());

    let entries = data_loader.load_usage_entries();
    let filtered_entries: Vec<UsageEntry> = filter
        .filter_stream(entries)
        .await
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    eprintln!(
        "Filtered {} entries for date range Jan 10-20",
        filtered_entries.len()
    );

    // Verify all entries are within the date range
    for entry in &filtered_entries {
        let date = entry.timestamp.as_ref().date_naive();
        assert!(date >= NaiveDate::from_ymd_opt(2024, 1, 10).unwrap());
        assert!(date <= NaiveDate::from_ymd_opt(2024, 1, 20).unwrap());
    }

    // Test project filtering
    let project_filter = UsageFilter::new().with_project("project-alpha".to_string());
    let entries = data_loader.load_usage_entries();
    let project_entries: Vec<UsageEntry> = project_filter
        .filter_stream(entries)
        .await
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Verify all entries are from project-alpha
    for entry in &project_entries {
        assert_eq!(entry.project, Some("project-alpha".to_string()));
    }
}

#[tokio::test]
async fn test_cost_calculation_workflow() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator.clone(),
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Test different cost modes
    for mode in &[CostMode::Auto, CostMode::Calculate, CostMode::Display] {
        let entries = data_loader.load_usage_entries();
        let daily_data = aggregator.aggregate_daily(entries, *mode).await.unwrap();

        // Verify costs are calculated
        for day in &daily_data {
            match mode {
                CostMode::Display => {
                    // Should use costUSD from the data
                    assert!(day.total_cost > 0.0);
                }
                CostMode::Calculate | CostMode::Auto => {
                    // Should calculate based on token counts
                    assert!(day.total_cost >= 0.0);
                }
            }
        }

        // Test that costs increase over the month (due to increasing usage)
        let first_week_cost: f64 = daily_data
            .iter()
            .filter(|d| {
                let day_str = d.date.format("%Y-%m-%d");
                day_str.as_str() >= "2024-01-01" && day_str.as_str() <= "2024-01-07"
            })
            .map(|d| d.total_cost)
            .sum();

        let last_week_cost: f64 = daily_data
            .iter()
            .filter(|d| {
                let day_str = d.date.format("%Y-%m-%d");
                day_str.as_str() >= "2024-01-25" && day_str.as_str() <= "2024-01-31"
            })
            .map(|d| d.total_cost)
            .sum();

        assert!(
            last_week_cost > first_week_cost,
            "Costs should increase over time"
        );
    }
}

#[tokio::test]
async fn test_timezone_aware_aggregation() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Test with different timezones
    let timezones = vec![
        ("UTC", TimezoneConfig::from_cli(None, true).unwrap()),
        (
            "America/New_York",
            TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap(),
        ),
        (
            "Asia/Tokyo",
            TimezoneConfig::from_cli(Some("Asia/Tokyo"), false).unwrap(),
        ),
    ];

    for (tz_name, tz_config) in timezones {
        let aggregator = Aggregator::new(cost_calculator.clone(), tz_config);

        let entries = data_loader.load_usage_entries();
        let daily_data = aggregator
            .aggregate_daily(entries, CostMode::Auto)
            .await
            .unwrap();

        // Should have data regardless of timezone
        assert!(!daily_data.is_empty(), "No data for timezone {}", tz_name);

        // The number of days might vary by 1 due to timezone differences
        assert!(
            daily_data.len() >= 30 && daily_data.len() <= 32,
            "Unexpected number of days for timezone {}: {}",
            tz_name,
            daily_data.len()
        );
    }
}

#[tokio::test]
async fn test_output_format_workflow() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Generate data
    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await
        .unwrap();

    let totals = ccstat::aggregation::Totals::from_daily(&daily_data);

    // Test table formatter
    let table_formatter = get_formatter(false, false);
    let table_output = table_formatter.format_daily(&daily_data, &totals);

    // Verify table output contains expected elements
    assert!(table_output.contains("Date"));
    assert!(table_output.contains("Input"));
    assert!(table_output.contains("Output"));
    assert!(table_output.contains("Cost"));
    assert!(table_output.contains("2024-01")); // Should have January dates

    // Test JSON formatter
    let json_formatter = get_formatter(true, false);
    let json_output = json_formatter.format_daily(&daily_data, &totals);

    // Verify JSON is valid and contains expected fields
    let parsed: serde_json::Value =
        serde_json::from_str(&json_output).expect("Output should be valid JSON");

    assert!(parsed["daily"].is_array());
    assert!(parsed["totals"].is_object());
    assert!(parsed["totals"]["total_cost"].is_number());

    // Test with full model names
    let full_name_formatter = get_formatter(false, true);
    let full_name_output = full_name_formatter.format_daily(&daily_data, &totals);

    // Should contain full model names
    assert!(full_name_output.contains("claude-3") || full_name_output.contains("Models"));
}

#[tokio::test]
async fn test_instance_grouping_workflow() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Test instance-based aggregation
    let entries = data_loader.load_usage_entries();
    let instance_data = aggregator
        .aggregate_daily_by_instance(entries, CostMode::Auto)
        .await
        .unwrap();

    // Should have multiple instances
    assert!(!instance_data.is_empty());

    // Count unique instances
    let unique_instances: std::collections::HashSet<_> =
        instance_data.iter().map(|d| &d.instance_id).collect();

    // Should have at least "default" and one other instance
    assert!(
        unique_instances.len() >= 2,
        "Should have multiple instances"
    );
    assert!(unique_instances.contains(&"default".to_string()));

    // Verify totals calculation
    let totals = ccstat::aggregation::Totals::from_daily_instances(&instance_data);
    assert!(totals.total_cost > 0.0);
    assert!(totals.tokens.input_tokens > 0);
}

#[tokio::test]
async fn test_performance_with_large_dataset() {
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };

    // Use UTC for consistent test behavior across timezones
    let aggregator = Aggregator::new(
        cost_calculator,
        TimezoneConfig::from_cli(None, true).unwrap(), // Use UTC
    );

    // Measure performance of loading and aggregating
    let start = std::time::Instant::now();

    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await
        .unwrap();

    let duration = start.elapsed();

    // Should complete reasonably quickly (under 1 second for test data)
    assert!(
        duration.as_secs() < 1,
        "Aggregation took too long: {:?}",
        duration
    );

    // Verify results are correct
    assert_eq!(daily_data.len(), 31);
}

#[tokio::test]
async fn test_error_handling_workflow() {
    // Test with invalid timezone
    let result = TimezoneConfig::from_cli(Some("Invalid/Timezone"), false);
    assert!(result.is_err());

    // Test with invalid date filters
    let result = parse_date_filter("not-a-date");
    assert!(result.is_err());

    let result = parse_month_filter("2024-13"); // Invalid month
    assert!(result.is_err());

    // Test with non-existent data directory - use mutex for thread safety
    {
        let _lock = common::ENV_MUTEX.lock().await;

        // Note: env functions are unsafe in Rust 1.82+ due to thread-safety concerns
        // We use a mutex to ensure thread safety, but the functions still require unsafe blocks
        unsafe {
            std::env::set_var("CLAUDE_DATA_PATH", "/nonexistent/path");
        }

        let loader_result = DataLoader::new().await;
        // DataLoader might create the directory or use fallback paths
        // We just want to ensure it handles the non-existent path gracefully
        match loader_result {
            Ok(loader) => {
                // If it succeeds, verify it has some valid paths
                assert!(!loader.paths().is_empty());
            }
            Err(e) => {
                // If it fails, it should be because no Claude directory was found
                assert!(matches!(e, CcstatError::NoClaudeDirectory));
            }
        }

        unsafe {
            std::env::remove_var("CLAUDE_DATA_PATH");
        }
    }
}
