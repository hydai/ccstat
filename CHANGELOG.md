# Changelog

All notable changes to ccstat will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-02-20

### Changed
- **BREAKING**: Restructured into multi-crate Cargo workspace (ccstat-core, ccstat-pricing, ccstat-terminal, ccstat-provider-claude, ccstat-provider-codex, ccstat-provider-opencode, ccstat-provider-amp, ccstat-provider-pi, ccstat-mcp)
- **BREAKING**: Two-level CLI structure with provider subcommands
- Updated Docker images and GitHub Actions to latest versions
- Updated Cargo dependencies to latest versions

### Fixed
- Resolved CI failures in workspace crate tests
- Fixed Docker Hub digests as source for GHCR manifest creation

## [0.4.0] - 2025-09-30

### Added
- Claude Sonnet 4.5 model support with pricing data

### Fixed
- Merge online and embedded pricing data for complete model lookup coverage

## [0.3.3] - 2025-08-16

### Added
- **Watch Command Alias**: New `ccstat watch` command as a convenient shortcut for `ccstat blocks --watch --active`
  - Quick access to the live billing block monitor without typing the full command
  - Supports all global options like `--interval` and `--project`
- **Custom Cost Limits**: New `--max-cost` option for setting user-defined cost thresholds
  - Available on both `watch` and `blocks` commands
  - Allows users to set their own maximum cost for progress bar calculations
  - Example: `ccstat watch --max-cost 150` sets $150 as the maximum limit
  - Defaults to historical maximum if not specified
- **Enhanced Live Billing Block Monitor**: Visual ASCII art dashboard with real-time metrics
  - Progress bars for time elapsed, current usage, and cost projections
  - Burn rate indicator (NORMAL/ELEVATED) based on usage patterns
  - Status indicators (WITHIN LIMITS/APPROACHING LIMIT/OVER LIMIT)

### Changed
- Refactored main.rs to eliminate code duplication with new `handle_blocks_command` function
- LiveMonitor now supports user-defined maximum cost limits via `with_max_cost` method
- CLI structs now derive `Clone` trait for better command reusability

### Documentation
- Added comprehensive documentation for the new `watch` command
- Updated README with examples of custom cost limit usage
- Added visual output examples of the live billing block monitor

## [0.3.2] - 2025-08-15

### Fixed
- Properly count sessions in billing blocks
- Improve format_duration robustness and remove redundant output
- Handle negative session duration to prevent panic
- Correct gap block creation logic

## [0.3.1] - 2025-08-14

### Fixed
- **Blocks command date filtering**: Fixed issue where `ccstat blocks --since` and `--until` filters were not being applied
  - The blocks command now correctly filters billing blocks by date range
  - Both normal and watch modes now respect date filters
  - Example: `ccstat blocks --since 2025-08-14` now shows only blocks starting from that date

## [0.3.0] - 2025-08-14

### Added
- **Live Monitoring for All Commands**: Extended `--watch` functionality to all commands
  - `ccstat monthly --watch` - Monitor monthly aggregations in real-time
  - `ccstat session --watch` - Watch active sessions live
  - `ccstat blocks --watch` - Track billing blocks as they update
  - Works with all filters and options (e.g., `--watch --project my-project`)
- CommandType enum for flexible command-specific monitoring modes
- Enhanced LiveMonitor to support all aggregation types (daily, monthly, session, blocks)

### Fixed
- **Billing blocks --active flag regression**: Fixed issue where `ccstat blocks --active` would not show any active blocks
  - Corrected `is_active` logic in `create_billing_blocks` to properly check if a block is within its 5-hour window
  - Added comprehensive test coverage for active block detection

### Changed
- **BREAKING**: Unified CLI options system - common options are now global
- **BREAKING**: Moved `--watch` and `--interval` flags from daily command to global level
- **BREAKING**: LiveMonitor::new() API signature changed to support all command types
- All common options (json, since, until, project, etc.) can be used at the global level
- Default to daily command when no subcommand is specified (`ccstat` = `ccstat daily`)
- Unified date parsing to accept both YYYY-MM-DD and YYYY-MM formats

### Removed
- **BREAKING**: Removed deprecated `--quiet` flag (quiet mode is now always the default)
- **BREAKING**: Removed deprecated `--parallel` flag (parallel processing is always enabled)
- **BREAKING**: Removed deprecated `load_usage_entries()` method from DataLoader
- Removed TimezoneArgs, ModelDisplayArgs, and PerformanceArgs structs (options moved to global)

### Improved
- Significantly reduced code duplication (~150+ lines removed)
- More intuitive CLI usage (e.g., `ccstat --json` instead of `ccstat daily --json`)
- Better user experience with unified options system
- Live monitoring now available for all report types, not just daily

