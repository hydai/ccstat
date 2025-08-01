# Claude Code Project Instructions

## Project Overview
This is the `ccstat` project - a CLI tool to analyze Claude Code usage data from local JSONL files.

## Build and Test Commands

### Building the project
```bash
cd ccstat
cargo build --release
```

### Running tests
```bash
cd ccstat
cargo test
```

### Running lints and format checks
```bash
cd ccstat
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

### Running the tool
```bash
cd ccstat
cargo run -- daily
cargo run -- monthly
cargo run -- session
cargo run -- blocks
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

## Important Notes
- The project requires Rust 1.75 or later
- Dependencies are managed in `ccusage/Cargo.toml`
- Tests are co-located with source files
- The tool looks for Claude usage data in platform-specific directories

## Rust Best Practices
- Always run cargo clippy --all-targets --all-features -- -D warnings to follow the Rust best practices