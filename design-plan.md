# Design Plan: ccusage Rust Implementation

## Executive Summary

This document outlines the design plan for reimplementing ccusage in Rust, focusing on memory safety, performance optimization, and maintainability. The design leverages Rust's ownership system, async capabilities, and zero-copy parsing to achieve a 50-70% reduction in memory usage and 2-3x performance improvement over the TypeScript implementation.

## System Architecture

### High-Level Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   CLI Module    │────▶│  Core Engine     │────▶│ Output Module   │
│  (User Input)   │     │  (Processing)    │     │ (Formatting)    │
└─────────────────┘     └──────────────────┘     └─────────────────┘
         │                       │                         │
         ▼                       ▼                         ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Command Parser │     │  Data Loader     │     │ Table Formatter │
│  (clap)         │     │  (Async Streams) │     │ (prettytable)   │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌──────────────────┐
                        │ Pricing Fetcher  │
                        │ (Cache + API)    │
                        └──────────────────┘
```

### Module Interactions

```
CLI ──parse──▶ Commands ──execute──▶ DataLoader ──stream──▶ Aggregator
                                           │                      │
                                           ▼                      ▼
                                    PricingFetcher         CostCalculator
                                           │                      │
                                           └──────────────────────┘
                                                      │
                                                      ▼
                                               OutputFormatter
