//! ccstat - Analyze Claude Code usage data from local JSONL files

use ccstat::{
    aggregation::{Aggregator, Totals},
    cli::{Cli, Command, parse_date_filter},
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::Result,
    filters::{MonthFilter, UsageFilter},
    live_monitor::{CommandType, LiveMonitor},
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
};
use chrono::Datelike;
use clap::Parser;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Helper function to create an aggregator with timezone configuration
fn create_aggregator_with_timezone(
    cost_calculator: Arc<CostCalculator>,
    show_progress: bool,
    timezone: Option<&str>,
    utc: bool,
) -> Result<Aggregator> {
    let tz_config = TimezoneConfig::from_cli(timezone, utc)?;
    info!("Using timezone: {}", tz_config.display_name());

    Ok(Aggregator::new(cost_calculator, tz_config).with_progress(show_progress))
}

/// Helper function to initialize a DataLoader with common configuration
async fn init_data_loader(show_progress: bool, intern: bool, arena: bool) -> Result<DataLoader> {
    Ok(DataLoader::new()
        .await?
        .with_progress(show_progress)
        .with_interning(intern)
        .with_arena(arena))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Skip logging initialization for statusline command
    let is_statusline = matches!(cli.command, Some(Command::Statusline { .. }));

    if !is_statusline {
        // Initialize logging. Default is quiet (warn level), --verbose enables info level.
        // RUST_LOG environment variable can override these defaults.
        let default_level = if cli.verbose { "ccstat=info" } else { "warn" };
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Handle commands - default to daily if no command specified
    match &cli.command {
        Some(Command::Daily {
            instances,
            detailed,
        }) => {
            info!("Running daily usage report");

            let instances = *instances;
            let detailed = *detailed;

            // Initialize components with progress bars enabled for terminal output
            let show_progress =
                !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout());
            let data_loader =
                Arc::new(init_data_loader(show_progress, cli.intern, cli.arena).await?);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(create_aggregator_with_timezone(
                cost_calculator,
                show_progress,
                cli.timezone.as_deref(),
                cli.utc,
            )?);

            // Build filter
            let mut filter = UsageFilter::new();

            if let Some(since_str) = &cli.since {
                let since_date = parse_date_filter(since_str)?;
                filter = filter.with_since(since_date);
            }
            if let Some(until_str) = &cli.until {
                let until_date = parse_date_filter(until_str)?;
                filter = filter.with_until(until_date);
            }
            if let Some(project_name) = &cli.project {
                filter = filter.with_project(project_name.clone());
            }
            // Apply timezone to filter
            filter = filter.with_timezone(aggregator.timezone_config().tz);

            // Check if we're in watch mode
            if cli.watch {
                info!("Starting live monitoring mode");
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator,
                    filter,
                    None, // No month filter for daily
                    cli.mode,
                    cli.json,
                    CommandType::Daily {
                        instances,
                        detailed,
                    },
                    cli.interval,
                    cli.full_model_names,
                );
                monitor.run().await?;
            } else {
                // Handle instances flag
                if instances {
                    // Load and filter entries, then group by instance
                    let entries = Box::pin(data_loader.load_usage_entries_parallel());
                    let filtered_entries = filter.filter_stream(entries).await;
                    let instance_data = aggregator
                        .aggregate_daily_by_instance(filtered_entries, cli.mode)
                        .await?;
                    let totals = Totals::from_daily_instances(&instance_data);
                    let formatter = get_formatter(cli.json, cli.full_model_names);
                    println!(
                        "{}",
                        formatter.format_daily_by_instance(&instance_data, &totals)
                    );
                } else {
                    // Load and filter entries, then aggregate normally
                    let entries = Box::pin(data_loader.load_usage_entries_parallel());
                    let filtered_entries = filter.filter_stream(entries).await;
                    let daily_data = aggregator
                        .aggregate_daily_detailed(filtered_entries, cli.mode, detailed)
                        .await?;
                    let totals = Totals::from_daily(&daily_data);
                    let formatter = get_formatter(cli.json, cli.full_model_names);
                    println!("{}", formatter.format_daily(&daily_data, &totals));
                }
            }
        }

        Some(Command::Monthly) => {
            info!("Running monthly usage report");

            // Initialize components with progress bars enabled for terminal output
            let show_progress =
                !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout());
            let data_loader =
                Arc::new(init_data_loader(show_progress, cli.intern, cli.arena).await?);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(create_aggregator_with_timezone(
                cost_calculator,
                show_progress,
                cli.timezone.as_deref(),
                cli.utc,
            )?);

            // Build month filter
            let mut month_filter = MonthFilter::new();

            if let Some(since_str) = &cli.since {
                let since_date = parse_date_filter(since_str)?;
                month_filter = month_filter.with_since(since_date.year(), since_date.month());
            }
            if let Some(until_str) = &cli.until {
                let until_date = parse_date_filter(until_str)?;
                month_filter = month_filter.with_until(until_date.year(), until_date.month());
            }

            // Check if we're in watch mode
            if cli.watch {
                info!("Starting live monitoring mode");
                // Create empty filter for monthly (we'll filter after aggregation)
                let filter = UsageFilter::new();
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator,
                    filter,
                    Some(month_filter), // Pass month filter
                    cli.mode,
                    cli.json,
                    CommandType::Monthly,
                    cli.interval,
                    cli.full_model_names,
                );
                monitor.run().await?;
            } else {
                // Load entries and aggregate data
                let entries = Box::pin(data_loader.load_usage_entries_parallel());
                let daily_data = aggregator.aggregate_daily(entries, cli.mode).await?;
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
                let formatter = get_formatter(cli.json, cli.full_model_names);
                println!("{}", formatter.format_monthly(&monthly_data, &totals));
            }
        }

        Some(Command::Session) => {
            info!("Running session usage report");

            // Initialize components with progress bars enabled for terminal output
            let show_progress =
                !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout());
            let data_loader =
                Arc::new(init_data_loader(show_progress, cli.intern, cli.arena).await?);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(create_aggregator_with_timezone(
                cost_calculator,
                show_progress,
                cli.timezone.as_deref(),
                cli.utc,
            )?);

            // Build filter
            let mut filter = UsageFilter::new();

            if let Some(since_str) = &cli.since {
                let since_date = parse_date_filter(since_str)?;
                filter = filter.with_since(since_date);
            }
            if let Some(until_str) = &cli.until {
                let until_date = parse_date_filter(until_str)?;
                filter = filter.with_until(until_date);
            }
            if let Some(project_name) = &cli.project {
                filter = filter.with_project(project_name.clone());
            }
            // Apply timezone to filter
            filter = filter.with_timezone(aggregator.timezone_config().tz);

            // Check if we're in watch mode
            if cli.watch {
                info!("Starting live monitoring mode");
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator.clone(),
                    filter,
                    None, // No month filter for sessions
                    cli.mode,
                    cli.json,
                    CommandType::Session,
                    cli.interval,
                    cli.full_model_names,
                );
                monitor.run().await?;
            } else {
                // Load, filter and aggregate data
                let entries = Box::pin(data_loader.load_usage_entries_parallel());
                let filtered_entries = filter.filter_stream(entries).await;
                let session_data = aggregator
                    .aggregate_sessions(filtered_entries, cli.mode)
                    .await?;
                let totals = Totals::from_sessions(&session_data);

                // Format and output
                let formatter = get_formatter(cli.json, cli.full_model_names);
                println!(
                    "{}",
                    formatter.format_sessions(
                        &session_data,
                        &totals,
                        &aggregator.timezone_config().tz
                    )
                );
            }
        }

        Some(Command::Blocks {
            active,
            recent,
            token_limit,
        }) => {
            info!("Running billing blocks report");

            // Initialize components with progress bars enabled for terminal output
            let show_progress =
                !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout());
            let data_loader =
                Arc::new(init_data_loader(show_progress, cli.intern, cli.arena).await?);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(create_aggregator_with_timezone(
                cost_calculator,
                show_progress,
                cli.timezone.as_deref(),
                cli.utc,
            )?);

            // Check if we're in watch mode
            if cli.watch {
                info!("Starting live monitoring mode");
                let filter = UsageFilter::new(); // No filters for blocks at the entry level
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator,
                    filter,
                    None, // No month filter for blocks
                    cli.mode,
                    cli.json,
                    CommandType::Blocks {
                        active: *active,
                        recent: *recent,
                        token_limit: token_limit.clone(),
                    },
                    cli.interval,
                    cli.full_model_names,
                );
                monitor.run().await?;
            } else {
                // Load entries and aggregate sessions
                let entries = Box::pin(data_loader.load_usage_entries_parallel());
                let session_data = aggregator.aggregate_sessions(entries, cli.mode).await?;

                // Create billing blocks
                let mut blocks = Aggregator::create_billing_blocks(&session_data);

                // Apply filters
                if *active {
                    blocks.retain(|b| b.is_active);
                }

                if *recent {
                    let cutoff = chrono::Utc::now() - chrono::Duration::days(1);
                    blocks.retain(|b| b.start_time > cutoff);
                }

                // Apply token limit warnings
                if let Some(limit_str) = token_limit {
                    // Parse token limit (can be a number or percentage like "80%")
                    let (limit_value, is_percentage) = if limit_str.ends_with('%') {
                        let value =
                            limit_str
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
                let formatter = get_formatter(cli.json, cli.full_model_names);
                println!(
                    "{}",
                    formatter.format_blocks(&blocks, &aggregator.timezone_config().tz)
                );
            }
        }

        Some(Command::Statusline {
            monthly_fee,
            no_color,
            show_date,
            show_git,
        }) => {
            // Run statusline handler
            ccstat::statusline::run(*monthly_fee, *no_color, *show_date, *show_git).await?;
        }

        None => {
            // Default to daily report
            info!("No command specified, running daily report");

            let instances = false;
            let detailed = false;

            // Initialize components with progress bars enabled for terminal output
            let show_progress =
                !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout());
            let data_loader =
                Arc::new(init_data_loader(show_progress, cli.intern, cli.arena).await?);
            let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
            let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
            let aggregator = Arc::new(create_aggregator_with_timezone(
                cost_calculator,
                show_progress,
                cli.timezone.as_deref(),
                cli.utc,
            )?);

            // Build filter
            let mut filter = UsageFilter::new();

            if let Some(since_str) = &cli.since {
                let since_date = parse_date_filter(since_str)?;
                filter = filter.with_since(since_date);
            }
            if let Some(until_str) = &cli.until {
                let until_date = parse_date_filter(until_str)?;
                filter = filter.with_until(until_date);
            }
            if let Some(project_name) = &cli.project {
                filter = filter.with_project(project_name.clone());
            }
            // Apply timezone to filter
            filter = filter.with_timezone(aggregator.timezone_config().tz);

            // Check if we're in watch mode
            if cli.watch {
                info!("Starting live monitoring mode");
                let monitor = LiveMonitor::new(
                    data_loader,
                    aggregator,
                    filter,
                    None, // No month filter for daily
                    cli.mode,
                    cli.json,
                    CommandType::Daily {
                        instances,
                        detailed,
                    },
                    cli.interval,
                    cli.full_model_names,
                );
                monitor.run().await?;
            } else {
                // Load and filter entries, then aggregate normally
                let entries = Box::pin(data_loader.load_usage_entries_parallel());
                let filtered_entries = filter.filter_stream(entries).await;
                let daily_data = aggregator
                    .aggregate_daily_detailed(filtered_entries, cli.mode, detailed)
                    .await?;
                let totals = Totals::from_daily(&daily_data);
                let formatter = get_formatter(cli.json, cli.full_model_names);
                println!("{}", formatter.format_daily(&daily_data, &totals));
            }
        }
    }

    Ok(())
}
