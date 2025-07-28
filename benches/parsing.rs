use ccusage::types::{TokenCounts, UsageEntry};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_jsonl_parsing(c: &mut Criterion) {
    let json_line = r#"{"session_id":"test123","timestamp":"2024-01-01T00:00:00Z","model":"claude-3-opus","input_tokens":1000,"output_tokens":500,"cache_creation_tokens":100,"cache_read_tokens":50,"total_cost":0.025}"#;

    c.bench_function("parse single jsonl entry", |b| {
        b.iter(|| {
            let _entry: UsageEntry = serde_json::from_str(black_box(json_line)).unwrap();
        })
    });
}

fn benchmark_token_arithmetic(c: &mut Criterion) {
    let tokens1 = TokenCounts::new(1000, 500, 100, 50);
    let tokens2 = TokenCounts::new(2000, 1000, 200, 100);

    c.bench_function("token counts addition", |b| {
        b.iter(|| {
            let _sum = black_box(tokens1) + black_box(tokens2);
        })
    });
}

criterion_group!(benches, benchmark_jsonl_parsing, benchmark_token_arithmetic);
criterion_main!(benches);