```

## Design Principles

### 1. Zero-Copy Architecture
- Use borrowed data (`&str`) instead of owned strings where possible
- Stream processing to avoid loading entire files into memory
- Leverage `serde_json::from_str` with borrowed fields

### 2. Type Safety
- Newtype pattern for domain concepts (ModelName, SessionId)
- Strong typing prevents mixing incompatible values
- Compile-time guarantees for correctness

### 3. Async-First Design
- Tokio runtime for all I/O operations
- Concurrent file processing with futures
- Non-blocking HTTP requests for pricing data

### 4. Error Resilience
- Graceful degradation for malformed data
- Fallback mechanisms for network failures
- Comprehensive error types with context

## Module Design Plans

### 1. Data Types Module (`src/types.rs`)

**Purpose**: Define strongly-typed domain models

**Key Design Decisions**:
- Newtype wrappers for type safety
- Derive common traits for serialization
- Copy types where appropriate for performance

### 2. Data Loader Module (`src/data_loader.rs`)

**Purpose**: Discover and stream JSONL files

**Key Design Decisions**:
- Async file operations with Tokio
- Stream-based processing to minimize memory
- Platform-specific path discovery
- Graceful handling of missing directories

### 3. Pricing Fetcher Module (`src/pricing_fetcher.rs`)

**Purpose**: Fetch and cache model pricing data

**Key Design Decisions**:
- RwLock for concurrent read access
- Embedded fallback pricing data
- Lazy loading with async initialization
- Model name fuzzy matching

### 4. Cost Calculator Module (`src/cost_calculator.rs`)

**Purpose**: Calculate costs from tokens and pricing

**Key Design Decisions**:
- Pure functions for calculation logic
- Support for all cost modes (Auto/Calculate/Display)
- Precision handling for financial calculations

### 5. Aggregation Module (`src/aggregation.rs`)

**Purpose**: Aggregate usage data by time period

**Key Design Decisions**:
- BTreeMap for sorted aggregations
- Accumulator pattern for incremental updates
- Generic aggregation traits for reuse

### 6. CLI Module (`src/cli.rs`)

**Purpose**: Parse and validate command-line arguments

**Key Design Decisions**:
- Clap derive API for maintainability
- Subcommand pattern for extensibility
- Environment variable support

### 7. Output Module (`src/output.rs`)

**Purpose**: Format data for display

**Key Design Decisions**:
- Trait-based formatter design
- Pluggable output formats
- Human-readable number formatting

### 8. MCP Server Module (`src/mcp.rs`)

**Purpose**: Provide API access to usage data

**Key Design Decisions**:
- JSON-RPC protocol compliance
- Shared state with Arc
- Transport abstraction

## User Stories

### Epic 1: Data Processing

#### Story 1.1: Automatic Data Discovery
**As a** Claude Code user  
**I want to** have my usage data automatically discovered  
**So that** I don't need to specify file paths manually

**Acceptance Criteria**:
- System finds Claude directories on macOS, Linux, and Windows
- Supports multiple Claude instances
- Works with standard and custom installations
- Provides clear error when no data found

**Technical Tasks**:
- Implement platform-specific path discovery
- Create directory traversal logic
- Add instance detection
- Handle permission errors gracefully

---

#### Story 1.2: Robust JSONL Parsing
**As a** developer  
**I want to** parse usage logs without crashes  
**So that** corrupted entries don't break my reports

**Acceptance Criteria**:
- Continues processing after malformed JSON
- Logs warnings for bad entries
- Maintains count of skipped entries
- Preserves all valid data

**Technical Tasks**:
- Implement line-by-line streaming parser
- Add error recovery logic
- Create validation for required fields
- Add metrics for parse success rate

---

### Epic 2: Cost Calculation

#### Story 2.1: Accurate Cost Calculation
**As a** budget-conscious user  
**I want to** see accurate costs for my usage  
**So that** I can track my spending

**Acceptance Criteria**:
- Calculates costs using latest LiteLLM pricing
- Supports all token types (input, output, cache)
- Handles precision correctly for currency
- Shows costs in USD with $ prefix

**Technical Tasks**:
- Integrate LiteLLM API client
- Implement cost calculation logic
- Add currency formatting
- Create cost mode handling

---

#### Story 2.2: Offline Mode Support
**As a** user without internet  
**I want to** calculate costs offline  
**So that** I can use the tool anywhere

**Acceptance Criteria**:
- Falls back to embedded pricing data
- Shows warning when using offline data
- Pricing data updated with each release
- No functionality loss in offline mode

**Technical Tasks**:
- Embed pricing JSON in binary
- Implement fallback mechanism
- Add offline mode detection
- Create update process for embedded data

---

### Epic 3: Reporting Features

#### Story 3.1: Daily Usage Reports
**As a** daily user  
**I want to** see my usage by day  
**So that** I can track daily patterns

**Acceptance Criteria**:
- Groups usage by calendar day
- Shows token breakdown by type
- Calculates daily costs
- Lists models used each day
- Supports date filtering

**Technical Tasks**:
- Implement daily aggregation logic
- Add date parsing and filtering
- Create daily report formatter
- Add model tracking

---

#### Story 3.2: Monthly Summary Reports
**As a** team lead  
**I want to** see monthly usage summaries  
**So that** I can report on team spending

**Acceptance Criteria**:
- Aggregates by calendar month
- Shows month-over-month trends
- Includes total costs and tokens
- Exports to JSON for processing

**Technical Tasks**:
- Implement monthly aggregation
- Add trend calculation
- Create summary statistics
- Implement JSON export

---

#### Story 3.3: Session-Based Analysis
**As a** power user  
**I want to** analyze individual sessions  
**So that** I can optimize my usage patterns

**Acceptance Criteria**:
- Lists all sessions with metadata
- Shows session duration
- Calculates per-session costs
- Sorts by various criteria

**Technical Tasks**:
- Implement session aggregation
- Calculate session duration
- Add sorting capabilities
- Create session report format

---

#### Story 3.4: Billing Block Tracking
**As a** cost-aware user  
**I want to** track 5-hour billing blocks  
**So that** I can optimize my usage timing

**Acceptance Criteria**:
- Groups sessions into 5-hour blocks
- Identifies active blocks
- Shows remaining time/tokens
- Warns about approaching limits

**Technical Tasks**:
- Implement block calculation logic
- Add active block detection
- Create limit warning system
- Build block report format

---

### Epic 4: Output and Integration

#### Story 4.1: Beautiful Table Output
**As a** terminal user  
**I want to** see well-formatted tables  
**So that** data is easy to read

**Acceptance Criteria**:
- Displays aligned ASCII tables
- Formats numbers with commas
- Shows currency with $ symbol
- Includes totals row
- Fits standard terminal width

**Technical Tasks**:
- Integrate prettytable-rs
- Implement number formatting
- Add column width optimization
- Create responsive layout

---

#### Story 4.2: Machine-Readable JSON
**As a** automation developer  
**I want to** get JSON output  
**So that** I can process data programmatically

**Acceptance Criteria**:
- Outputs valid JSON with --json flag
- Includes all data fields
- Maintains precision for numbers
- Pretty-prints for readability

**Technical Tasks**:
- Implement JSON serialization
- Add pretty-printing
- Ensure schema consistency
- Document JSON structure

---

#### Story 4.3: MCP Server Mode
**As a** tool developer  
**I want to** access usage data via API  
**So that** I can build integrations

**Acceptance Criteria**:
- Provides JSON-RPC interface
- Supports all report types
- Handles concurrent requests
- Works over stdio and HTTP

**Technical Tasks**:
- Implement JSON-RPC handler
- Create transport abstraction
- Add request routing
- Build error responses

---

### Epic 5: Performance and Reliability

#### Story 5.1: Lightning-Fast Reports
**As a** impatient user  
**I want to** get reports instantly  
**So that** I don't waste time waiting

**Acceptance Criteria**:
- Daily reports complete in <100ms
- Handles 1M+ entries efficiently
- Uses minimal memory
- Provides progress indication

**Technical Tasks**:
- Optimize parsing performance
- Implement parallel processing
- Add progress reporting
- Profile and benchmark

---

#### Story 5.2: Zero Memory Leaks
**As a** long-running service user  
**I want to** run without memory leaks  
**So that** the tool remains stable

**Acceptance Criteria**:
- No memory growth over time
- Proper cleanup of resources
- Verified by memory profiling
- Constant memory usage

**Technical Tasks**:
- Implement RAII patterns
- Add drop implementations
- Create memory benchmarks
- Run leak detection tools

---

## Technical Design Patterns

### 1. Stream Processing Pattern
```rust
impl DataLoader {
    pub fn load_entries(&self) -> impl Stream<Item = Result<Entry>> {
        self.find_files()
            .map(|path| self.parse_file_stream(path))
            .flatten_unordered(10) // Process 10 files concurrently
    }
}
```

### 2. Accumulator Pattern
```rust
struct DailyAccumulator {
    tokens: TokenCounts,
    cost: f64,
    models: HashSet<ModelName>,
}

