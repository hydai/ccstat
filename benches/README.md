# ccusage Performance Benchmarks

This directory contains performance benchmarks for the ccusage library.

## Running Benchmarks

To run all benchmarks:

```bash
cargo bench
```

To run a specific benchmark:

```bash
cargo bench --bench parsing
cargo bench --bench aggregation
cargo bench --bench cost_calculation
```

## Benchmark Suites

### Parsing Benchmarks (`parsing.rs`)

Tests the performance of:
- JSON parsing for single usage entries
- Batch parsing of multiple entries
- Token arithmetic operations
- Date conversion and formatting

### Aggregation Benchmarks (`aggregation.rs`)

Measures the performance of:
- Daily aggregation with 100 and 1000 entries
- Monthly rollup aggregation with 365 days of data
- Instance-based aggregation with 500 entries across multiple instances

### Cost Calculation Benchmarks (`cost_calculation.rs`)

Evaluates:
- Direct cost calculation from pricing data
- Cost calculation with different token sizes (small, medium, large)
- Different cost modes (Auto, Calculate, Display)
- Batch cost calculations for multiple entries

## Benchmark Results

Benchmarks generate HTML reports in `target/criterion/` with detailed performance metrics including:
- Execution time statistics
- Comparison with previous runs
- Performance regression detection
- Visual plots of performance distribution

## Performance Tips

Based on benchmark results:

1. **Batch Operations**: Processing entries in batches is more efficient than individual processing
2. **Pre-calculated Costs**: Using `CostMode::Auto` with pre-calculated costs is significantly faster
3. **Token Arithmetic**: Token addition operations are very fast and scale linearly
4. **Aggregation**: Monthly aggregation is efficient even with a full year of daily data

## Adding New Benchmarks

To add a new benchmark:

1. Create a new file in `benches/` directory
2. Add the benchmark configuration to `Cargo.toml`:
   ```toml
   [[bench]]
   name = "your_benchmark"
   harness = false
   ```
3. Use the criterion framework for consistent benchmark structure