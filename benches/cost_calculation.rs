use ccstat::{
    cost_calculator::CostCalculator,
    pricing_fetcher::PricingFetcher,
    types::{CostMode, ModelName, ModelPricing, TokenCounts},
};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;

fn create_test_pricing() -> ModelPricing {
    ModelPricing {
        input_cost_per_token: Some(0.00001),
        output_cost_per_token: Some(0.00002),
        cache_creation_input_token_cost: Some(0.000015),
        cache_read_input_token_cost: Some(0.000001),
    }
}

fn benchmark_cost_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cost_calculation");

    // Benchmark direct cost calculation from pricing
    group.bench_function("calculate_from_pricing", |b| {
        let tokens = TokenCounts::new(10000, 5000, 1000, 500);
        let pricing = create_test_pricing();

        b.iter(|| {
            let _cost =
                CostCalculator::calculate_from_pricing(black_box(&tokens), black_box(&pricing));
        });
    });

    // Benchmark cost calculation with different token sizes
    group.bench_function("calculate_small_tokens", |b| {
        let tokens = TokenCounts::new(100, 50, 10, 5);
        let pricing = create_test_pricing();

        b.iter(|| {
            let _cost =
                CostCalculator::calculate_from_pricing(black_box(&tokens), black_box(&pricing));
        });
    });

    group.bench_function("calculate_large_tokens", |b| {
        let tokens = TokenCounts::new(1000000, 500000, 100000, 50000);
        let pricing = create_test_pricing();

        b.iter(|| {
            let _cost =
                CostCalculator::calculate_from_pricing(black_box(&tokens), black_box(&pricing));
        });
    });

    group.finish();
}

fn benchmark_cost_modes(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("cost_modes");
    group.sample_size(10);

    // Pre-create cost calculator
    let pricing_fetcher = runtime.block_on(async { Arc::new(PricingFetcher::new(true).await) });
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));

    // Benchmark Auto mode with pre-calculated cost
    group.bench_function("auto_mode_with_precalculated", |b| {
        let tokens = TokenCounts::new(10000, 5000, 1000, 500);
        let model = ModelName::new("claude-3-opus");
        let pre_calculated = Some(0.25);

        b.iter(|| {
            runtime.block_on(async {
                let _cost = cost_calculator
                    .calculate_with_mode(
                        black_box(&tokens),
                        black_box(&model),
                        black_box(pre_calculated),
                        CostMode::Auto,
                    )
                    .await
                    .unwrap();
            });
        });
    });

    // Benchmark Calculate mode (always calculates)
    group.bench_function("calculate_mode", |b| {
        let tokens = TokenCounts::new(10000, 5000, 1000, 500);
        let model = ModelName::new("claude-3-opus");
        let pre_calculated = Some(0.25);

        b.iter(|| {
            runtime.block_on(async {
                let _cost = cost_calculator
                    .calculate_with_mode(
                        black_box(&tokens),
                        black_box(&model),
                        black_box(pre_calculated),
                        CostMode::Calculate,
                    )
                    .await
                    .unwrap();
            });
        });
    });

    group.finish();
}

fn benchmark_batch_cost_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_cost_calculation");

    // Benchmark calculating costs for multiple entries
    group.bench_function("calculate_100_entries", |b| {
        let pricing = create_test_pricing();
        let entries: Vec<TokenCounts> = (0..100)
            .map(|i| {
                TokenCounts::new(
                    (i * 100) as u64,
                    (i * 50) as u64,
                    (i * 10) as u64,
                    (i * 5) as u64,
                )
            })
            .collect();

        b.iter(|| {
            let mut total_cost = 0.0;
            for tokens in &entries {
                total_cost +=
                    CostCalculator::calculate_from_pricing(black_box(tokens), black_box(&pricing));
            }
            black_box(total_cost)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_cost_calculation,
    benchmark_cost_modes,
    benchmark_batch_cost_calculation
);
criterion_main!(benches);
