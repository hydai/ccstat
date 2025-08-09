use ccstat::{
    aggregation::{Aggregator, DailyUsage},
    cost_calculator::CostCalculator,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
    types::{CostMode, ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry},
};
use chrono::Utc;
use criterion::{Criterion, criterion_group, criterion_main};
use futures::stream;
use std::hint::black_box;
use std::sync::Arc;

fn create_test_entries(count: usize) -> Vec<UsageEntry> {
    let mut entries = Vec::with_capacity(count);
    let base_time = Utc::now();

    for i in 0..count {
        let hours_ago = (i / 10) as i64;
        let timestamp = base_time - chrono::Duration::hours(hours_ago);

        entries.push(UsageEntry {
            session_id: SessionId::new(format!("session-{i}")),
            timestamp: ISOTimestamp::new(timestamp),
            model: ModelName::new(if i % 3 == 0 {
                "claude-3-opus"
            } else {
                "claude-3-sonnet"
            }),
            tokens: TokenCounts::new(
                (i * 100) as u64,
                (i * 50) as u64,
                (i * 10) as u64,
                (i * 5) as u64,
            ),
            total_cost: Some((i as f64) * 0.01),
            project: Some(format!("project-{}", i % 5)),
            instance_id: Some(format!("instance-{}", i % 3)),
        });
    }

    entries
}

fn benchmark_daily_aggregation(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("daily_aggregation");
    group.sample_size(10);

    // Pre-create components outside the benchmark
    let pricing_fetcher = runtime.block_on(async { Arc::new(PricingFetcher::new(true).await) });
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Benchmark aggregating 100 entries
    group.bench_function("aggregate_100_entries", |b| {
        let entries = create_test_entries(100);

        b.iter(|| {
            let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
            runtime.block_on(async {
                let _result = aggregator
                    .aggregate_daily(entries_stream, CostMode::Auto)
                    .await
                    .unwrap();
            });
        });
    });

    // Benchmark aggregating 1000 entries
    group.bench_function("aggregate_1000_entries", |b| {
        let entries = create_test_entries(1000);

        b.iter(|| {
            let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
            runtime.block_on(async {
                let _result = aggregator
                    .aggregate_daily(entries_stream, CostMode::Auto)
                    .await
                    .unwrap();
            });
        });
    });

    group.finish();
}

fn benchmark_monthly_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("monthly_aggregation");

    // Create daily data spanning multiple months
    let mut daily_data = Vec::new();
    let base_date = chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();

    for i in 0..365 {
        let date = base_date + chrono::Duration::days(i);
        daily_data.push(DailyUsage {
            date: ccstat::types::DailyDate::new(date),
            tokens: TokenCounts::new(1000, 500, 100, 50),
            total_cost: 0.025,
            models_used: vec!["claude-3-opus".to_string()],
            entries: None,
        });
    }

    group.bench_function("aggregate_365_days", |b| {
        b.iter(|| {
            let _result = Aggregator::aggregate_monthly(black_box(&daily_data));
        });
    });

    group.finish();
}

fn benchmark_instance_aggregation(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("instance_aggregation");
    group.sample_size(10);

    // Pre-create components outside the benchmark
    let pricing_fetcher = runtime.block_on(async { Arc::new(PricingFetcher::new(true).await) });
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    // Benchmark aggregating by instance with 500 entries across 5 instances
    group.bench_function("aggregate_500_entries_5_instances", |b| {
        let entries = create_test_entries(500);

        b.iter(|| {
            let entries_stream = stream::iter(entries.clone().into_iter().map(Ok));
            runtime.block_on(async {
                let _result = aggregator
                    .aggregate_daily_by_instance(entries_stream, CostMode::Auto)
                    .await
                    .unwrap();
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_daily_aggregation,
    benchmark_monthly_aggregation,
    benchmark_instance_aggregation
);
criterion_main!(benches);
