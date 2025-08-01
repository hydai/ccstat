# Requirements Specification: ccusage Rust Implementation

## 1. Overview

### 1.1 Purpose
This document defines the functional and non-functional requirements for reimplementing the ccusage CLI tool in Rust. The primary objective is to create a memory-safe, high-performance version that eliminates memory leaks while maintaining complete feature parity with the existing TypeScript implementation.

### 1.2 Scope
The ccusage tool analyzes Claude Code usage data from local JSONL files, calculates costs using LiteLLM pricing data, and provides various reporting views including daily, monthly, session-based, and 5-hour billing block reports.

## 2. Functional Requirements

### 2.1 Data Processing

#### 2.1.1 Data Discovery
- **FR-001**: The system SHALL automatically discover Claude data directories across supported platforms
- **FR-002**: The system SHALL search for JSONL files within discovered Claude directories
- **FR-003**: The system SHALL support reading from multiple Claude instances/directories

#### 2.1.2 JSONL Parsing
- **FR-004**: The system SHALL parse JSONL usage log files line by line
- **FR-005**: The system SHALL extract usage entries containing:
  - Session ID
  - Timestamp (ISO format)
  - Model name
  - Token counts (input, output, cache creation, cache read)
  - Pre-calculated costs (when available)
- **FR-006**: The system SHALL handle malformed JSON entries gracefully without terminating

#### 2.1.3 Cost Calculation
- **FR-007**: The system SHALL fetch model pricing data from LiteLLM API
- **FR-008**: The system SHALL support offline mode using embedded pricing data
- **FR-009**: The system SHALL calculate costs based on token counts and model pricing
- **FR-010**: The system SHALL support three cost modes:
  - Auto: Use pre-calculated costs when available
  - Calculate: Always calculate from tokens
  - Display: Always use pre-calculated costs

### 2.2 Reporting Features

#### 2.2.1 Daily Usage Reports
- **FR-011**: The system SHALL aggregate usage data by calendar day
- **FR-012**: The system SHALL display daily token counts by type
- **FR-013**: The system SHALL show total cost per day
- **FR-014**: The system SHALL list models used each day
- **FR-015**: The system SHALL support date range filtering (--since, --until)
- **FR-016**: The system SHALL support project filtering (--project)
- **FR-017**: The system SHALL support instance-specific reporting (--instances)

#### 2.2.2 Monthly Usage Reports
- **FR-018**: The system SHALL aggregate usage data by calendar month
- **FR-019**: The system SHALL display monthly token counts and costs
- **FR-020**: The system SHALL support date range filtering for months

#### 2.2.3 Session Usage Reports
- **FR-021**: The system SHALL aggregate usage data by session ID
- **FR-022**: The system SHALL display session duration, token counts, and costs
- **FR-023**: The system SHALL sort sessions by timestamp
- **FR-024**: The system SHALL support date range filtering for sessions

#### 2.2.4 Billing Block Reports
- **FR-025**: The system SHALL group sessions into 5-hour billing blocks
- **FR-026**: The system SHALL identify active billing blocks
- **FR-027**: The system SHALL support filtering for recent blocks
- **FR-028**: The system SHALL support token limit warnings

### 2.3 Output Formats

#### 2.3.1 Table Format
- **FR-029**: The system SHALL display data in formatted ASCII tables
- **FR-030**: The system SHALL include column headers and totals rows
- **FR-031**: The system SHALL format numbers with appropriate separators
- **FR-032**: The system SHALL format currency values with $ prefix

#### 2.3.2 JSON Format
- **FR-033**: The system SHALL support JSON output via --json flag
- **FR-034**: The system SHALL include both data and totals in JSON output
- **FR-035**: The system SHALL format JSON with proper indentation

### 2.4 MCP Server

#### 2.4.1 Server Functionality
- **FR-036**: The system SHALL provide an MCP server mode
- **FR-037**: The system SHALL support stdio and HTTP transports
- **FR-038**: The system SHALL expose usage data via JSON-RPC methods
- **FR-039**: The system SHALL support configurable port for HTTP transport