impl DailyAccumulator {
    fn add_entry(&mut self, entry: UsageEntry) {
        self.tokens += entry.tokens;
        self.cost += entry.cost;
        self.models.insert(entry.model);
    }
}
```

### 3. Type State Pattern
```rust
struct PricingFetcher<S> {
    state: S,
}

struct Uninitialized;
struct Initialized {
    cache: HashMap<String, ModelPricing>,
}

impl PricingFetcher<Uninitialized> {
    async fn initialize(self) -> PricingFetcher<Initialized> {
        // Fetch and cache pricing
    }
}
```

### 4. Builder Pattern for Complex Types
```rust
impl UsageReportBuilder {
    fn new() -> Self { ... }
    fn with_date_range(self, start: Date, end: Date) -> Self { ... }
    fn with_cost_mode(self, mode: CostMode) -> Self { ... }
    fn build(self) -> Result<UsageReport> { ... }
}
```

## Data Flow Design

### 1. Input Flow
```
JSONL Files → Stream Reader → Line Parser → Entry Validator → Usage Entry
```

### 2. Processing Flow
```
Usage Entry → Aggregator → Accumulator → Cost Calculator → Aggregated Data
```

### 3. Output Flow
```
Aggregated Data → Formatter Selection → Format Conversion → Display/Export
```

## Error Handling Strategy

### 1. Error Types Hierarchy
```rust
#[derive(Error, Debug)]
pub enum CcusageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error in {file}: {error}")]
    Parse { file: PathBuf, error: String },
    
    #[error("No Claude directories found")]
    NoDataDirectory,
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}
```

### 2. Error Recovery
- Continue processing on parse errors
- Fall back to embedded pricing on network errors
- Log warnings for non-fatal issues
- Provide actionable error messages

### 3. Result Propagation
- Use `?` operator for clean error propagation
- Wrap external errors with context
- Collect errors for batch operations
- Report partial success when appropriate

## Performance Design

### 1. Memory Management
- **Zero-Copy Parsing**: Use `&str` references into original data
- **Streaming**: Process files line-by-line, not all at once
- **Arena Allocation**: Pool allocations for parsing phase
- **Const Generics**: Compile-time optimizations

### 2. Concurrency Strategy
- **Parallel File Processing**: Process multiple files concurrently
- **Async I/O**: Non-blocking file and network operations
- **Read-Write Locks**: Optimize for concurrent reads
- **Work Stealing**: Use Tokio's work-stealing scheduler

### 3. Optimization Techniques
- **String Interning**: Reuse common strings (model names)
- **SIMD Operations**: Use for number formatting
- **Branch Prediction**: Order conditions by likelihood
- **LTO**: Link-time optimization for release builds

## Testing Strategy

### 1. Unit Testing
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_valid_entry() {
        let json = r#"{"session_id": "123", ...}"#;
        let entry = parse_entry(json).unwrap();
        assert_eq!(entry.session_id, SessionId::new("123"));
    }
}
```

