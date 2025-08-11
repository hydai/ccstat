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
# Basic commands
cargo run -- daily
cargo run -- monthly
cargo run -- session
cargo run -- blocks
cargo run -- statusline
cargo run -- mcp

# With timezone options
cargo run -- daily --timezone "America/New_York"
cargo run -- daily --utc

# With model display options
cargo run -- daily --full-model-names

# With live monitoring
cargo run -- daily --watch --interval 5

# With filters
cargo run -- daily --since 2025-01-01 --until 2025-01-31
cargo run -- daily --project my-project
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

### Global Options
- `--quiet` / `-q` - Suppress informational output (only show warnings and errors)

### Timezone Options
- `--timezone` / `-z` - Specify timezone for date grouping (e.g., "America/New_York", "Asia/Tokyo")
- `--utc` - Use UTC for date grouping (overrides --timezone)
- Default: Uses system's local timezone

### Model Display Options
- `--full-model-names` - Show full model names instead of shortened versions

### Performance Options
- `--watch` / `-w` - Enable live monitoring mode with auto-refresh
- `--interval` - Refresh interval in seconds for watch mode (default: 5)
- `--parallel` - Use parallel file processing for better performance
- `--intern` - Enable string interning for memory optimization
- `--arena` - Enable arena allocation for parsing

### Filtering Options
- `--since` - Filter by start date (YYYY-MM-DD) or month (YYYY-MM)
- `--until` - Filter by end date (YYYY-MM-DD) or month (YYYY-MM)
- `--project` / `-p` - Filter by project name
- `--instances` / `-i` - Show per-instance breakdown (daily command)

### Output Options
- `--json` - Output results in JSON format instead of tables
- `--verbose` / `-v` - Show detailed token information per entry

### Command-Specific Options

#### Blocks Command
- `--active` - Show only active billing blocks
- `--recent` - Show only recent blocks (last 24h)
- `--token-limit` - Set token limit for warnings (e.g., "80%")

#### Statusline Command
- `--monthly-fee` - Monthly subscription fee in USD (default: 200)
- `--no-color` - Disable colored output
- `--show-date` - Show date and time in statusline
- `--show-git` - Show git branch in statusline

#### MCP Command
- `--transport` - Transport type: stdio or http (default: stdio)
- `--port` - Port for HTTP transport (default: 8080)

## Important Notes
- Current version: 0.1.7
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
- Use `--parallel` flag to enable parallel file processing
- Significantly improves performance for large datasets
- Automatically utilizes available CPU cores

### Memory Optimization
- **String Interning** (`--intern`): Reduces memory usage by sharing identical strings
- **Arena Allocation** (`--arena`): Optimizes memory allocation for parsing
- **Memory Pools**: Built-in memory pooling for efficient resource management
- **Stream Processing**: Processes data in chunks to minimize memory footprint

### Live Monitoring
- Use `--watch` flag for real-time updates
- Configurable refresh interval with `--interval`
- Optimized for minimal CPU and memory usage during monitoring

### Statusline Performance
- Optimized specifically for Claude Code integration
- Minimal memory footprint and fast response times
- Caches recent data for instant updates

## Rust Best Practices
- Always run cargo clippy --all-targets --all-features -- -D warnings to follow the Rust best practices

## Git Commit Guidelines
- Use clear, descriptive commit messages
- Follow conventional commit style (e.g., `feat:`, `fix:`, `docs:`)
- Group related changes together
- Avoid large, monolithic commits
- Before committing, ensure all tests pass and code is formatted
