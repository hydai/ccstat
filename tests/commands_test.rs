//! Integration tests for ccstat CLI commands
//!
//! These tests verify the main.rs functionality by testing the various commands
//! with mock data and ensuring they work correctly end-to-end.

mod common;

use ccstat::{
    aggregation::Aggregator,
    cli::{parse_date_filter, parse_month_filter},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    filters::UsageFilter,
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
    types::CostMode,
};
use chrono::{Datelike, NaiveDate};
use futures::StreamExt;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test DataLoader with sample JSONL files
async fn create_test_data_loader() -> Option<(DataLoader, TempDir)> {
    // Generate diverse test data using common utilities
    let entries = vec![
        common::UsageEntryBuilder::new()
            .with_session_id("session1")
            .with_date(2024, 1, 1, 10)
            .with_model("claude-3-opus-20240229")
            .with_tokens(1000, 500)
            .with_cache_tokens(100, 50)
            .with_project("project-a")
            .with_cost(0.05)
            .to_jsonl(),
        common::UsageEntryBuilder::new()
            .with_session_id("session1")
            .with_date(2024, 1, 1, 11)
            .with_model("claude-3-opus-20240229")
            .with_tokens(2000, 1000)
            .with_project("project-a")
            .with_cost(0.10)
            .to_jsonl(),
        common::UsageEntryBuilder::new()
            .with_session_id("session2")
            .with_date(2024, 1, 2, 10)
            .with_model("claude-3-sonnet-20240229")
            .with_tokens(500, 250)
            .with_project("project-b")
            .with_cost(0.02)
            .to_jsonl(),
        common::UsageEntryBuilder::new()
            .with_session_id("session3")
            .with_date(2024, 2, 1, 10)
            .with_model("claude-3-haiku-20240307")
            .with_tokens(3000, 1500)
            .with_project("project-c")
            .with_cost(0.01)
            .to_jsonl(),
        common::UsageEntryBuilder::new()
            .with_session_id("session4")
            .with_date(2024, 2, 15, 10)
            .with_model("claude-3-opus-20240229")
            .with_tokens(5000, 2500)
            .with_instance("instance-1")
            .with_cost(0.25)
            .to_jsonl(),
    ];

    // Create test data directory using common utilities
    let (temp_dir, loader) = common::create_test_data_dir(entries).await;
    Some((loader, temp_dir))
}

#[tokio::test]
async fn test_daily_command() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and aggregate daily data
    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await
        .unwrap();

    // Verify we have daily data
    assert!(!daily_data.is_empty());

    // Check that dates are correct using direct date comparisons
    let dates: Vec<_> = daily_data.iter().map(|d| *d.date.inner()).collect();
    assert!(dates.contains(&NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()));
    assert!(dates.contains(&NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()));
    assert!(dates.contains(&NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()));
    assert!(dates.contains(&NaiveDate::from_ymd_opt(2024, 2, 15).unwrap()));

    // Verify token counts
    let jan1_data = daily_data
        .iter()
        .find(|d| *d.date.inner() == NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        .unwrap();
    assert_eq!(jan1_data.tokens.input_tokens, 3000); // 1000 + 2000
    assert_eq!(jan1_data.tokens.output_tokens, 1500); // 500 + 1000
}

#[tokio::test]
async fn test_monthly_command() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and aggregate monthly data
    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await
        .unwrap();
    let monthly_data = Aggregator::aggregate_monthly(&daily_data);

    // Verify we have monthly data
    assert!(!monthly_data.is_empty());

    // Check months
    let months: Vec<_> = monthly_data.iter().map(|m| &m.month).collect();
    assert!(months.contains(&&"2024-01".to_string()));
    assert!(months.contains(&&"2024-02".to_string()));

    // Verify aggregation
    let jan_data = monthly_data.iter().find(|m| m.month == "2024-01").unwrap();
    assert_eq!(jan_data.tokens.input_tokens, 3500); // 1000 + 2000 + 500
    assert_eq!(jan_data.tokens.output_tokens, 1750); // 500 + 1000 + 250
}

#[tokio::test]
async fn test_session_command() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and aggregate session data
    let entries = data_loader.load_usage_entries();
    let session_data = aggregator
        .aggregate_sessions(entries, CostMode::Auto)
        .await
        .unwrap();

    // Verify we have session data
    assert!(!session_data.is_empty());

    // Check sessions
    let session_ids: Vec<_> = session_data.iter().map(|s| s.session_id.as_str()).collect();
    assert!(session_ids.contains(&"session1"));
    assert!(session_ids.contains(&"session2"));
    assert!(session_ids.contains(&"session3"));
    assert!(session_ids.contains(&"session4"));

    // Verify session1 aggregation (has 2 entries)
    let session1 = session_data
        .iter()
        .find(|s| s.session_id.as_str() == "session1")
        .unwrap();
    assert_eq!(session1.tokens.input_tokens, 3000); // 1000 + 2000
    assert_eq!(session1.tokens.output_tokens, 1500); // 500 + 1000
}

#[tokio::test]
async fn test_blocks_command() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and create billing blocks
    let entries = data_loader.load_usage_entries();
    let session_data = aggregator
        .aggregate_sessions(entries, CostMode::Auto)
        .await
        .unwrap();
    let blocks = Aggregator::create_billing_blocks(&session_data);

    // Verify we have billing blocks
    assert!(!blocks.is_empty());

    // Check that blocks are 5 hours long
    for block in &blocks {
        let duration = block.end_time - block.start_time;
        assert!(duration <= chrono::Duration::hours(5));
    }
}

