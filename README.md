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
- 👀 **Universal Live Monitoring**: Real-time tracking with auto-refresh for ALL commands
- 📊 **Live Billing Block Monitor**: Visual ASCII art display with progress bars, burn rate, and projections
- 🎨 **Watch Command Alias**: Quick `ccstat watch` shortcut for live billing block monitoring
- 💸 **Custom Cost Limits**: Set your own maximum cost threshold for progress calculations
- ⚡ **Performance Options**: Parallel processing, string interning, arena allocation
- 🔧 **Advanced Filtering**: By date, project, instance, and more
- 🌍 **Timezone Support**: Accurate daily aggregation across different timezones
- 📊 **Statusline Integration**: Real-time usage monitoring for Claude Code status bar
- 🎯 **Model Name Simplification**: Shortened model names with `--full-model-names` option

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
# View today's usage (defaults to daily command)
ccstat

# View with informational messages
ccstat --verbose

# View this month's usage
ccstat monthly

# View all sessions with costs
ccstat session

# Show statusline for Claude Code integration
ccstat statusline

# Export data as JSON for further processing (global option)
ccstat --json > usage.json

# Live monitoring (works with all commands)
ccstat --watch                    # Watch daily usage (default)
ccstat monthly --watch            # Watch monthly aggregations
ccstat session --watch            # Watch active sessions
ccstat blocks --watch --active    # Visual live billing block monitor with progress bars

# Quick alias for live billing block monitor
ccstat watch                      # Same as blocks --watch --active
ccstat watch --max-cost 150       # Set custom $150 limit for progress bars
```

## Usage

### Daily Usage Report

Show daily token usage and costs. The daily command is the default when no command is specified.

```bash
# Default table output (these are equivalent)
ccstat
ccstat daily

# Common options can be used globally or with commands
ccstat --json                               # JSON output (global)
ccstat daily --json                         # JSON output (command-specific, backward compatible)

# Filter by date range (accepts YYYY-MM-DD or YYYY-MM format)
ccstat --since 2024-01-01 --until 2024-01-31
ccstat --since 2024-01                      # From January 2024

# Daily-specific options
ccstat daily --instances                    # Show per-instance breakdown
ccstat daily --detailed                     # Show detailed token info

# Live monitoring (global option, works with all commands)
ccstat --watch                              # Watch daily usage (default)
ccstat --watch --interval 30                # Custom refresh interval

# Global options work with all commands
ccstat --project my-project                 # Filter by project
ccstat --timezone "America/New_York"        # Use specific timezone
ccstat --utc                                # Force UTC timezone
ccstat --full-model-names                   # Show full model names

# Performance options (global)
ccstat --intern                             # Use string interning
ccstat --arena                              # Use arena allocation
```

### Monthly Summary

Aggregate usage by month:

```bash
# Monthly totals
ccstat monthly

# Filter specific months (accepts YYYY-MM-DD or YYYY-MM format)
ccstat monthly --since 2024-01-01 --until 2024-03-31
ccstat monthly --since 2024-01 --until 2024-03  # Also works

# Live monitoring
ccstat monthly --watch                      # Watch monthly aggregations
ccstat monthly --watch --interval 10        # Custom refresh interval

# JSON output
ccstat monthly --json

# Filter by project
ccstat monthly --project my-project

# Timezone configuration
ccstat monthly --timezone "Asia/Tokyo"      # Use specific timezone
ccstat monthly --utc                        # Force UTC timezone

# Model display options
ccstat monthly --full-model-names           # Show full model names
```

### Session Analysis

Analyze individual sessions:

```bash
# List all sessions
ccstat session

# Live monitoring
ccstat session --watch                      # Watch active sessions
ccstat session --watch --interval 5         # Refresh every 5 seconds

# JSON output with full details
ccstat session --json

# Filter by date range
ccstat session --since 2024-01-01 --until 2024-01-31

# Filter by project
ccstat session --project my-project

# Timezone configuration
ccstat session --timezone "Europe/London"   # Use specific timezone
ccstat session --utc                        # Force UTC timezone

# Model display options
ccstat session --full-model-names           # Show full model names

# Different cost calculation modes
ccstat session --mode calculate   # Always calculate from tokens
ccstat session --mode display     # Use pre-calculated costs only
```

### Billing Blocks

Track 5-hour billing blocks:

```bash
# Show all blocks
ccstat blocks

# Live monitoring
ccstat blocks --watch                       # Watch billing blocks update
ccstat blocks --watch --active              # Visual live monitor with progress bars (recommended)
ccstat blocks --watch --interval 10         # Custom refresh interval
ccstat blocks --watch --active --max-cost 100 # Set custom $100 limit for progress calculations

# Only active blocks
ccstat blocks --active

# Recent blocks (last 24h)
ccstat blocks --recent

# JSON output
ccstat blocks --json

# Filter by project
ccstat blocks --project my-project

# Filter by date range
ccstat blocks --since 2025-08-01
ccstat blocks --since 2025-08-01 --until 2025-08-15

# Set token limit for warnings
ccstat blocks --token-limit "80%"

# Timezone configuration
ccstat blocks --timezone "America/New_York"  # Use specific timezone
ccstat blocks --utc                          # Force UTC timezone

