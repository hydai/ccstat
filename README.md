# ccstat

[![Crates.io](https://img.shields.io/crates/v/ccstat.svg)](https://crates.io/crates/ccstat)
[![Docker Hub](https://img.shields.io/docker/v/hydai/ccstat?label=docker&sort=semver)](https://hub.docker.com/r/hydai/ccstat)

Analyze Claude Code usage data from local JSONL files.

## Overview

ccstat is a high-performance Rust CLI tool that processes Claude Code usage logs, calculates costs using LiteLLM pricing data, and provides various reporting views including daily, monthly, session-based, and 5-hour billing block reports.

This project is inspired by [ccusage](https://github.com/ryoppippi/ccusage) and is a Rust reimplementation (RIIR - Rewrite It In Rust) of the original TypeScript tool, offering:
- 50-70% reduction in memory usage
- 2-3x faster processing speed
- Zero memory leaks through RAII
- Better error handling and recovery

## Features

- 📊 **Multiple Report Types**: Daily, monthly, session, and billing block views
- 💰 **Accurate Cost Calculation**: Uses latest LiteLLM pricing data with offline fallback
- 🔍 **Automatic Discovery**: Finds Claude data directories across platforms
- 📈 **Flexible Output**: Table format for humans, JSON for machines
- 🚀 **High Performance**: Stream processing with minimal memory footprint
- 🔌 **MCP Server**: JSON-RPC API for tool integrations
- 👀 **Live Monitoring**: Real-time usage tracking with auto-refresh
- ⚡ **Performance Options**: Parallel processing, string interning, arena allocation
- 🔧 **Advanced Filtering**: By date, project, instance, and more

## Installation

### From crates.io

The easiest way to install ccstat is using cargo:

```bash
cargo install ccstat
```

### From Source

```bash
# Clone the repository
git clone https://github.com/hydai/ccstat
cd ccstat

# Build and install
cargo install --path .
```

### Pre-built Binaries

Download the latest release for your platform from the [releases page](https://github.com/hydai/ccstat/releases).

### Docker

You can run ccstat using Docker without installing Rust or building from source:

```bash
# Pull the latest image
docker pull hydai/ccstat:latest

# Run ccstat with your Claude data directory mounted
docker run -v "$HOME/.claude:/data:ro" hydai/ccstat daily

# Use a specific version
docker run -v "$HOME/.claude:/data:ro" hydai/ccstat:v1.0.0 monthly

# Run with custom options
docker run -v "$HOME/.claude:/data:ro" hydai/ccstat daily --json --since 2024-01-01
```

For Linux users, the path is the same:
```bash
docker run -v "$HOME/.claude:/data:ro" hydai/ccstat daily
```

For Windows users (PowerShell):
```powershell
docker run -v "$env:APPDATA\Claude:/data:ro" hydai/ccstat daily
```

The Docker image is multi-platform and supports both `linux/amd64` and `linux/arm64` architectures.

## Quick Start

```bash
# View today's usage
ccstat daily

# View this month's usage
ccstat monthly

# View all sessions with costs
ccstat session

# Export data as JSON for further processing
ccstat daily --json > usage.json
```

## Usage

### Daily Usage Report

Show daily token usage and costs:

```bash
# Default table output
ccstat daily

# JSON output for processing
ccstat daily --json

# Filter by date range
ccstat daily --since 2024-01-01 --until 2024-01-31

# Show per-instance breakdown
ccstat daily --instances

# Filter by project
ccstat daily --project my-project

# Live monitoring mode (auto-refresh)
ccstat daily --watch

# Custom refresh interval (seconds)
ccstat daily --watch --interval 30

# Performance options
ccstat daily --parallel        # Enable parallel file processing
ccstat daily --intern         # Use string interning for memory efficiency
ccstat daily --arena          # Use arena allocation

# Verbose mode (show detailed token info per entry)
ccstat daily --verbose
```

### Monthly Summary

Aggregate usage by month:

```bash
# Monthly totals
ccstat monthly

# Filter specific months
ccstat monthly --since 2024-01 --until 2024-03

# JSON output
ccstat monthly --json

# Filter by project
ccstat monthly --project my-project

# Show per-instance breakdown
ccstat monthly --instances
```

### Session Analysis

Analyze individual sessions:

```bash
# List all sessions
ccstat session

# JSON output with full details
ccstat session --json

# Filter by date range
ccstat session --since 2024-01-01 --until 2024-01-31

# Filter by project
ccstat session --project my-project

# Show detailed models per session
ccstat session --models

# Different cost calculation modes
ccstat session --mode calculate   # Always calculate from tokens
ccstat session --mode display     # Use pre-calculated costs only
```

### Billing Blocks

Track 5-hour billing blocks:

```bash
# Show all blocks
ccstat blocks

# Only active blocks
ccstat blocks --active

# Recent blocks (last 24h)
ccstat blocks --recent

# JSON output
ccstat blocks --json

# Filter by project
ccstat blocks --project my-project
```

### Cost Calculation Modes

Control how costs are calculated:

```bash
# Auto mode (default) - use pre-calculated when available
ccstat daily --mode auto

# Always calculate from tokens
ccstat daily --mode calculate

# Only use pre-calculated costs
ccstat daily --mode display
```

### Verbose Mode

Get detailed token information for each API call:

```bash
# Show individual entries for daily usage
ccstat daily --verbose

# Verbose mode with JSON output
ccstat daily --verbose --json

# Verbose mode for specific date
ccstat daily --verbose --since 2024-01-15 --until 2024-01-15
```

### Performance Options

Optimize for large datasets:

```bash
# Enable parallel processing
ccstat daily --parallel

# Use string interning to reduce memory
ccstat daily --intern

# Use arena allocation for better performance
ccstat daily --arena

# Combine all optimizations
ccstat daily --parallel --intern --arena
```

## Output Examples

### Table Format (Default)

```
┌────────────┬───────────┬──────────┬──────────────┬────────────┬───────────┬──────────┬─────────────────┐
│    Date    │   Input   │  Output  │ Cache Create │ Cache Read │   Total   │   Cost   │     Models      │
├────────────┼───────────┼──────────┼──────────────┼────────────┼───────────┼──────────┼─────────────────┤
│ 2024-01-15 │ 1,234,567 │  123,456 │      12,345  │     1,234  │ 1,371,602 │  $12.35  │ claude-3-opus   │
│ 2024-01-16 │ 2,345,678 │  234,567 │      23,456  │     2,345  │ 2,606,046 │  $23.46  │ claude-3-sonnet │
├────────────┼───────────┼──────────┼──────────────┼────────────┼───────────┼──────────┼─────────────────┤
│   TOTAL    │ 3,580,245 │  358,023 │      35,801  │     3,579  │ 3,977,648 │  $35.81  │                 │
└────────────┴───────────┴──────────┴──────────────┴────────────┴───────────┴──────────┴─────────────────┘
```

### JSON Format

```json
{
  "daily": [
    {
      "date": "2024-01-15",
      "tokens": {
        "input_tokens": 1234567,
        "output_tokens": 123456,
        "cache_creation_tokens": 12345,
        "cache_read_tokens": 1234,
        "total": 1371602
      },
      "total_cost": 12.35,
      "models_used": ["claude-3-opus"]
    }
  ],
  "totals": {
    "tokens": {
      "input_tokens": 3580245,
      "output_tokens": 358023,
      "cache_creation_tokens": 35801,
      "cache_read_tokens": 3579,
      "total": 3977648
    },
    "total_cost": 35.81
  }
}
```

### MCP Server Mode

Run ccstat as an MCP (Model Context Protocol) server for integration with other tools:

```bash
# Start MCP server on stdio
ccstat mcp

# Start MCP server on HTTP
ccstat mcp --transport http --port 8080
```

The MCP server exposes the following methods:
- `daily` - Get daily usage data
- `session` - Get session data
- `monthly` - Get monthly aggregated data

Example MCP request:
```json
{
  "method": "daily",
  "params": {
    "since": "2024-01-01",
    "until": "2024-01-31",
    "costMode": "calculate"
  }
}
```

## Configuration

### Environment Variables

- `CLAUDE_DATA_PATH`: Override default Claude data directory location
- `RUST_LOG`: Control logging level (e.g., `RUST_LOG=ccstat=debug`)

### Data Locations

ccstat automatically discovers Claude data in standard locations:

- **macOS**: `~/.claude/`
- **Linux**: `~/.claude/`
- **Windows**: `%APPDATA%\Claude\`

## Using as a Library

ccstat can also be used as a Rust library. Add to your `Cargo.toml`:

```toml
[dependencies]
ccstat = "0.1.0"
```

Example usage:

```rust
use ccstat::{
    data_loader::DataLoader,
    aggregation::Aggregator,
    cost_calculator::CostCalculator,
    pricing_fetcher::PricingFetcher,
    types::CostMode,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> ccstat::Result<()> {
    // Load and analyze usage data
    let data_loader = DataLoader::new().await?;
    let pricing_fetcher = Arc::new(PricingFetcher::new(false).await);
    let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
    let aggregator = Aggregator::new(cost_calculator);

    let entries = data_loader.load_usage_entries();
    let daily_data = aggregator.aggregate_daily(entries, CostMode::Auto).await?;

    for day in &daily_data {
        println!("{}: {} tokens", day.date, day.tokens.total());
    }

    Ok(())
}
```

See the `examples/` directory for more usage examples.

## Development

### Building from Source

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Architecture

The project follows a modular architecture:

- `types.rs` - Domain types with newtype pattern
- `data_loader.rs` - Async streaming JSONL parser
- `pricing_fetcher.rs` - LiteLLM API client with caching
- `cost_calculator.rs` - Token-based cost calculations
- `aggregation.rs` - Time-based data aggregation
- `cli.rs` - Command-line interface
- `output.rs` - Table and JSON formatters
- `mcp.rs` - MCP server implementation

### Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Run `cargo fmt` and `cargo clippy`
5. Submit a pull request

## Troubleshooting

### Common Issues

**No data found:**
- Ensure Claude Code is installed and has been used
- Check if data exists in the expected location
- Try setting `CLAUDE_DATA_PATH` environment variable

**Permission errors:**
- ccstat needs read access to Claude data directory
- On Unix systems, check directory permissions

**Pricing data unavailable:**
- ccstat will use embedded pricing data if LiteLLM API is unavailable
- Check internet connection for latest pricing

**Memory issues with large datasets:**
- Use `--parallel` flag to enable parallel processing
- Use `--intern` flag to reduce memory usage for repeated strings
- Use `--arena` flag for more efficient memory allocation

### Debug Mode

Enable debug logging to troubleshoot issues:

```bash
RUST_LOG=ccstat=debug ccstat daily
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- [ccusage](https://github.com/ryoppippi/ccusage) - The original TypeScript implementation that inspired this project
- LiteLLM for model pricing data
- Claude by Anthropic for the usage data format
