//! Example of exporting ccstat data to JSON
//!
//! This example shows how to aggregate data and export it in JSON format.

use ccstat::{
    Result,
    aggregation::{Aggregator, Totals},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
    types::CostMode,
};
use std::fs;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize components
    let data_loader = DataLoader::new().await?;
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Load and aggregate data
    println!("Loading usage data...");
    let entries = data_loader.load_usage_entries_parallel();

    // Get daily data
    let daily_data = aggregator.aggregate_daily(entries, CostMode::Auto).await?;

    if daily_data.is_empty() {
        println!("No usage data found");
        return Ok(());
    }

    // Calculate totals
    let totals = Totals::from_daily(&daily_data);

    // Export to JSON
    let json_formatter = get_formatter(true, false);
    let json_output = json_formatter.format_daily(&daily_data, &totals);

    // Save to file
    let output_file = "usage_report.json";
    fs::write(output_file, &json_output)?;
    println!("Exported usage data to {output_file}");

    // Also create monthly summary
    let monthly_data = Aggregator::aggregate_monthly(&daily_data);
    let monthly_totals = Totals::from_monthly(&monthly_data);
    let monthly_json = json_formatter.format_monthly(&monthly_data, &monthly_totals);

    let monthly_file = "monthly_summary.json";
    fs::write(monthly_file, &monthly_json)?;
    println!("Exported monthly summary to {monthly_file}");

    // Print summary
    println!("\nSummary:");
    println!("========");
    println!("Total days: {}", daily_data.len());
    println!("Total tokens: {}", totals.tokens.total());
    println!("Total cost: ${:.2}", totals.total_cost);

    Ok(())
}