### 2.5 Live Monitoring
- **FR-040**: The system SHALL support live monitoring of active sessions
- **FR-041**: The system SHALL auto-refresh usage data at intervals
- **FR-042**: The system SHALL highlight currently active sessions

## 3. Non-Functional Requirements

### 3.1 Performance

#### 3.1.1 Memory Usage
- **NFR-001**: The system SHALL use zero-copy parsing where possible
- **NFR-002**: The system SHALL stream files instead of loading entire contents
- **NFR-003**: The system SHALL achieve 50-70% reduction in memory usage vs TypeScript
- **NFR-004**: The system SHALL maintain constant memory usage regardless of data size

#### 3.1.2 Processing Speed
- **NFR-005**: The system SHALL achieve 2-3x faster processing than TypeScript version
- **NFR-006**: The system SHALL use parallel processing for aggregations
- **NFR-007**: The system SHALL complete typical daily reports in <100ms

### 3.2 Reliability

#### 3.2.1 Error Handling
- **NFR-008**: The system SHALL handle all errors gracefully without panics
- **NFR-009**: The system SHALL provide meaningful error messages
- **NFR-010**: The system SHALL continue processing after encountering bad data
- **NFR-011**: The system SHALL fall back to embedded pricing on network failures

#### 3.2.2 Data Integrity
- **NFR-012**: The system SHALL accurately parse all valid JSONL entries
- **NFR-013**: The system SHALL maintain precision for cost calculations
- **NFR-014**: The system SHALL handle timezone conversions correctly

### 3.3 Compatibility

#### 3.3.1 Platform Support
- **NFR-015**: The system SHALL support Linux x86_64 (primary)
- **NFR-016**: The system SHALL support macOS arm64/x86_64 (primary)
- **NFR-017**: The system SHALL support Windows x86_64 (secondary)
- **NFR-018**: The system SHALL require Rust 1.75.0 or later

#### 3.3.2 Data Compatibility
- **NFR-019**: The system SHALL read existing JSONL format without changes
- **NFR-020**: The system SHALL produce output compatible with existing tools
- **NFR-021**: The system SHALL maintain API compatibility for MCP server

### 3.4 Security
- **NFR-022**: The system SHALL not expose sensitive user data
- **NFR-023**: The system SHALL validate all input data
- **NFR-024**: The system SHALL use secure HTTPS for API requests
- **NFR-025**: The system SHALL not store credentials or tokens

### 3.5 Maintainability

#### 3.5.1 Code Quality
- **NFR-026**: The system SHALL follow Rust best practices and idioms
- **NFR-027**: The system SHALL have comprehensive error types
- **NFR-028**: The system SHALL use strong typing for domain concepts
- **NFR-029**: The system SHALL minimize unsafe code usage

#### 3.5.2 Testing
- **NFR-030**: The system SHALL have >80% unit test coverage
- **NFR-031**: The system SHALL include integration tests
- **NFR-032**: The system SHALL include performance benchmarks
- **NFR-033**: The system SHALL use property-based testing for parsers

### 3.6 Documentation
- **NFR-034**: The system SHALL include comprehensive API documentation
- **NFR-035**: The system SHALL provide usage examples
- **NFR-036**: The system SHALL document all command-line options
- **NFR-037**: The system SHALL include architecture documentation

## 4. Technical Requirements

### 4.1 Architecture

#### 4.1.1 Module Structure
- **TR-001**: The system SHALL use a modular architecture with clear separation
- **TR-002**: The system SHALL implement the following core modules:
  - Data types and domain models
  - Data loader for file discovery and parsing
  - Pricing fetcher with caching
  - Cost calculator
  - Aggregation engine
  - CLI interface
  - Output formatters
  - MCP server

#### 4.1.2 Data Types
- **TR-003**: The system SHALL use newtype pattern for branded types
- **TR-004**: The system SHALL use strong typing for:
  - Model names
  - Session IDs
  - Timestamps
  - Daily dates
  - Token counts

