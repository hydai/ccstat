//! ccusage - Analyze Claude Code usage data from local JSONL files

use ccusage::{
    aggregation::{Aggregator, Totals},
    cli::{parse_date_filter, parse_month_filter, Cli, Command, McpTransport},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::Result,
    mcp::McpServer,
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
};
use clap::Parser;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ccusage=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle commands
    match cli.command {
        Some(Command::Daily {
            mode,
            json,
            since,
            until,
            instances,
            project,
        }) => {
            info!("Running daily usage report");

            // Initialize components
            let data_loader = DataLoader::new().await?;
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator);

            // Load and filter entries
            let entries = data_loader.load_usage_entries();

            // TODO: Apply date filters
            if let Some(since_str) = &since {
                let _since_date = parse_date_filter(since_str)?;
                // TODO: Filter entries by start date
            }
            if let Some(until_str) = &until {
                let _until_date = parse_date_filter(until_str)?;
                // TODO: Filter entries by end date
            }

            // TODO: Apply project filter
            if let Some(_project_name) = &project {
                // TODO: Filter entries by project
            }

            // TODO: Handle instances flag
            if instances {
                // TODO: Group by instance
            }

            // Aggregate data
            let daily_data = aggregator.aggregate_daily(entries, mode).await?;
            let totals = Totals::from_daily(&daily_data);

            // Format and output
            let formatter = get_formatter(json);
            println!("{}", formatter.format_daily(&daily_data, &totals));
        }

        Some(Command::Monthly {
            mode,
            json,
            since,
            until,
        }) => {
            info!("Running monthly usage report");

            // Initialize components
            let data_loader = DataLoader::new().await?;
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator);

            // Load entries
            let entries = data_loader.load_usage_entries();

            // TODO: Apply month filters
            if let Some(since_str) = &since {
                let _since_month = parse_month_filter(since_str)?;
                // TODO: Filter entries by start month
            }
            if let Some(until_str) = &until {
                let _until_month = parse_month_filter(until_str)?;
                // TODO: Filter entries by end month
            }

            // Aggregate data
            let daily_data = aggregator.aggregate_daily(entries, mode).await?;
            let monthly_data = Aggregator::aggregate_monthly(&daily_data);

            let mut totals = Totals::default();
            for monthly in &monthly_data {
                totals.tokens += monthly.tokens;
                totals.total_cost += monthly.total_cost;
            }

            // Format and output
            let formatter = get_formatter(json);
            println!("{}", formatter.format_monthly(&monthly_data, &totals));
        }

        Some(Command::Session {
            mode,
            json,
            since,
            until,
        }) => {
            info!("Running session usage report");

            // Initialize components
            let data_loader = DataLoader::new().await?;
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator);

            // Load entries
            let entries = data_loader.load_usage_entries();

            // TODO: Apply date filters
            if let Some(since_str) = &since {
                let _since_date = parse_date_filter(since_str)?;
                // TODO: Filter entries by start date
            }
            if let Some(until_str) = &until {
                let _until_date = parse_date_filter(until_str)?;
                // TODO: Filter entries by end date
            }

            // Aggregate data
            let session_data = aggregator.aggregate_sessions(entries, mode).await?;
            let totals = Totals::from_sessions(&session_data);

            // Format and output
            let formatter = get_formatter(json);
            println!("{}", formatter.format_sessions(&session_data, &totals));
        }

        Some(Command::Blocks {
            mode,
            json,
            active,
            recent,
            token_limit,
        }) => {
            info!("Running billing blocks report");

            // Initialize components
            let data_loader = DataLoader::new().await?;
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator);

            // Load entries
            let entries = data_loader.load_usage_entries();

            // Aggregate sessions first
            let session_data = aggregator.aggregate_sessions(entries, mode).await?;

            // Create billing blocks
            let mut blocks = Aggregator::create_billing_blocks(&session_data);

            // Apply filters
            if active {
                blocks.retain(|b| b.is_active);
            }

            if recent {
                let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
                blocks.retain(|b| b.start_time > cutoff);
            }

            // TODO: Apply token limit warnings
            if let Some(_limit_str) = &token_limit {
                // TODO: Parse limit and add warnings
            }

            // Format and output
            let formatter = get_formatter(json);
            println!("{}", formatter.format_blocks(&blocks));
        }

        Some(Command::Mcp { transport, port }) => {
            info!("Starting MCP server");

            let server = McpServer::new().await?;

            match transport {
                McpTransport::Stdio => {
                    server.run_stdio().await?;
                }
                McpTransport::Http => {
                    server.run_http(port).await?;
                }
            }
        }

        None => {
            // Default to daily report
            info!("No command specified, running daily report");

            // Initialize components
            let data_loader = DataLoader::new().await?;
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator);

            // Load entries
            let entries = data_loader.load_usage_entries();

            // Aggregate data
            let daily_data = aggregator
                .aggregate_daily(entries, Default::default())
                .await?;
            let totals = Totals::from_daily(&daily_data);

            // Format and output
            let formatter = get_formatter(false);
            println!("{}", formatter.format_daily(&daily_data, &totals));
        }
    }

    Ok(())
}
