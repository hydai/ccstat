# Changelog

All notable changes to ccstat will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial Rust implementation of ccstat (formerly ccusage)
- Daily usage aggregation with token breakdown
- Monthly usage rollup reports
- Session-based usage tracking
- 5-hour billing block calculations
- Multiple cost calculation modes (auto, calculate, display)
- JSON and table output formats
- MCP server support for API access
- Cross-platform support (macOS, Linux, Windows)
- Performance optimizations (parallel processing, string interning, arena allocation)
- Verbose mode for detailed token information
- Project-based filtering
- Date range filtering
- Comprehensive test suite (unit, integration, property-based)
- Performance benchmarks
- Complete documentation (API, user guide, architecture)

### Changed
- Renamed project from ccusage to ccstat
- Improved JSONL parsing to align with Claude Code data format
- Enhanced error handling with detailed error types
- Optimized memory usage for large datasets

### Fixed
- Token calculation discrepancies with TypeScript implementation
- Billing block duration calculations
- Cost calculation precision issues

## [0.1.0] - TBD

Initial release of ccstat.

### Features
- **Data Aggregation**
  - Daily usage reports
  - Monthly summaries
  - Session analysis
  - Billing block tracking

- **Cost Calculation**
  - LiteLLM pricing integration
  - Multiple calculation modes
  - Pre-calculated cost support

- **Output Formats**
  - Human-readable tables
  - Machine-readable JSON
  - MCP server API

- **Performance**
  - Streaming JSONL parser
  - Parallel file processing
  - Memory optimizations

- **Platform Support**
  - macOS (x86_64, ARM64)
  - Linux (x86_64, ARM64)
  - Windows (x86_64)

### Known Issues
- Large datasets (>1GB) may require performance flags
- MCP HTTP transport is experimental

[Unreleased]: https://github.com/yourusername/ccstat/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourusername/ccstat/releases/tag/v0.1.0