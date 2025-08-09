//! Performance benchmarks for ccstat
//!
//! Run with: cargo bench

use ccstat::{
    aggregation::Aggregator,
    cost_calculator::CostCalculator,
    pricing_fetcher::PricingFetcher,
    timezone::TimezoneConfig,
    types::{CostMode, ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry},
};
use chrono::{TimeZone, Utc};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::stream;
use std::hint::black_box;
use std::sync::Arc;

/// Generate test usage entries
fn generate_usage_entries(count: usize) -> Vec<UsageEntry> {
    let models = [
        "claude-3-opus",
        "claude-3-sonnet",
        "claude-3-haiku",
        "claude-3.5-sonnet",
    ];

    (0..count)
        .map(|i| {
            let timestamp = Utc
                .timestamp_opt(1704067200 + (i as i64 * 3600), 0)
                .unwrap();
            let model_idx = i % models.len();

            UsageEntry {
                session_id: SessionId::new(format!("session-{}", i / 10)),
                timestamp: ISOTimestamp::new(timestamp),
                model: ModelName::new(models[model_idx]),
                tokens: TokenCounts::new(
                    (i * 100) as u64,
                    (i * 50) as u64,
                    (i * 10) as u64,
                    (i * 5) as u64,
                ),
                total_cost: None,
                project: Some("benchmark-project".to_string()),
                instance_id: Some(format!("instance-{}", i % 5)),
            }
        })
        .collect()
}

fn benchmark_aggregation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Pre-create the aggregator outside the benchmark
    let pricing_fetcher = rt.block_on(async { Arc::new(PricingFetcher::new(false).await) });
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator, TimezoneConfig::default());

    let mut group = c.benchmark_group("aggregation");

    // Benchmark small dataset (100 entries)
    group.bench_function("daily_100_entries", |b| {
        b.iter_batched(
            || generate_usage_entries(100),
            |entries| {
                rt.block_on(async {
                    let stream = stream::iter(entries.into_iter().map(Ok));
                    aggregator
                        .aggregate_daily(stream, CostMode::Calculate)
                        .await
                        .unwrap()
                })
            },
            BatchSize::SmallInput,
        )
    });

    // Benchmark medium dataset (1,000 entries)
    group.bench_function("daily_1k_entries", |b| {
        b.iter_batched(
            || generate_usage_entries(1_000),
            |entries| {
                rt.block_on(async {
                    let stream = stream::iter(entries.into_iter().map(Ok));
                    aggregator
                        .aggregate_daily(stream, CostMode::Calculate)
                        .await
                        .unwrap()
                })
            },
            BatchSize::SmallInput,
        )
    });

    // Benchmark large dataset (10,000 entries)
    group.bench_function("daily_10k_entries", |b| {
        b.iter_batched(
            || generate_usage_entries(10_000),
            |entries| {
                rt.block_on(async {
                    let stream = stream::iter(entries.into_iter().map(Ok));
                    aggregator
                        .aggregate_daily(stream, CostMode::Calculate)
                        .await
                        .unwrap()
                })
            },
            BatchSize::LargeInput,
        )
    });

    // Benchmark session aggregation
    group.bench_function("session_1k_entries", |b| {
        b.iter_batched(
            || generate_usage_entries(1_000),
            |entries| {
                rt.block_on(async {
                    let stream = stream::iter(entries.into_iter().map(Ok));
                    aggregator
                        .aggregate_sessions(stream, CostMode::Calculate)
                        .await
                        .unwrap()
                })
            },
            BatchSize::SmallInput,
        )
    });

    // Benchmark monthly aggregation from daily data
    group.bench_function("monthly_from_daily", |b| {
        let daily_data = rt.block_on(async {
            let entries = generate_usage_entries(1_000);
            let stream = stream::iter(entries.into_iter().map(Ok));
            aggregator
                .aggregate_daily(stream, CostMode::Calculate)
                .await
                .unwrap()
        });

        b.iter(|| black_box(Aggregator::aggregate_monthly(&daily_data)))
    });

    group.finish();
}

fn benchmark_cost_calculation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let pricing_fetcher = rt.block_on(async { Arc::new(PricingFetcher::new(false).await) });
    let calculator = CostCalculator::new(pricing_fetcher);

    let mut group = c.benchmark_group("cost_calculation");

    // Benchmark individual cost calculation
    group.bench_function("single_calculation", |b| {
        let tokens = TokenCounts::new(1000, 500, 100, 50);
        let model = ModelName::new("claude-3-opus");

        b.iter(|| rt.block_on(async { calculator.calculate_cost(&tokens, &model).await.unwrap() }))
    });

    // Benchmark batch cost calculation
    group.bench_function("batch_100_calculations", |b| {
        let calculations: Vec<_> = (0..100)
            .map(|i| {
                (
                    TokenCounts::new(i * 100, i * 50, i * 10, i * 5),
                    ModelName::new("claude-3-opus"),
                )
            })
            .collect();

        b.iter(|| {
            rt.block_on(async {
                for (tokens, model) in &calculations {
                    black_box(calculator.calculate_cost(tokens, model).await.unwrap());
                }
            })
        })
    });

    group.finish();
}

fn benchmark_token_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_arithmetic");

    // Benchmark token addition
    group.bench_function("token_addition", |b| {
        let t1 = TokenCounts::new(1000, 500, 100, 50);
        let t2 = TokenCounts::new(2000, 1000, 200, 100);

        b.iter(|| black_box(t1 + t2))
    });

    // Benchmark accumulated token addition (simulating aggregation)
    group.bench_function("accumulated_addition_1k", |b| {
        let tokens: Vec<_> = (0..1000)
            .map(|i| TokenCounts::new(i, i / 2, i / 10, i / 20))
            .collect();

        b.iter(|| {
            tokens
                .iter()
                .fold(TokenCounts::default(), |acc, t| acc + *t)
        })
    });

    group.finish();
}

fn benchmark_json_parsing(c: &mut Criterion) {
    use ccstat::types::RawJsonlEntry;

    let mut group = c.benchmark_group("json_parsing");

    // Create a sample JSON line
    let json_line = r#"{"sessionId":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2024-01-01T10:00:00Z","message":{"model":"claude-3-opus","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}},"type":"assistant","uuid":"123e4567-e89b-12d3-a456-426614174000","cwd":"/home/user/project"}"#;

    // Benchmark single line parsing
    group.bench_function("parse_single_line", |b| {
        b.iter(|| {
            let raw: RawJsonlEntry = serde_json::from_str(black_box(json_line)).unwrap();
            black_box(UsageEntry::from_raw(raw))
        })
    });

    // Benchmark batch parsing
    group.bench_function("parse_100_lines", |b| {
        let lines: Vec<_> = (0..100).map(|_| json_line).collect();

        b.iter(|| {
            for line in &lines {
                let raw: RawJsonlEntry = serde_json::from_str(black_box(line)).unwrap();
                black_box(UsageEntry::from_raw(raw));
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_aggregation,
    benchmark_cost_calculation,
    benchmark_token_arithmetic,
    benchmark_json_parsing
);
criterion_main!(benches);
