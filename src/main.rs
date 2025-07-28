//! ccstat - Analyze Claude Code usage data from local JSONL files

use ccstat::{
    aggregation::{Aggregator, Totals},
    cli::{parse_date_filter, parse_month_filter, Cli, Command, McpTransport},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::Result,
    filters::{MonthFilter, UsageFilter},
    live_monitor::LiveMonitor,
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
                .unwrap_or_else(|_| "ccstat=info".into()),
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
            watch,
            interval,
            parallel,
            intern,
            arena,
        }) => {
            info!("Running daily usage report");

            // Initialize components with progress bars enabled for terminal output
            let show_progress = !json && !watch && atty::is(atty::Stream::Stdout);
            let data_loader = Arc::new(
                DataLoader::new()
                    .await?
                    .with_progress(show_progress)
                    .with_interning(intern)
                    .with_arena(arena)
            );
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(Aggregator::new(cost_calculator).with_progress(show_progress));

            // Build filter
            let mut filter = UsageFilter::new();

            if let Some(since_str) = &since {
                let since_date = parse_date_filter(since_str)?;
                filter = filter.with_since(since_date);
            }
            if let Some(until_str) = &until {
                let until_date = parse_date_filter(until_str)?;
                filter = filter.with_until(until_date);
            }
            if let Some(project_name) = &project {
                filter = filter.with_project(project_name.clone());
            }

            // Check if we're in watch mode
            if watch {
                info!("Starting live monitoring mode");
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator,
                    filter,
                    mode,
                    json,
                    instances,
                    interval,
                );
                monitor.run().await?;
            } else {
                // Handle instances flag
                if instances {
                    // Load and filter entries, then group by instance
                    if parallel {
                        let entries = data_loader.load_usage_entries_parallel();
                        let filtered_entries = filter.filter_stream(entries).await;
                        let instance_data = aggregator.aggregate_daily_by_instance(filtered_entries, mode).await?;
                        let totals = Totals::from_daily_instances(&instance_data);
                        let formatter = get_formatter(json);
                        println!("{}", formatter.format_daily_by_instance(&instance_data, &totals));
                    } else {
                        let entries = data_loader.load_usage_entries();
                        let filtered_entries = filter.filter_stream(entries).await;
                        let instance_data = aggregator.aggregate_daily_by_instance(filtered_entries, mode).await?;
                        let totals = Totals::from_daily_instances(&instance_data);
                        let formatter = get_formatter(json);
                        println!("{}", formatter.format_daily_by_instance(&instance_data, &totals));
                    }
                } else {
                    // Load and filter entries, then aggregate normally
                    if parallel {
                        let entries = data_loader.load_usage_entries_parallel();
                        let filtered_entries = filter.filter_stream(entries).await;
                        let daily_data = aggregator.aggregate_daily(filtered_entries, mode).await?;
                        let totals = Totals::from_daily(&daily_data);
                        let formatter = get_formatter(json);
                        println!("{}", formatter.format_daily(&daily_data, &totals));
                    } else {
                        let entries = data_loader.load_usage_entries();
                        let filtered_entries = filter.filter_stream(entries).await;
                        let daily_data = aggregator.aggregate_daily(filtered_entries, mode).await?;
                        let totals = Totals::from_daily(&daily_data);
                        let formatter = get_formatter(json);
                        println!("{}", formatter.format_daily(&daily_data, &totals));
                    }
                }
            }
        }

        Some(Command::Monthly {
            mode,
            json,
            since,
            until,
        }) => {
            info!("Running monthly usage report");

            // Initialize components with progress bars enabled for terminal output
            let show_progress = !json && atty::is(atty::Stream::Stdout);
            let data_loader = DataLoader::new().await?.with_progress(show_progress);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

            // Build month filter
            let mut month_filter = MonthFilter::new();

            if let Some(since_str) = &since {
                let (year, month) = parse_month_filter(since_str)?;
                month_filter = month_filter.with_since(year, month);
            }
            if let Some(until_str) = &until {
                let (year, month) = parse_month_filter(until_str)?;
                month_filter = month_filter.with_until(year, month);
            }

            // Load entries
            let entries = data_loader.load_usage_entries();

            // Aggregate data
            let daily_data = aggregator.aggregate_daily(entries, mode).await?;
            let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);

            // Apply month filter to aggregated monthly data
            monthly_data.retain(|monthly| {
                // Parse month string (YYYY-MM) to check filter
                if let Ok((year, month)) = monthly
                    .month
                    .split_once('-')
                    .and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?)))
                    .ok_or(())
                {
                    // Create a date for the first day of the month to check filter
                    if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, 1) {
                        return month_filter.matches_date(&date);
                    }
                }
                false
            });

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

            // Initialize components with progress bars enabled for terminal output
            let show_progress = !json && atty::is(atty::Stream::Stdout);
            let data_loader = DataLoader::new().await?.with_progress(show_progress);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

            // Build filter
            let mut filter = UsageFilter::new();

            if let Some(since_str) = &since {
                let since_date = parse_date_filter(since_str)?;
                filter = filter.with_since(since_date);
            }
            if let Some(until_str) = &until {
                let until_date = parse_date_filter(until_str)?;
                filter = filter.with_until(until_date);
            }

            // Load and filter entries
            let entries = data_loader.load_usage_entries();
            let filtered_entries = filter.filter_stream(entries).await;

            // Aggregate data
            let session_data = aggregator
                .aggregate_sessions(filtered_entries, mode)
                .await?;
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

            // Initialize components with progress bars enabled for terminal output
            let show_progress = !json && atty::is(atty::Stream::Stdout);
            let data_loader = DataLoader::new().await?.with_progress(show_progress);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

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

            // Apply token limit warnings
            if let Some(limit_str) = &token_limit {
                // Parse token limit (can be a number or percentage like "80%")
                let (limit_value, is_percentage) = if limit_str.ends_with('%') {
                    let value = limit_str
                        .trim_end_matches('%')
                        .parse::<f64>()
                        .map_err(|_| {
                            ccstat::error::CcstatError::InvalidDate(format!(
                                "Invalid token limit: {limit_str}"
                            ))
                        })?;
                    (value / 100.0, true)
                } else {
                    let value = limit_str.parse::<u64>().map_err(|_| {
                        ccstat::error::CcstatError::InvalidDate(format!(
                            "Invalid token limit: {limit_str}"
                        ))
                    })?;
                    (value as f64, false)
                };

                // Apply warnings to blocks
                for block in &mut blocks {
                    if block.is_active {
                        let total_tokens = block.tokens.total();
                        let threshold = if is_percentage {
                            // Assuming 5-hour block has a typical max of ~10M tokens
                            10_000_000.0 * limit_value
                        } else {
                            limit_value
                        };

                        if total_tokens as f64 >= threshold {
                            block.warning = Some(format!(
                                "⚠️  Block has used {} tokens, exceeding threshold of {}",
                                total_tokens,
                                if is_percentage {
                                    format!(
                                        "{}% (~{:.0} tokens)",
                                        (limit_value * 100.0) as u32,
                                        threshold
                                    )
                                } else {
                                    format!("{} tokens", threshold as u64)
                                }
                            ));
                        } else if total_tokens as f64 >= threshold * 0.8 {
                            block.warning = Some(format!(
                                "⚠️  Block approaching limit: {} tokens used ({}% of threshold)",
                                total_tokens,
                                ((total_tokens as f64 / threshold) * 100.0) as u32
                            ));
                        }
                    }
                }
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

            // Initialize components with progress bars enabled for terminal output
            let show_progress = atty::is(atty::Stream::Stdout);
            let data_loader = DataLoader::new().await?.with_progress(show_progress);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Aggregator::new(cost_calculator).with_progress(show_progress);

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
