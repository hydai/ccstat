//! ccstat - Analyze AI coding tool usage data

use ccstat::{
    aggregation::{
        Aggregator, BillingBlockParams, Totals, create_and_filter_billing_blocks,
        filter_monthly_data,
    },
    cli::{
        BlocksArgs, Cli, Command, Provider, Report, WeeklyArgs, is_statusline_command,
        parse_date_filter, parse_weekday, resolve_provider_report, validate_provider_report,
    },
    cost_calculator::CostCalculator,
    data_loader::DataLoader,
    error::{CcstatError, Result},
    filters::{MonthFilter, UsageFilter},
    live_monitor::{CommandType, LiveMonitor},
    output::get_formatter,
    pricing_fetcher::PricingFetcher,
    provider::ProviderDataLoader,
    timezone::TimezoneConfig,
};
use chrono::Datelike;
use clap::Parser;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Approximate maximum tokens for a 5-hour billing block
const APPROX_MAX_TOKENS_PER_BLOCK: f64 = 10_000_000.0;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

async fn init_data_loader(show_progress: bool, intern: bool, arena: bool) -> Result<DataLoader> {
    Ok(DataLoader::new()
        .await?
        .with_progress(show_progress)
        .with_interning(intern)
        .with_arena(arena))
}

fn build_usage_filter(cli: &Cli, aggregator: &Aggregator) -> Result<UsageFilter> {
    let mut filter = UsageFilter::new();
    if let Some(since_str) = &cli.since {
        filter = filter.with_since(parse_date_filter(since_str)?);
    }
    if let Some(until_str) = &cli.until {
        filter = filter.with_until(parse_date_filter(until_str)?);
    }
    if let Some(project_name) = &cli.project {
        filter = filter.with_project(project_name.clone());
    }
    filter = filter.with_timezone(aggregator.timezone_config().tz);
    Ok(filter)
}