## [0.2.2] - 2025-08-13

### Changed
- Parallel processing is now always enabled for improved performance
- The `--parallel` flag has been deprecated and will be removed in v0.3.0

### Fixed
- Removed duplicate deprecation warnings
- Fixed rustdoc warnings for deprecated text

## [0.2.1] - 2025-08-13

### Fixed
- Changed release workflow to automatically publish releases instead of creating drafts
  - Streamlines the release process by eliminating manual publishing step

## [0.2.0] - 2025-08-13

### Changed
- Major version bump to 0.2.0
- Performance improvements and optimizations
- Enhanced code structure for better maintainability

## [0.1.9] - 2025-08-12

### Changed
- Version bump with minor updates

## [0.1.8] - 2025-08-11

### Fixed
- Prevent statusline command from hanging when run interactively
  - Added TTY detection to return error immediately when run in terminal
  - Implemented 5-second timeout to prevent indefinite waiting for input

### Removed
- MCP (Model Context Protocol) server functionality - no longer on roadmap

## [0.1.7] - 2025-08-11

### Added
- Simplified model name display with `--full-model-names` option
  - Shows shortened model names by default for better readability
  - Full model names available via flag when needed

### Fixed
- Display correct timezone in blocks and sessions command output
  - Session start/end times now respect the configured timezone
  - Billing block times properly show in the user's timezone

## [0.1.6] - 2025-08-11

### Fixed
- Fixed billing block detection in statusline command
  - Corrected block duration from 8 hours back to 5 hours (Claude's actual billing period)
  - Fixed block start time alignment to hour boundaries (XX:00) instead of 8-hour epochs
  - Now matches the behavior in aggregation.rs::create_billing_blocks

## [0.1.5] - 2025-08-10

### Added
- New `statusline` command for Claude Code integration
  - Real-time usage monitoring for Claude Code status bar
  - Optimized for minimal memory footprint and fast response times
  - Returns current session usage, billing block info, and cost data

### Changed
- Performance optimizations for data processing pipeline
- Improved memory efficiency for large usage datasets

### Fixed
- Timestamp handling improvements for better accuracy
- Memory usage optimizations in statusline processing

## [0.1.4] - 2025-08-10

### Added
- Timezone support for accurate daily aggregation
  - New `--timezone` flag to specify custom timezone (e.g., "America/New_York", "Asia/Tokyo")
  - New `--utc` flag to force UTC timezone
  - Automatic local timezone detection (uses system timezone by default)
  - Timezone-aware date filtering and aggregation

### Fixed
- Daily usage now correctly shows today's data in timezones ahead of UTC
  - Previously, timestamps were always converted to UTC dates, causing "today's" usage to be grouped under "yesterday" in timezones ahead of UTC
  - Now respects the configured timezone when determining which day a usage entry belongs to

### Changed
- Date aggregation now uses local timezone by default instead of UTC
- All commands (daily, monthly, session, blocks) now support timezone configuration

## [0.1.3] - 2025-08-09

### Fixed
- Billing block start times now correctly align to hour boundaries (XX:00) according to Claude Code Spec
  - Blocks now start at the beginning of the hour rather than at the exact session start time
  - Ensures accurate billing window tracking and time remaining calculations

## [0.1.2] - 2025-08-09

### Added
- `--quiet` flag to suppress INFO level logs for cleaner output
- Security-events write permission for Trivy scanner in CI workflow
- Packages write permission for GitHub Container Registry

### Changed
- Use native runners for Docker builds instead of QEMU for better performance
- Added caching for cargo-tarpaulin in CI workflow to speed up test coverage

## [0.1.1] - 2025-08-04

### Added
- Initial release of ccstat
- Daily, monthly, session, and billing block report views
- Automatic Claude data directory discovery across platforms
- Cost calculation using LiteLLM pricing data with offline fallback
- Table and JSON output formats
- MCP server for JSON-RPC API integrations
- Live monitoring with auto-refresh capability
- Advanced filtering options by date, project, and instance
- High-performance stream processing with minimal memory footprint

[Unreleased]: https://github.com/hydai/ccstat/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/hydai/ccstat/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/hydai/ccstat/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/hydai/ccstat/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/hydai/ccstat/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/hydai/ccstat/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/hydai/ccstat/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/hydai/ccstat/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/hydai/ccstat/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/hydai/ccstat/compare/v0.1.9...v0.2.0
[0.1.9]: https://github.com/hydai/ccstat/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/hydai/ccstat/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/hydai/ccstat/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/hydai/ccstat/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/hydai/ccstat/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/hydai/ccstat/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/hydai/ccstat/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/hydai/ccstat/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/hydai/ccstat/releases/tag/v0.1.1