### 2. Integration Testing
```rust
#[tokio::test]
async fn test_daily_aggregation() {
    let loader = DataLoader::new().unwrap();
    let entries = loader.load_entries();
    let daily = aggregate_daily(entries).await.unwrap();
    assert!(!daily.is_empty());
}
```

### 3. Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_cost_calculation(
        input in 0u64..1_000_000,
        rate in 0.0..1.0
    ) {
        let cost = calculate_cost(input, rate);
        assert!(cost >= 0.0);
        assert!(cost.is_finite());
    }
}
```

### 4. Benchmark Suite
```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_parsing(c: &mut Criterion) {
    c.bench_function("parse_1000_entries", |b| {
        b.iter(|| parse_jsonl_file("bench_data/1000.jsonl"))
    });
}
```

## Migration Plan

### Phase 1: Core Infrastructure (Week 1-2)
- Data types and domain models
- Basic file discovery and parsing
- Error handling framework
- Unit test infrastructure

### Phase 2: Processing Engine (Week 3-4)
- Streaming parser implementation
- Aggregation logic
- Cost calculation
- Integration with pricing data

### Phase 3: CLI Implementation (Week 5-6)
- Command parsing with clap
- Report generation
- Output formatting
- End-to-end testing

### Phase 4: Advanced Features (Week 7-8)
- MCP server implementation
- Live monitoring mode
- Performance optimization
- Memory profiling

### Phase 5: Polish and Release (Week 9-10)
- Documentation completion
- Cross-platform testing
- Performance benchmarking
- Release preparation

## Risk Mitigation

### 1. Technical Risks
- **Risk**: Incompatible JSONL format changes
- **Mitigation**: Version detection and migration logic

### 2. Performance Risks
- **Risk**: Large files causing memory spikes
- **Mitigation**: Streaming with backpressure

### 3. Compatibility Risks
- **Risk**: Platform-specific issues
- **Mitigation**: CI testing on all platforms

### 4. Data Risks
- **Risk**: Corrupted usage files
- **Mitigation**: Validation and recovery logic

## Success Metrics

### 1. Performance Metrics
- Memory usage: <50% of TypeScript version
- Processing speed: >2x faster
- Startup time: <50ms
- Report generation: <100ms

### 2. Quality Metrics
- Test coverage: >80%
- Zero memory leaks
- No panics in production
- All clippy lints pass

### 3. User Metrics
- Feature parity: 100%
- Backward compatibility: 100%
- User satisfaction: No regressions
- Issue resolution: <48 hours

---

This design plan provides a comprehensive blueprint for implementing ccusage in Rust, with clear user stories, technical designs, and success criteria. The phased approach ensures incremental delivery while maintaining quality and performance goals.