# Model display options
ccstat blocks --full-model-names            # Show full model names
```

#### Live Billing Block Monitor

The `--watch --active` combination provides a rich visual monitoring interface for active billing blocks:

```bash
ccstat blocks --watch --active
```

This displays a real-time ASCII art dashboard with:

- **TIME Progress Bar**: Shows how much of the current 5-hour billing block has elapsed
  - Visual progress bar with percentage completion
  - Start time, elapsed time, and time remaining until block ends

- **USAGE Progress Bar**: Tracks current cost and token consumption
  - Real-time cost tracking against monthly limit
  - Burn rate indicator (NORMAL/ELEVATED) based on usage patterns
  - Total tokens consumed in the current block

- **PROJECTION Progress Bar**: Estimates final cost for the billing block
  - Status indicator (WITHIN LIMITS/APPROACHING LIMIT/OVER LIMIT)
  - Projected final cost based on current burn rate

- **Summary Line**: Shows active models, number of sessions, and projects

The monitor refreshes every 5 seconds by default (customizable with `--interval`) and provides an at-a-glance view of your Claude Code usage patterns and spending.

### Watch Command (Quick Access)

The `watch` command provides a convenient alias for the most common live monitoring use case:

```bash
# Quick access to live billing block monitor
ccstat watch                         # Equivalent to: ccstat blocks --watch --active

# Set custom cost limit
ccstat watch --max-cost 100          # Use $100 as the maximum for progress bars
ccstat watch --max-cost 250          # Use $250 for users with higher limits

# Combine with global options
ccstat watch --interval 10           # Refresh every 10 seconds
ccstat watch --project my-project    # Filter by specific project
```

The `--max-cost` option allows you to customize the cost limit used for:
- Progress bar calculations in the USAGE section
- Projection warnings in the PROJECTION section
- Status indicators (WITHIN LIMITS/APPROACHING LIMIT/OVER LIMIT)

If not specified, the tool automatically uses the highest historical cost from your billing blocks.

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

### Detailed Output Mode

Get detailed token information for each API call:

```bash
# Show individual entries for daily usage
ccstat daily --detailed

# Detailed mode with JSON output
ccstat daily --detailed --json

# Detailed mode for specific date
ccstat daily --detailed --since 2024-01-15 --until 2024-01-15
```

### Statusline Command

Real-time usage monitoring for Claude Code integration:

```bash
# Basic statusline output (requires JSON input from stdin)
echo '{"session_id": "test", "model": {"id": "claude-3-opus", "display_name": "Claude 3 Opus"}}' | ccstat statusline

# Customize monthly fee (default: $200)
ccstat statusline --monthly-fee 250

# Disable colored output
ccstat statusline --no-color

# Show date and time
ccstat statusline --show-date

# Show git branch
ccstat statusline --show-git
```

**Important**: The statusline command is designed to be called by Claude Code and expects JSON input from stdin. It will:
- Return an error immediately if run interactively in a terminal (TTY detection)
- Timeout after 5 seconds if stdin doesn't provide input
- Example usage: `echo '{"session_id": "test", "model": {"id": "claude-3-opus", "display_name": "Claude 3 Opus"}}' | ccstat statusline`

The statusline command is optimized for minimal memory footprint and fast response times, making it ideal for integration with Claude Code's status bar.

### Performance Options

Optimize for large datasets:

```bash
# Parallel processing is always enabled
ccstat daily

# Use string interning to reduce memory
ccstat daily --intern

# Use arena allocation for better performance
ccstat daily --arena

# Combine all optimizations
ccstat daily --intern --arena
```

## Output Examples

### Live Billing Block Monitor

When using `ccstat blocks --watch --active`, you get a visual dashboard:

```
+--------------------------------------------------------------------------------------------------+
|                               CCSTAT - LIVE BILLING BLOCK MONITOR                                |
+--------------------------------------------------------------------------------------------------+

| TIME         [##################......................]   46.0%                                  |
|    Started: 19:00:00  Elapsed: 2h 18m  Remaining: 2h 41m (00:00:00)                              |

| USAGE        [#############...........................]   32.9% ($   68.49/$  208.37)            |
|    Cost: $   68.49  (Burn: $0.496/min ELEVATED)  Tokens: 31,321,720                              |

| PROJECTION   [############################............]   71.2% ($  148.39/$  208.37)            |
|    Status: WITHIN LIMITS  Projected Cost: $  148.39                                              |

| Models: Opus 4.1, Sonnet 4  Sessions: 10  Projects: 2                                            |
+--------------------------------------------------------------------------------------------------+
|                            Refreshing every 5s - Press Ctrl+C to stop                            |
+--------------------------------------------------------------------------------------------------+
```

This provides:
- Real-time progress tracking for the current 5-hour billing block
- Visual indicators for usage against monthly limits
- Burn rate monitoring to detect unusual usage patterns
- Projected costs to help stay within budget

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

## Configuration

### Environment Variables

- `CLAUDE_DATA_PATH`: Override default Claude data directory location
- `RUST_LOG`: Control logging level (e.g., `RUST_LOG=ccstat=debug`)

### Logging Behavior

ccstat runs in quiet mode by default (only warnings and errors are shown):
- Use `--verbose` or `-v` flag to show informational messages
- `RUST_LOG` environment variable can override these defaults

### Data Locations

ccstat automatically discovers Claude data in standard locations:

- **macOS**: `~/.claude/`
- **Linux**: `~/.claude/`
- **Windows**: `%APPDATA%\Claude\`

## Using as a Library

ccstat can also be used as a Rust library. Add to your `Cargo.toml`:

```toml
[dependencies]
ccstat = "0.6.0"
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
- `statusline.rs` - Statusline command for Claude Code integration
- `timezone.rs` - Timezone support and configuration
- `model_formatter.rs` - Model name formatting utilities
- `filters.rs` - Data filtering logic
- `live_monitor.rs` - Live monitoring with auto-refresh
- `memory_pool.rs` - Memory pool optimization
- `string_pool.rs` - String interning for memory efficiency

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
- Parallel processing is always enabled for better performance
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
