# Changelog

All notable changes to ccstat will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/hydai/ccstat/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/hydai/ccstat/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/hydai/ccstat/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/hydai/ccstat/releases/tag/v0.1.1