#[tokio::test]
async fn test_date_filter_parsing() {
    // Test valid date formats
    let date = parse_date_filter("2024-01-15").unwrap();
    assert_eq!(date.year(), 2024);
    assert_eq!(date.month(), 1);
    assert_eq!(date.day(), 15);

    // Test invalid date format
    assert!(parse_date_filter("invalid-date").is_err());
    assert!(parse_date_filter("2024-13-01").is_err()); // Invalid month
    assert!(parse_date_filter("2024-02-30").is_err()); // Invalid day
}

#[tokio::test]
async fn test_month_filter_parsing() {
    // Test valid month formats
    let (year, month) = parse_month_filter("2024-01").unwrap();
    assert_eq!(year, 2024);
    assert_eq!(month, 1);

    let (year, month) = parse_month_filter("2024-12").unwrap();
    assert_eq!(year, 2024);
    assert_eq!(month, 12);

    // Test invalid month format
    assert!(parse_month_filter("2024").is_err());
    assert!(parse_month_filter("2024-13").is_err());
    assert!(parse_month_filter("2024-00").is_err());
    assert!(parse_month_filter("invalid").is_err());
}

#[tokio::test]
async fn test_filter_with_project() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let filter = UsageFilter::new().with_project("project-a".to_string());

    // Load and filter entries
    let entries = data_loader.load_usage_entries();
    let filtered_entries: Vec<_> = filter
        .filter_stream(entries)
        .await
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Should only have entries from project-a
    assert_eq!(filtered_entries.len(), 2); // session1 has 2 entries in project-a
    for entry in &filtered_entries {
        assert_eq!(entry.project, Some("project-a".to_string()));
    }
}

#[tokio::test]
async fn test_filter_with_date_range() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let filter = UsageFilter::new()
        .with_since(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        .with_until(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());

    // Load and filter entries
    let entries = data_loader.load_usage_entries();
    let filtered_entries: Vec<_> = filter
        .filter_stream(entries)
        .await
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Should only have January entries
    assert_eq!(filtered_entries.len(), 3); // 2 from session1, 1 from session2
    for entry in &filtered_entries {
        let date = entry.timestamp.as_ref().date_naive();
        assert!(date >= NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        assert!(date <= NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }
}

#[tokio::test]
async fn test_timezone_configuration() {
    let tz_config = TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap();
    assert_eq!(tz_config.display_name(), "America/New_York");

    let tz_config = TimezoneConfig::from_cli(None, true).unwrap();
    assert_eq!(tz_config.display_name(), "UTC");

    // Test invalid timezone
    assert!(TimezoneConfig::from_cli(Some("Invalid/Timezone"), false).is_err());
}

#[tokio::test]
async fn test_cost_modes() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Test all cost modes
    for mode in &[CostMode::Auto, CostMode::Calculate, CostMode::Display] {
        let entries = data_loader.load_usage_entries();
        let daily_data = aggregator.aggregate_daily(entries, *mode).await.unwrap();

        // All modes should produce results
        assert!(!daily_data.is_empty(), "No daily data for mode {:?}", mode);

        // Verify costs are calculated/displayed appropriately
        for day in &daily_data {
            match mode {
                CostMode::Display => {
                    // In display mode, cost should be from the JSONL if available
                    if *day.date.inner() == NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() {
                        // This day has costUSD in the test data
                        assert!(day.total_cost > 0.0);
                    }
                }
                CostMode::Calculate | CostMode::Auto => {
                    // These modes should calculate costs
                    assert!(day.total_cost >= 0.0);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_instance_grouping() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and aggregate by instance
    let entries = data_loader.load_usage_entries();
    let instance_data = aggregator
        .aggregate_daily_by_instance(entries, CostMode::Auto)
        .await
        .unwrap();

    // Verify we have instance data
    assert!(!instance_data.is_empty());

    // Check that we have both default and instance-1
    let instance_ids: Vec<_> = instance_data.iter().map(|i| &i.instance_id).collect();
    assert!(instance_ids.contains(&&"default".to_string()));
    assert!(instance_ids.contains(&&"instance-1".to_string()));

    // Verify instance-1 data
    let instance1 = instance_data
        .iter()
        .find(|i| i.instance_id == "instance-1")
        .unwrap();
    assert_eq!(instance1.tokens.input_tokens, 5000);
    assert_eq!(instance1.tokens.output_tokens, 2500);
}

#[tokio::test]
async fn test_output_formatters() {
    let (data_loader, _temp_dir) = match create_test_data_loader().await {
        Some(result) => result,
        None => {
            println!("Skipping test: Unable to create test data loader");
            return;
        }
    };

    let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load data
    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await
        .unwrap();

    // Test table formatter
    let table_formatter = get_formatter(false, false);
    let totals = ccstat::aggregation::Totals::from_daily(&daily_data);
    let table_output = table_formatter.format_daily(&daily_data, &totals);
    assert!(!table_output.is_empty());
    assert!(table_output.contains("Date")); // Table should have headers

    // Test JSON formatter
    let json_formatter = get_formatter(true, false);
    let json_output = json_formatter.format_daily(&daily_data, &totals);
    assert!(!json_output.is_empty());
    assert!(json_output.contains("\"date\"")); // JSON should have date field
    assert!(json_output.contains("\"tokens\"")); // JSON should have tokens field
}

#[tokio::test]
async fn test_all_cost_modes() {
    // Test parsing from string
    assert_eq!("auto".parse::<CostMode>().unwrap(), CostMode::Auto);
    assert_eq!(
        "calculate".parse::<CostMode>().unwrap(),
        CostMode::Calculate
    );
    assert_eq!("display".parse::<CostMode>().unwrap(), CostMode::Display);

    // Test invalid mode
    assert!("invalid".parse::<CostMode>().is_err());
}
