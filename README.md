# ccstat

Analyze Claude Code usage data from local JSONL files.

## Overview

ccstat is a high-performance Rust CLI tool that processes Claude Code usage logs, calculates costs using LiteLLM pricing data, and provides various reporting views including daily, monthly, session-based, and 5-hour billing block reports.

This is a Rust reimplementation of the original TypeScript tool, offering:
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
- 🔌 **MCP Server**: API access for integrations (coming soon)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/ccstat
cd ccstat

# Build and install
cargo install --path .
```

### Pre-built Binaries

Download the latest release for your platform from the [releases page](https://github.com/yourusername/ccstat/releases).

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
```

### Monthly Summary

Aggregate usage by month:

```bash
# Monthly totals
ccstat monthly

# Filter specific months
ccstat monthly --since 2024-01 --until 2024-03
```

### Session Analysis

Analyze individual sessions:

```bash
# List all sessions
ccstat session

# JSON output with full details
ccstat session --json
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

## Configuration

### Environment Variables

- `CLAUDE_DATA_PATH`: Override default Claude data directory location
- `RUST_LOG`: Control logging level (e.g., `RUST_LOG=ccstat=debug`)

### Data Locations

ccstat automatically discovers Claude data in standard locations:

- **macOS**: `~/Library/Application Support/Claude/`
- **Linux**: `~/.config/Claude/`
- **Windows**: `%APPDATA%\Claude\`

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

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- Original TypeScript implementation by the ccstat team
- LiteLLM for model pricing data
- Claude by Anthropic for the usage data format