fn show_progress(cli: &Cli) -> bool {
    !cli.json && !cli.watch && is_terminal::is_terminal(std::io::stdout())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Skip logging for statusline (it writes to stdout and must be fast)
    if !is_statusline_command(&cli.command) {
        let default_level = if cli.verbose { "ccstat=info" } else { "warn" };
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    match &cli.command {
        // No command → show help
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            return Ok(());
        }

        // Hidden alias: watch → blocks --watch --active
        Some(Command::Watch(args)) => {
            info!("Running live billing block monitor");
            let mut cli_with_watch = cli.clone();
            cli_with_watch.watch = true;
            handle_blocks_command(
                &cli_with_watch,
                &BlocksArgs {
                    active: true,
                    recent: false,
                    token_limit: None,
                    session_duration: 5.0,
                    max_cost: args.max_cost,
                },
            )
            .await?;
        }

        // Stub: MCP server
        Some(Command::Mcp) => {
            return Err(CcstatError::Config(
                "MCP server is not yet implemented. Track progress at https://github.com/hydai/ccstat/issues".into(),
            ));
        }

        // Provider/report commands (includes both explicit provider and shortcuts)
        Some(cmd) => {
            let (provider, report) =
                resolve_provider_report(cmd).expect("Watch and Mcp are handled above");
            validate_provider_report(provider, &report)?;

            dispatch_provider_report(&cli, provider, &report).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Report dispatch
// ---------------------------------------------------------------------------

/// Route a report to the appropriate provider-specific handler
async fn dispatch_provider_report(cli: &Cli, provider: Provider, report: &Report) -> Result<()> {
    match provider {
        Provider::Claude => dispatch_claude_report(cli, report).await,
        Provider::Codex => {
            dispatch_provider_with_loader::<ccstat_provider_codex::DataLoader>(cli, report, "codex")
                .await
        }
        Provider::Opencode => {
            dispatch_provider_with_loader::<ccstat_provider_opencode::DataLoader>(
                cli, report, "opencode",
            )
            .await
        }
        Provider::Amp => {
            dispatch_provider_with_loader::<ccstat_provider_amp::DataLoader>(cli, report, "amp")
                .await
        }
        Provider::Pi => {
            dispatch_provider_with_loader::<ccstat_provider_pi::DataLoader>(cli, report, "pi").await
        }
    }
}

async fn dispatch_claude_report(cli: &Cli, report: &Report) -> Result<()> {
    match report {
        Report::Daily(args) => handle_daily_command(cli, args.instances, args.detailed).await,
        Report::Monthly => handle_monthly_command(cli).await,
        Report::Weekly(args) => handle_weekly_command(cli, args).await,
        Report::Session(_) => handle_session_command(cli).await,
        Report::Blocks(args) => handle_blocks_command(cli, args).await,
        Report::Statusline(args) => {
            ccstat::statusline::run(
                args.monthly_fee,
                args.no_color,
                args.show_date,
                args.show_git,
            )
            .await
        }
    }
}

/// Generic dispatch for non-Claude providers.
///
/// Creates the provider's data loader, feeds entries into the shared
/// aggregation pipeline, and formats output. Watch mode is not supported
/// for non-Claude providers.
async fn dispatch_provider_with_loader<T: ProviderDataLoader>(
    cli: &Cli,
    report: &Report,
    provider_name: &str,
) -> Result<()> {
    info!("Running {} provider report", provider_name);

    let sp = show_progress(cli);
    let data_loader = T::new().await?;
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator =
        create_aggregator_with_timezone(cost_calculator, sp, cli.timezone.as_deref(), cli.utc)?;
    let filter = build_usage_filter(cli, &aggregator)?;

    let entries = data_loader.load_entries();
    let filtered_entries = filter.filter_stream(entries).await;

    match report {
        Report::Daily(args) => {
            if args.instances {
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
                let daily_data = aggregator
                    .aggregate_daily_detailed(filtered_entries, cli.mode, args.detailed)
                    .await?;
                let totals = Totals::from_daily(&daily_data);
                let formatter = get_formatter(cli.json, cli.full_model_names);
                println!("{}", formatter.format_daily(&daily_data, &totals));
            }
        }
        Report::Monthly => {
            let daily_data = aggregator
                .aggregate_daily(filtered_entries, cli.mode)
                .await?;
            let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);
            let mut month_filter = MonthFilter::new();
            if let Some(since_str) = &cli.since {
                let since_date = parse_date_filter(since_str)?;
                month_filter = month_filter.with_since(since_date.year(), since_date.month());
            }
            if let Some(until_str) = &cli.until {
                let until_date = parse_date_filter(until_str)?;
                month_filter = month_filter.with_until(until_date.year(), until_date.month());
            }
            filter_monthly_data(&mut monthly_data, &month_filter);
            let totals = Totals::from_monthly(&monthly_data);
            let formatter = get_formatter(cli.json, cli.full_model_names);
            println!("{}", formatter.format_monthly(&monthly_data, &totals));
        }
        Report::Weekly(args) => {
            let start_of_week = parse_weekday(&args.start_of_week)?;
            let daily_data = aggregator
                .aggregate_daily(filtered_entries, cli.mode)
                .await?;
            let weekly_data = Aggregator::aggregate_weekly(&daily_data, start_of_week);
            let totals = Totals::from_weekly(&weekly_data);
            let formatter = get_formatter(cli.json, cli.full_model_names);
            println!("{}", formatter.format_weekly(&weekly_data, &totals));
        }
        Report::Session(_) => {
            let session_data = aggregator
                .aggregate_sessions(filtered_entries, cli.mode)
                .await?;
            let totals = Totals::from_sessions(&session_data);
            let formatter = get_formatter(cli.json, cli.full_model_names);
            println!(
                "{}",
                formatter.format_sessions(&session_data, &totals, &aggregator.timezone_config().tz)
            );
        }
        _ => {
            return Err(CcstatError::Config(format!(
                "Report type not supported for {} provider",
                provider_name
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

async fn handle_daily_command(cli: &Cli, instances: bool, detailed: bool) -> Result<()> {
    info!("Running daily usage report");

    let sp = show_progress(cli);
    let data_loader = Arc::new(init_data_loader(sp, cli.intern, cli.arena).await?);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(create_aggregator_with_timezone(
        cost_calculator,
        sp,
        cli.timezone.as_deref(),
        cli.utc,
    )?);
    let filter = build_usage_filter(cli, &aggregator)?;

    if cli.watch {
        info!("Starting live monitoring mode");
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            cli.mode,
            cli.json,
            CommandType::Daily {
                instances,
                detailed,
            },
            cli.interval,
            cli.full_model_names,
        );
        monitor.run().await
    } else if instances {
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
        Ok(())
    } else {
        let entries = Box::pin(data_loader.load_usage_entries_parallel());
        let filtered_entries = filter.filter_stream(entries).await;
        let daily_data = aggregator
            .aggregate_daily_detailed(filtered_entries, cli.mode, detailed)
            .await?;
        let totals = Totals::from_daily(&daily_data);
        let formatter = get_formatter(cli.json, cli.full_model_names);
        println!("{}", formatter.format_daily(&daily_data, &totals));
        Ok(())
    }
}

async fn handle_monthly_command(cli: &Cli) -> Result<()> {
    info!("Running monthly usage report");

    let sp = show_progress(cli);
    let data_loader = Arc::new(init_data_loader(sp, cli.intern, cli.arena).await?);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(create_aggregator_with_timezone(
        cost_calculator,
        sp,
        cli.timezone.as_deref(),
        cli.utc,
    )?);

    let mut month_filter = MonthFilter::new();
    if let Some(since_str) = &cli.since {
        let since_date = parse_date_filter(since_str)?;
        month_filter = month_filter.with_since(since_date.year(), since_date.month());
    }
    if let Some(until_str) = &cli.until {
        let until_date = parse_date_filter(until_str)?;
        month_filter = month_filter.with_until(until_date.year(), until_date.month());
    }

    if cli.watch {
        info!("Starting live monitoring mode");
        let filter = UsageFilter::new();
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            Some(month_filter),
            cli.mode,
            cli.json,
            CommandType::Monthly,
            cli.interval,
            cli.full_model_names,
        );
        monitor.run().await
    } else {
        let entries = Box::pin(data_loader.load_usage_entries_parallel());
        let daily_data = aggregator.aggregate_daily(entries, cli.mode).await?;
        let mut monthly_data = Aggregator::aggregate_monthly(&daily_data);
        filter_monthly_data(&mut monthly_data, &month_filter);
        let totals = Totals::from_monthly(&monthly_data);
        let formatter = get_formatter(cli.json, cli.full_model_names);
        println!("{}", formatter.format_monthly(&monthly_data, &totals));
        Ok(())
    }
}

async fn handle_weekly_command(cli: &Cli, args: &WeeklyArgs) -> Result<()> {
    info!("Running weekly usage report");

    let start_of_week = parse_weekday(&args.start_of_week)?;

    let sp = show_progress(cli);
    let data_loader = Arc::new(init_data_loader(sp, cli.intern, cli.arena).await?);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(create_aggregator_with_timezone(
        cost_calculator,
        sp,
        cli.timezone.as_deref(),
        cli.utc,
    )?);

    let mut month_filter = MonthFilter::new();
    if let Some(since_str) = &cli.since {
        let since_date = parse_date_filter(since_str)?;
        month_filter = month_filter.with_since(since_date.year(), since_date.month());
    }
    if let Some(until_str) = &cli.until {
        let until_date = parse_date_filter(until_str)?;
        month_filter = month_filter.with_until(until_date.year(), until_date.month());
    }

    if cli.watch {
        info!("Starting live monitoring mode");
        let filter = UsageFilter::new();
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            Some(month_filter),
            cli.mode,
            cli.json,
            CommandType::Weekly { start_of_week },
            cli.interval,
            cli.full_model_names,
        );
        monitor.run().await
    } else {
        let entries = Box::pin(data_loader.load_usage_entries_parallel());
        let daily_data = aggregator.aggregate_daily(entries, cli.mode).await?;
        let mut weekly_data = Aggregator::aggregate_weekly(&daily_data, start_of_week);

        // Apply month filter to weekly data (filter by week start date)
        weekly_data.retain(|w| {
            if let Ok(date) = parse_date_filter(&w.week) {
                month_filter.matches_date(&date)
            } else {
                false
            }
        });

        let totals = Totals::from_weekly(&weekly_data);
        let formatter = get_formatter(cli.json, cli.full_model_names);
        println!("{}", formatter.format_weekly(&weekly_data, &totals));
        Ok(())
    }
}

async fn handle_session_command(cli: &Cli) -> Result<()> {
    info!("Running session usage report");

    let sp = show_progress(cli);
    let data_loader = Arc::new(init_data_loader(sp, cli.intern, cli.arena).await?);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(create_aggregator_with_timezone(
        cost_calculator,
        sp,
        cli.timezone.as_deref(),
        cli.utc,
    )?);
    let filter = build_usage_filter(cli, &aggregator)?;

    if cli.watch {
        info!("Starting live monitoring mode");
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator.clone(),
            filter,
            None,
            cli.mode,
            cli.json,
            CommandType::Session,
            cli.interval,
            cli.full_model_names,
        );
        monitor.run().await
    } else {
        let entries = Box::pin(data_loader.load_usage_entries_parallel());
        let filtered_entries = filter.filter_stream(entries).await;
        let session_data = aggregator
            .aggregate_sessions(filtered_entries, cli.mode)
            .await?;
        let totals = Totals::from_sessions(&session_data);
        let formatter = get_formatter(cli.json, cli.full_model_names);
        println!(
            "{}",
            formatter.format_sessions(&session_data, &totals, &aggregator.timezone_config().tz)
        );
        Ok(())
    }
}

async fn handle_blocks_command(cli: &Cli, args: &BlocksArgs) -> Result<()> {
    info!("Running billing blocks report");

    let sp = show_progress(cli);
    let data_loader = Arc::new(init_data_loader(sp, cli.intern, cli.arena).await?);
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Arc::new(create_aggregator_with_timezone(
        cost_calculator,
        sp,
        cli.timezone.as_deref(),
        cli.utc,
    )?);
    let filter = build_usage_filter(cli, &aggregator)?;

    if cli.watch {
        info!("Starting live monitoring mode");
        let monitor = LiveMonitor::new(
            data_loader,
            aggregator,
            filter,
            None,
            cli.mode,
            cli.json,
            CommandType::Blocks {
                active: args.active,
                recent: args.recent,
                token_limit: args.token_limit.clone(),
                session_duration: args.session_duration,
            },
            cli.interval,
            cli.full_model_names,
        )
        .with_max_cost(args.max_cost);
        monitor.run().await
    } else {
        let since_date = filter.since_date;
        let until_date = filter.until_date;
        let params = BillingBlockParams {
            data_loader: &data_loader,
            aggregator: &aggregator,
            cost_mode: cli.mode,
            session_duration_hours: args.session_duration,
            project: cli.project.as_deref(),
            since_date,
            until_date,
            active: args.active,
            recent: args.recent,
            token_limit: args.token_limit.as_deref(),
            approx_max_tokens: APPROX_MAX_TOKENS_PER_BLOCK,
        };
        let blocks = create_and_filter_billing_blocks(params).await?;
        let formatter = get_formatter(cli.json, cli.full_model_names);
        println!(
            "{}",
            formatter.format_blocks(&blocks, &aggregator.timezone_config().tz)
        );
        Ok(())
    }
}
