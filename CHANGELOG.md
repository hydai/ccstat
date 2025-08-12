# Changelog

All notable changes to ccstat will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/hydai/ccstat/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/hydai/ccstat/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/hydai/ccstat/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/hydai/ccstat/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/hydai/ccstat/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/hydai/ccstat/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/hydai/ccstat/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/hydai/ccstat/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/hydai/ccstat/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/hydai/ccstat/releases/tag/v0.1.1
