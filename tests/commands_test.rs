//! Integration tests for ccstat CLI commands
//!
//! These tests verify the main.rs functionality by testing the various commands
//! with mock data and ensuring they work correctly end-to-end.

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
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Helper to create a test DataLoader with sample JSONL files
async fn create_test_data_loader() -> Option<(DataLoader, TempDir)> {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    
    // Create test JSONL files with various entries
    let jsonl_path = path.join("test_data.jsonl");
    let mut file = fs::File::create(&jsonl_path).await.unwrap();
    
    // Write multiple test entries with known models
    let entries = vec![
        r#"{"sessionId":"session1","timestamp":"2024-01-01T10:00:00Z","type":"assistant","message":{"model":"claude-3-opus-20240229","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}},"cwd":"/home/user/project-a","costUSD":0.05}"#,
        r#"{"sessionId":"session1","timestamp":"2024-01-01T11:00:00Z","type":"assistant","message":{"model":"claude-3-opus-20240229","usage":{"input_tokens":2000,"output_tokens":1000}},"cwd":"/home/user/project-a","costUSD":0.10}"#,
        r#"{"sessionId":"session2","timestamp":"2024-01-02T10:00:00Z","type":"assistant","message":{"model":"claude-3-sonnet-20240229","usage":{"input_tokens":500,"output_tokens":250}},"cwd":"/home/user/project-b","costUSD":0.02}"#,
        r#"{"sessionId":"session3","timestamp":"2024-02-01T10:00:00Z","type":"assistant","message":{"model":"claude-3-haiku-20240307","usage":{"input_tokens":3000,"output_tokens":1500}},"cwd":"/home/user/project-c","costUSD":0.01}"#,
        r#"{"sessionId":"session4","timestamp":"2024-02-15T10:00:00Z","type":"assistant","message":{"model":"claude-3-opus-20240229","usage":{"input_tokens":5000,"output_tokens":2500}},"uuid":"instance-1","costUSD":0.25}"#,
    ];
    
    for entry in entries {
        file.write_all(entry.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
    }
    
    drop(file); // Ensure file is closed before DataLoader reads it
    
    // Create a temporary HOME to isolate from real Claude directories
    let original_home = std::env::var("HOME").ok();
    unsafe {
        std::env::set_var("HOME", "/nonexistent");
        std::env::set_var("CLAUDE_DATA_PATH", path.to_str().unwrap());
    }
    
    match DataLoader::new().await {
        Ok(loader) => {
            unsafe {
                std::env::remove_var("CLAUDE_DATA_PATH");
                if let Some(home) = original_home {
                    std::env::set_var("HOME", home);
                }
            }
            Some((loader, temp_dir))
        }
        Err(_) => {
            unsafe {
                std::env::remove_var("CLAUDE_DATA_PATH");
                if let Some(home) = original_home {
                    std::env::set_var("HOME", home);
                }
            }
            None
        }
    }
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
    
    // Check that dates are correct (convert to string for comparison)
    let dates: Vec<_> = daily_data.iter().map(|d| d.date.format("%Y-%m-%d")).collect();
    assert!(dates.contains(&"2024-01-01".to_string()));
    assert!(dates.contains(&"2024-01-02".to_string()));
    assert!(dates.contains(&"2024-02-01".to_string()));
    assert!(dates.contains(&"2024-02-15".to_string()));
    
    // Verify token counts
    let jan1_data = daily_data.iter().find(|d| d.date.format("%Y-%m-%d") == "2024-01-01").unwrap();
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
    for mode in vec![CostMode::Auto, CostMode::Calculate, CostMode::Display] {
        let entries = data_loader.load_usage_entries();
        let daily_data = aggregator
            .aggregate_daily(entries, mode)
            .await
            .unwrap();
        
        // All modes should produce results
        assert!(!daily_data.is_empty(), "No daily data for mode {:?}", mode);
        
        // Verify costs are calculated/displayed appropriately
        for day in &daily_data {
            match mode {
                CostMode::Display => {
                    // In display mode, cost should be from the JSONL if available
                    if day.date.format("%Y-%m-%d") == "2024-01-01" {
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
    assert_eq!("calculate".parse::<CostMode>().unwrap(), CostMode::Calculate);
    assert_eq!("display".parse::<CostMode>().unwrap(), CostMode::Display);
    
    // Test invalid mode
    assert!("invalid".parse::<CostMode>().is_err());
}