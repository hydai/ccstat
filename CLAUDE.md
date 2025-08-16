# Claude Code Project Instructions

## Project Overview
This is the `ccstat` project - a CLI tool to analyze Claude Code usage data from local JSONL files.

## Build and Test Commands

### Building the project
```bash
cargo build --release
```

### Running tests
```bash
cargo test
```

### Running lints and format checks
```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

### Running the tool
```bash
# Basic commands (ccstat defaults to daily command)
cargo run                                    # Same as cargo run -- daily
cargo run -- daily
cargo run -- monthly
cargo run -- session
cargo run -- blocks
cargo run -- statusline

# Global options can be used without a command (defaults to daily)
cargo run -- --json                         # Daily report in JSON
cargo run -- --since 2025-01-01             # Daily report from Jan 1
cargo run -- --timezone "America/New_York"   # Daily report in NY timezone

# Global options work with all commands
cargo run -- --json monthly                 # Monthly report in JSON
cargo run -- --project my-project session   # Sessions for a project

# Date filters accept YYYY-MM-DD or YYYY-MM format
cargo run -- --since 2025-01-01 --until 2025-01-31
cargo run -- --since 2025-01                # From January 2025

# Live monitoring (global option, works with all commands)
cargo run -- --watch                        # Watch daily usage (default)
cargo run -- monthly --watch                # Watch monthly aggregations
cargo run -- session --watch --interval 10  # Watch sessions with 10s refresh
cargo run -- blocks --watch --active        # Watch active billing blocks

# Command-specific options
cargo run -- daily --instances              # Per-instance breakdown
cargo run -- daily --detailed               # Show detailed token info
cargo run -- blocks --active                # Active blocks only
cargo run -- blocks --since 2025-08-01      # Blocks from August 1, 2025
cargo run -- blocks --since 2025-08-01 --until 2025-08-15  # Blocks in date range
```

## Project Structure
- `src/` - Source code
  - `main.rs` - Entry point
  - `lib.rs` - Library root
  - `cli.rs` - CLI argument parsing
  - `types.rs` - Core types and data structures
  - `data_loader.rs` - JSONL file discovery and parsing
  - `aggregation.rs` - Usage data aggregation logic
  - `cost_calculator.rs` - Cost calculation logic
  - `pricing_fetcher.rs` - Model pricing data fetcher
  - `output.rs` - Output formatting (table/JSON)
  - `mcp.rs` - MCP server implementation
  - `error.rs` - Error types
  - `statusline.rs` - Statusline command for Claude Code integration
  - `timezone.rs` - Timezone support and configuration
  - `model_formatter.rs` - Model name formatting utilities
  - `filters.rs` - Data filtering logic
  - `live_monitor.rs` - Live monitoring with auto-refresh
  - `memory_pool.rs` - Memory pool optimization
  - `string_pool.rs` - String interning for memory efficiency

## Command-Line Options

### Global Options (work with all commands)
- `--verbose` / `-v` - Show informational output (default is quiet mode with only warnings and errors)
- `--watch` / `-w` - Enable live monitoring mode with auto-refresh (works with all commands)
- `--interval` - Refresh interval in seconds for watch mode (default: 5)
- `--mode` - Cost calculation mode (auto, calculate, fetch, offline, none)
- `--json` - Output results in JSON format instead of tables
- `--since` - Filter by start date (YYYY-MM-DD or YYYY-MM format)
- `--until` - Filter by end date (YYYY-MM-DD or YYYY-MM format)
- `--project` / `-p` - Filter by project name
- `--timezone` / `-z` - Specify timezone for date grouping (e.g., "America/New_York", "Asia/Tokyo")
- `--utc` - Use UTC for date grouping (overrides --timezone)
- `--full-model-names` - Show full model names instead of shortened versions
- `--intern` - Enable string interning for memory optimization
- `--arena` - Enable arena allocation for parsing

### Command-Specific Options

#### Daily Command
- `--instances` / `-i` - Show per-instance breakdown
- `--detailed` / `-d` - Show detailed token information per entry

#### Monthly Command
No command-specific options. Uses all global options.

#### Session Command
No command-specific options. Uses all global options.

#### Blocks Command
- `--active` - Show only active billing blocks
- `--recent` - Show only recent blocks (last 24h)
- `--token-limit` - Set token limit for warnings (e.g., "80%")

#### Statusline Command
- `--monthly-fee` - Monthly subscription fee in USD (default: 200)
- `--no-color` - Disable colored output
- `--show-date` - Show date and time in statusline
- `--show-git` - Show git branch in statusline

**Important**: The statusline command is designed to be called by Claude Code and expects JSON input from stdin. It will:
- Return an error immediately if run interactively in a terminal
- Timeout after 5 seconds if stdin doesn't provide input
- Example usage: `echo '{"session_id": "test", "model": {"id": "claude-3-opus", "display_name": "Claude 3 Opus"}}' | ccstat statusline`

## Important Notes
- Current version: 0.3.3
- The project requires Rust 1.75 or later
- Dependencies are managed in `ccusage/Cargo.toml`
- Tests are co-located with source files
- The tool looks for Claude usage data in platform-specific directories
- Timezone support enables accurate daily aggregation across different timezones
- The statusline command provides real-time usage monitoring for Claude Code integration
- Billing blocks are 5 hours long and start at hour boundaries (XX:00)

## Performance Optimization

The project includes several performance optimization features:

### Parallel Processing
- Parallel file processing is always enabled
- Significantly improves performance for large datasets
- Automatically utilizes available CPU cores

### Memory Optimization
- **String Interning** (`--intern`): Reduces memory usage by sharing identical strings
- **Arena Allocation** (`--arena`): Optimizes memory allocation for parsing
- **Memory Pools**: Built-in memory pooling for efficient resource management
- **Stream Processing**: Processes data in chunks to minimize memory footprint

### Live Monitoring
- Use `--watch` flag for real-time updates with ALL commands (daily, monthly, session, blocks)
- Configurable refresh interval with `--interval`
- Optimized for minimal CPU and memory usage during monitoring
- Automatically refreshes when data files change or at specified intervals

### Statusline Performance
- Optimized specifically for Claude Code integration
- Minimal memory footprint and fast response times
- Caches recent data for instant updates
- Includes TTY detection to prevent hanging when run interactively
- Has a 5-second timeout to prevent indefinite waiting for input

## Rust Best Practices
- Always run cargo clippy --all-targets --all-features -- -D warnings to follow the Rust best practices

## Git Commit Guidelines
- Use clear, descriptive commit messages
- Follow conventional commit style (e.g., `feat:`, `fix:`, `docs:`)
- Group related changes together
- Avoid large, monolithic commits
- Before committing, ensure all tests pass and code is formatted
- Run `lineguard --fix -r .` to lint the codes before every commit
