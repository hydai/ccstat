//! Basic usage example for ccstat library
//! 
//! This example shows how to use ccstat as a library to analyze Claude usage data.

use ccstat::{
    aggregation::Aggregator,
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    pricing_fetcher::PricingFetcher,
    types::CostMode,
    Result,
};
use futures::StreamExt;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create data loader
    let data_loader = DataLoader::new().await?;
    println!("Found Claude data directories");

    // Set up pricing and cost calculation
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    // Load usage entries
    let entries = data_loader.load_usage_entries();

    // Aggregate by day
    println!("\nDaily Usage Summary:");
    println!("====================");
    
    let daily_data = aggregator
        .aggregate_daily(entries, CostMode::Auto)
        .await?;

    for day in &daily_data {
        println!(
            "{}: {} tokens, ${:.2}",
            day.date.format("%Y-%m-%d"),
            day.tokens.total(),
            day.total_cost
        );
    }

    // Calculate totals
    let total_tokens: u64 = daily_data.iter().map(|d| d.tokens.total()).sum();
    let total_cost: f64 = daily_data.iter().map(|d| d.total_cost).sum();

    println!("\nTotal Usage:");
    println!("============");
    println!("Tokens: {}", total_tokens);
    println!("Cost: ${:.2}", total_cost);

    Ok(())
}