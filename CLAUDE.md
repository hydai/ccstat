# Claude Code Project Instructions

## Project Overview
This is the `ccstat` project - a CLI tool to analyze AI coding tool usage data from local log files. It supports multiple providers: Claude, Codex, OpenCode, Amp, and Pi.

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
# Basic commands (ccstat defaults to claude daily command)
cargo run                                    # Same as cargo run -- daily
cargo run -- daily
cargo run -- weekly
cargo run -- monthly
cargo run -- session
cargo run -- blocks
cargo run -- statusline

# Multi-provider support (default provider is claude)
cargo run -- codex daily                    # Codex daily usage
cargo run -- opencode monthly               # OpenCode monthly usage
cargo run -- amp session                    # Amp session analysis
cargo run -- pi daily                       # Pi Agent daily usage

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

# Watch command (hidden alias for blocks --watch --active)
cargo run -- watch                          # Live billing block monitor
cargo run -- watch --max-cost 150           # With custom cost limit

# Command-specific options
cargo run -- daily --instances              # Per-instance breakdown
cargo run -- daily --detailed               # Show detailed token info
cargo run -- weekly --start-of-week monday  # Week starting Monday
cargo run -- blocks --active                # Active blocks only
cargo run -- blocks --session-duration 3.0  # Custom block duration (hours)
cargo run -- blocks --since 2025-08-01      # Blocks from August 1, 2025
cargo run -- blocks --since 2025-08-01 --until 2025-08-15  # Blocks in date range
```

## Project Structure

This is a Cargo workspace. The root `Cargo.toml` defines workspace members and shared dependencies.

- `src/` - Main binary crate (CLI entry point)
  - `main.rs` - Entry point
  - `lib.rs` - Library root (re-exports from workspace crates)
  - `cli.rs` - CLI argument parsing (two-level: provider + report)
  - `aggregation.rs` - Usage data aggregation logic
  - `live_monitor.rs` - Live monitoring with auto-refresh
  - `statusline.rs` - Statusline command for Claude Code integration
- `crates/` - Workspace crates
  - `ccstat-core/` - Core types, error handling, filters, timezone, model formatting, string pool, memory pool
  - `ccstat-pricing/` - Cost calculation and LiteLLM pricing data fetcher
  - `ccstat-terminal/` - Output formatting (table/JSON) and blocks monitor UI
  - `ccstat-provider-claude/` - Claude Code data loader
  - `ccstat-provider-codex/` - Codex data loader
  - `ccstat-provider-opencode/` - OpenCode data loader
  - `ccstat-provider-amp/` - Amp data loader
  - `ccstat-provider-pi/` - Pi Agent data loader
  - `ccstat-mcp/` - MCP server implementation (stub)

## Command-Line Options

### Global Options (work with all commands)
- `--verbose` / `-v` - Show informational output (default is quiet mode with only warnings and errors)
- `--watch` / `-w` - Enable live monitoring mode with auto-refresh (works with all commands)
- `--interval` - Refresh interval in seconds for watch mode (default: 5)
- `--mode` - Cost calculation mode (auto, calculate, display)
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

#### Weekly Command
- `--start-of-week` - Day to start the week (default: sunday)

#### Monthly Command
No command-specific options. Uses all global options.

#### Session Command
No command-specific options. Uses all global options.

#### Blocks Command
- `--active` - Show only active billing blocks
- `--recent` - Show only recent blocks (last 24h)
- `--token-limit` - Set token limit for warnings (e.g., "80%")
- `--session-duration` - Billing block duration in hours (default: 5.0)
- `--max-cost` - Maximum cost limit in USD for progress calculations (defaults to historical maximum)

#### Watch Command (hidden alias)
Equivalent to `blocks --watch --active`.
- `--max-cost` - Maximum cost limit in USD for progress calculations

#### MCP Command
Starts an MCP server. Currently a stub that returns "not implemented".

#### Statusline Command
- `--monthly-fee` - Monthly subscription fee in USD (default: 200)
- `--no-color` - Disable colored output
- `--show-date` - Show date and time in statusline
- `--show-git` - Show git branch in statusline

**Important**: The statusline command is designed to be called by Claude Code and expects JSON input from stdin. It will:
- Return an error immediately if run interactively in a terminal
- Timeout after 5 seconds if stdin doesn't provide input
- Example usage: `echo '{"session_id": "test", "model": {"id": "claude-3-opus", "display_name": "Claude 3 Opus"}}' | ccstat statusline`

## Multi-Provider Support

ccstat supports 5 providers. The provider is an optional first subcommand (defaults to `claude`):

```bash
ccstat daily                    # Implicit Claude provider
ccstat claude daily             # Explicit Claude provider
ccstat codex daily              # Codex provider
ccstat opencode monthly         # OpenCode provider
ccstat amp session              # Amp provider
ccstat pi daily                 # Pi Agent provider
```

### Provider-Report Matrix
| Report | Claude | Codex | OpenCode | Amp | Pi |
|--------|--------|-------|----------|-----|-----|
| daily | yes | yes | yes | yes | yes |
| weekly | yes | no | yes | no | no |
| monthly | yes | yes | yes | yes | yes |
| session | yes | yes | yes | yes | yes |
| blocks | yes | no | no | no | no |
| statusline | yes | no | no | no | no |

### Provider Environment Variables
- `CLAUDE_DATA_PATH` - Override Claude data directory
- `CODEX_HOME` - Override Codex home directory (default: `~/.codex`)
- `OPENCODE_DATA_DIR` - Override OpenCode data directory (default: `~/.local/share/opencode`)
- `AMP_DATA_DIR` - Override Amp data directory (default: `~/.local/share/amp`)
- `PI_AGENT_DIR` - Override Pi Agent directory (default: `~/.pi/agent`)

## Important Notes
- Current version: 0.6.1
- The project requires **Rust 1.85 or later** (edition 2024)
- Dependencies are managed in the workspace root `Cargo.toml`
- Tests are co-located with source files
- The tool discovers usage data in platform-specific directories for each provider
- Timezone support enables accurate daily aggregation across different timezones
- The statusline command provides real-time usage monitoring for Claude Code integration
- Billing blocks default to 5 hours (configurable with `--session-duration`)

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
