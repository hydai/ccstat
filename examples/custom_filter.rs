//! Example of using custom filters with ccstat
//!
//! This example demonstrates how to filter usage data by date range and project.

use ccstat::{
    Result, aggregation::Aggregator, cost_calculator::CostCalculator, data_loader::DataLoader,
    filters::UsageFilter, pricing_fetcher::PricingFetcher, types::CostMode,
};
use chrono::NaiveDate;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize components
    let data_loader = DataLoader::new().await?;
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    // Create filter for January 2024
    let filter = UsageFilter::new()
        .with_since(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        .with_until(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());

    println!("Filtering data for January 2024...\n");

    // Load and filter entries
    let entries = data_loader.load_usage_entries();
    let filtered_entries = filter.filter_stream(entries).await;

    // Aggregate filtered data
    let daily_data = aggregator
        .aggregate_daily(filtered_entries, CostMode::Calculate)
        .await?;

    // Display results
    if daily_data.is_empty() {
        println!("No usage data found for January 2024");
    } else {
        println!("January 2024 Usage:");
        println!("==================");

        for day in &daily_data {
            println!(
                "{}: {} input, {} output tokens",
                day.date.format("%Y-%m-%d"),
                day.tokens.input_tokens,
                day.tokens.output_tokens
            );

            // Show models used
            if !day.models_used.is_empty() {
                println!("  Models: {}", day.models_used.join(", "));
            }
        }

        // Monthly summary
        let total_tokens: u64 = daily_data.iter().map(|d| d.tokens.total()).sum();
        let total_cost: f64 = daily_data.iter().map(|d| d.total_cost).sum();

        println!("\nJanuary 2024 Summary:");
        println!("====================");
        println!("Total tokens: {total_tokens}");
        println!("Total cost: ${total_cost:.2}");
        println!("Active days: {}", daily_data.len());
    }

    Ok(())
}
