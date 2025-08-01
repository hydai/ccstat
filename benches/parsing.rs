use ccstat::types::{ISOTimestamp, ModelName, SessionId, TokenCounts, UsageEntry};
use chrono::Utc;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_test_entry(session_num: u32) -> UsageEntry {
    UsageEntry {
        session_id: SessionId::new(format!("session-{session_num}")),
        timestamp: ISOTimestamp::new(Utc::now()),
        model: ModelName::new("claude-3-opus"),
        tokens: TokenCounts::new(1000, 500, 100, 50),
        total_cost: Some(0.025),
        project: Some("test-project".to_string()),
        instance_id: Some("instance-1".to_string()),
    }
}

#[allow(dead_code)]
fn create_jsonl_file(num_entries: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();

    for i in 0..num_entries {
        let entry = create_test_entry(i as u32);
        let json = serde_json::to_string(&entry).unwrap();
        writeln!(file, "{json}").unwrap();
    }

    file.flush().unwrap();
    file
}

fn benchmark_json_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    // Benchmark parsing a single entry
    group.bench_function("parse_single_entry", |b| {
        let entry = create_test_entry(1);
        let json = serde_json::to_string(&entry).unwrap();

        b.iter(|| {
            let _parsed: UsageEntry = serde_json::from_str(black_box(&json)).unwrap();
        });
    });

    // Benchmark parsing 100 entries
    group.bench_function("parse_100_entries", |b| {
        let mut entries = Vec::new();
        for i in 0..100 {
            let entry = create_test_entry(i);
            entries.push(serde_json::to_string(&entry).unwrap());
        }

        b.iter(|| {
            for json in &entries {
                let _parsed: UsageEntry = serde_json::from_str(black_box(json)).unwrap();
            }
        });
    });

    group.finish();
}

fn benchmark_token_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_arithmetic");

    // Benchmark token addition
    group.bench_function("token_addition", |b| {
        let tokens1 = TokenCounts::new(1000, 500, 100, 50);
        let tokens2 = TokenCounts::new(2000, 1000, 200, 100);

        b.iter(|| {
            let _result = black_box(tokens1) + black_box(tokens2);
        });
    });

    // Benchmark repeated token accumulation
    group.bench_function("token_accumulation_100", |b| {
        b.iter(|| {
            let mut total = TokenCounts::default();
            for _ in 0..100 {
                let tokens = TokenCounts::new(100, 50, 10, 5);
                total += black_box(tokens);
            }
            black_box(total)
        });
    });

    group.finish();
}

fn benchmark_date_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("date_conversion");

    // Benchmark timestamp to daily date conversion
    group.bench_function("timestamp_to_daily_date", |b| {
        let timestamp = ISOTimestamp::new(Utc::now());

        b.iter(|| {
            let _daily = black_box(&timestamp).to_daily_date();
        });
    });

    // Benchmark date formatting
    group.bench_function("date_formatting", |b| {
        let timestamp = ISOTimestamp::new(Utc::now());
        let daily = timestamp.to_daily_date();

        b.iter(|| {
            let _formatted = black_box(&daily).format("%Y-%m-%d");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_json_parsing,
    benchmark_token_arithmetic,
    benchmark_date_conversion
);
criterion_main!(benches);
