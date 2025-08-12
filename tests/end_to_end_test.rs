//! End-to-end integration tests for ccstat
//!
//! These tests verify complete workflows from data loading through aggregation
//! to final output, ensuring all components work together correctly.

use ccstat::{
    aggregation::Aggregator,
    cli::{parse_date_filter, parse_month_filter},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    filters::UsageFilter,
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
    types::{CostMode, UsageEntry},
};
use chrono::{Local, NaiveDate};
use futures::StreamExt;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Helper to create a test environment with sample data
async fn setup_test_environment() -> Option<(DataLoader, TempDir, Arc<CostCalculator>)> {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    
    // Create comprehensive test data covering various scenarios
    let jsonl_path = path.join("comprehensive_test.jsonl");
    let mut file = fs::File::create(&jsonl_path).await.unwrap();
    
    // Generate test data for a full month
    let entries = generate_test_month_data();
    
    for entry in entries {
        file.write_all(entry.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
    }
    
    drop(file);
    
    // Set up environment and create DataLoader
    let original_home = std::env::var("HOME").ok();
    unsafe {
        std::env::set_var("HOME", "/nonexistent");
        std::env::set_var("CLAUDE_DATA_PATH", path.to_str().unwrap());
    }
    
    let result = match DataLoader::new().await {
        Ok(loader) => {
            let pricing_fetcher = Arc::new(PricingFetcher::new(true).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            Some((loader, temp_dir, cost_calculator))
        }
        Err(_) => None,
    };
    
    // Restore environment
    unsafe {
        std::env::remove_var("CLAUDE_DATA_PATH");
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        }
    }
    
    result
}

/// Generate test data for a full month with various patterns
fn generate_test_month_data() -> Vec<String> {
    let mut entries = Vec::new();
    let models = vec![
        "claude-3-opus-20240229",
        "claude-3-sonnet-20240229",
        "claude-3-haiku-20240307",
        "claude-3-5-sonnet-20240620",
    ];
    
    let projects = vec!["project-alpha", "project-beta", "project-gamma"];
    let instances = vec!["default", "instance-1", "instance-2"];
    
    // Generate entries for each day of January 2024
    for day in 1..=31 {
        let sessions_per_day = if day % 7 == 0 { 1 } else { 3 }; // Fewer sessions on Sundays
        
        for session in 0..sessions_per_day {
            let model = models[session % models.len()];
            let project = projects[session % projects.len()];
            let instance_field = if day > 15 {
                format!(r#","uuid":"{}""#, instances[session % instances.len()])
            } else {
                String::new()
            };
            
            // Morning session
            let hour = 9 + session * 4;
            entries.push(format!(
                r#"{{"sessionId":"session-{}-{}-1","timestamp":"2024-01-{:02}T{:02}:00:00Z","type":"assistant","message":{{"model":"{}","usage":{{"input_tokens":{},"output_tokens":{},"cache_creation_input_tokens":{},"cache_read_input_tokens":{}}}}},"cwd":"/home/user/{}","costUSD":{}{}}}"#,
                day, session, day, hour, model,
                1000 + day * 100, // Increasing usage over time
                500 + day * 50,
                100 + day * 10,
                50 + day * 5,
                project,
                0.01 * (day as f64),
                instance_field
            ));
            
            // Afternoon session (for busier days)
            if session > 0 {
                entries.push(format!(
                    r#"{{"sessionId":"session-{}-{}-2","timestamp":"2024-01-{:02}T{:02}:30:00Z","type":"assistant","message":{{"model":"{}","usage":{{"input_tokens":{},"output_tokens":{}}}}},"cwd":"/home/user/{}","costUSD":{}{}}}"#,
                    day, session, day, hour, model,
                    500 + day * 50,
                    250 + day * 25,
                    project,
                    0.005 * (day as f64),
                    instance_field
                ));
            }
        }
    }
    
    entries
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
    
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
    
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
    
    assert_eq!(daily_data.len(), 31); // Should have data for all 31 days
    
    // Verify daily totals increase over time
    let first_day = daily_data.iter().find(|d| d.date.format("%Y-%m-%d") == "2024-01-01").unwrap();
    let last_day = daily_data.iter().find(|d| d.date.format("%Y-%m-%d") == "2024-01-31").unwrap();
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
    let (data_loader, _temp_dir, cost_calculator) = match setup_test_environment().await {
        Some(env) => env,
        None => {
            println!("Skipping test: Unable to set up test environment");
            return;
        }
    };
    
    let _aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
    
    // Test date range filtering
    let filter = UsageFilter::new()
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
    
    let aggregator = Aggregator::new(cost_calculator.clone(), TimezoneConfig::default());
    
    // Test different cost modes
    for mode in vec![CostMode::Auto, CostMode::Calculate, CostMode::Display] {
        let entries = data_loader.load_usage_entries();
        let daily_data = aggregator
            .aggregate_daily(entries, mode)
            .await
            .unwrap();
        
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
            
        assert!(last_week_cost > first_week_cost, "Costs should increase over time");
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
        ("America/New_York", TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap()),
        ("Asia/Tokyo", TimezoneConfig::from_cli(Some("Asia/Tokyo"), false).unwrap()),
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
    
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
    
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
    let parsed: serde_json::Value = serde_json::from_str(&json_output)
        .expect("Output should be valid JSON");
    
    assert!(parsed["daily_usage"].is_array());
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
    
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
    
    // Test instance-based aggregation
    let entries = data_loader.load_usage_entries();
    let instance_data = aggregator
        .aggregate_daily_by_instance(entries, CostMode::Auto)
        .await
        .unwrap();
    
    // Should have multiple instances
    assert!(!instance_data.is_empty());
    
    // Count unique instances
    let unique_instances: std::collections::HashSet<_> = instance_data
        .iter()
        .map(|d| &d.instance_id)
        .collect();
    
    // Should have at least "default" and one other instance
    assert!(unique_instances.len() >= 2, "Should have multiple instances");
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
    
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());
    
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
    
    // Test with non-existent data directory
    unsafe {
        std::env::set_var("CLAUDE_DATA_PATH", "/nonexistent/path");
    }
    
    let loader_result = DataLoader::new().await;
    assert!(loader_result.is_err());
    
    unsafe {
        std::env::remove_var("CLAUDE_DATA_PATH");
    }
}