### 4.2 Dependencies

#### 4.2.1 Core Dependencies
- **TR-005**: The system SHALL use tokio for async runtime
- **TR-006**: The system SHALL use serde for JSON serialization
- **TR-007**: The system SHALL use chrono for date/time handling
- **TR-008**: The system SHALL use clap for CLI parsing

#### 4.2.2 Additional Dependencies
- **TR-009**: The system SHALL use reqwest for HTTP requests
- **TR-010**: The system SHALL use prettytable-rs for table formatting
- **TR-011**: The system SHALL use thiserror for error handling
- **TR-012**: The system SHALL use tracing for logging

### 4.3 Build Configuration
- **TR-013**: The system SHALL use LTO for release builds
- **TR-014**: The system SHALL strip symbols in release builds
- **TR-015**: The system SHALL use optimization level 3
- **TR-016**: The system SHALL produce single static binaries

## 5. Data Specifications

### 5.1 Input Format

#### 5.1.1 JSONL Entry Structure
```json
{
  "session_id": "string",
  "timestamp": "ISO 8601 datetime",
  "model": "string",
  "input_tokens": number,
  "output_tokens": number,
  "cache_creation_tokens": number,
  "cache_read_tokens": number,
  "total_cost": number (optional)
}
```

### 5.2 Output Formats

#### 5.2.1 Daily Usage JSON
```json
{
  "daily": [
    {
      "date": "YYYY-MM-DD",
      "tokens": {
        "input_tokens": number,
        "output_tokens": number,
        "cache_creation_tokens": number,
        "cache_read_tokens": number
      },
      "total_cost": number,
      "models_used": ["model1", "model2"]
    }
  ],
  "totals": {
    "tokens": { ... },
    "total_cost": number
  }
}
```

## 6. Constraints and Assumptions

### 6.1 Constraints
- **C-001**: Must maintain complete feature parity with TypeScript version
- **C-002**: Must not require changes to existing JSONL format
- **C-003**: Must work with existing Claude directory structures
- **C-004**: Must be distributable as single binary

### 6.2 Assumptions
- **A-001**: JSONL files follow consistent format
- **A-002**: Claude directories follow standard naming patterns
- **A-003**: System has sufficient permissions to read log files
- **A-004**: LiteLLM API remains stable

## 7. Success Criteria

### 7.1 Acceptance Criteria
- **AC-001**: All functional requirements implemented and tested
- **AC-002**: Memory usage reduced by at least 50%
- **AC-003**: Processing speed improved by at least 2x
- **AC-004**: Zero memory leaks verified through testing
- **AC-005**: All existing use cases supported

### 7.2 Quality Metrics
- **QM-001**: Test coverage > 80%
- **QM-002**: No clippy warnings at pedantic level
- **QM-003**: Documentation coverage 100%
- **QM-004**: Benchmark suite shows consistent performance

## 8. Migration Requirements

### 8.1 Phased Approach
- **MR-001**: Phase 1 - Core data structures and parsing
- **MR-002**: Phase 2 - CLI commands (daily, monthly, session)
- **MR-003**: Phase 3 - Advanced features (blocks, MCP server)
- **MR-004**: Phase 4 - Performance optimization
- **MR-005**: Phase 5 - Feature parity testing

### 8.2 Compatibility Testing
- **MR-006**: Verify output matches TypeScript version
- **MR-007**: Ensure MCP API compatibility
- **MR-008**: Test with real-world usage data
- **MR-009**: Validate performance improvements

## 9. Future Considerations

### 9.1 Potential Enhancements
- Support for additional output formats (CSV, HTML)
- Real-time streaming of usage data
- Historical data archiving and compression
- Advanced filtering and query capabilities
- Integration with cost management platforms

### 9.2 Extensibility
- Plugin system for custom aggregations
- Configurable pricing sources
- Custom output formatters
- Webhook notifications for limits

---

This requirements document serves as the authoritative specification for the ccusage Rust implementation. All development should reference these requirements to ensure complete and accurate implementation.