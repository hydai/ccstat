# ccstat Architecture Documentation

## Overview

ccstat is a Rust CLI application designed to analyze Claude API usage data from local JSONL files. The architecture emphasizes performance, memory efficiency, and modularity.

## Design Principles

1. **Streaming Processing**: Handle large datasets without loading everything into memory
2. **Type Safety**: Leverage Rust's type system with newtype pattern for domain modeling
3. **Async/Await**: Non-blocking I/O for file and network operations
4. **Zero-Copy Parsing**: Minimize allocations during JSON parsing
5. **Modular Design**: Clear separation of concerns with focused modules

## System Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌───────────────┐
│   CLI (main)    │────▶│  Command Parser  │────▶│   Aggregator  │
└─────────────────┘     └──────────────────┘     └───────────────┘
                                │                         │
                                ▼                         ▼
                        ┌──────────────────┐     ┌───────────────┐
                        │   Data Loader    │     │ Cost Calculator│
                        └──────────────────┘     └───────────────┘
                                │                         │
                                ▼                         ▼
                        ┌──────────────────┐     ┌───────────────┐
                        │  JSONL Parser    │     │ Pricing Fetcher│
                        └──────────────────┘     └───────────────┘
```

## Module Structure

### Core Modules

#### `types.rs`
Defines domain types using newtype pattern for type safety:
- `ModelName`: Strongly-typed model identifier
- `SessionId`: UUID wrapper for session tracking
- `TokenCounts`: Token usage with arithmetic operations
- `UsageEntry`: Core data structure from JSONL

**Design Decision**: Newtype pattern prevents mixing different string types and enables domain-specific methods.

#### `data_loader.rs`
Handles platform-specific data discovery and streaming:
- Auto-discovers Claude data directories
- Provides async streaming API
- Supports parallel file processing
- Implements deduplication logic

**Design Decision**: Streaming prevents memory exhaustion with large datasets.

#### `aggregation.rs`
Time-based data aggregation engine:
- Daily, monthly, session, and billing block aggregations
- Accumulator pattern for efficient aggregation
- Support for verbose mode with detailed entries

**Design Decision**: Accumulator pattern allows single-pass aggregation.

#### `cost_calculator.rs`
Token-based cost calculation:
- Integrates with pricing data
- Supports multiple cost modes (auto/calculate/display)
- Caches pricing data for performance

**Design Decision**: Separation from aggregation allows flexible cost strategies.

### Support Modules

#### `pricing_fetcher.rs`
LiteLLM API client with caching:
- Fetches current model pricing
- Falls back to embedded data
- In-memory caching with 1-hour TTL

#### `filters.rs`
Flexible filtering system:
- Date range filtering
- Project-based filtering
- Composable filter design

#### `output.rs`
Output formatting with trait-based design:
- `OutputFormatter` trait for extensibility
- Table formatter for human-readable output
- JSON formatter for machine processing

### Optimization Modules

#### `memory_pool.rs`
Arena allocation for reduced fragmentation:
- Pre-allocates memory blocks
- Reduces allocator pressure
- Optional performance optimization

#### `string_pool.rs`
String interning for memory efficiency:
- Deduplicates repeated strings (models, projects)
- Significant memory savings with large datasets
- Thread-safe implementation

## Data Flow

1. **Discovery Phase**
   ```
   CLI → DataLoader → Platform-specific paths → JSONL files
   ```

2. **Parsing Phase**
   ```
   JSONL files → Async reader → Serde deserializer → RawJsonlEntry → UsageEntry
   ```

3. **Aggregation Phase**
   ```
   Stream<UsageEntry> → Aggregator → Accumulator → AggregatedData
   ```

4. **Output Phase**
   ```
   AggregatedData → OutputFormatter → Table/JSON → stdout
   ```

## Performance Optimizations

### Parallel Processing
- Rayon for CPU-bound operations
- Tokio for I/O-bound operations
- Work-stealing for load balancing

### Memory Optimizations
- String interning reduces memory by ~60% for repeated strings
- Arena allocation improves cache locality
- Streaming prevents loading entire dataset

### Algorithmic Optimizations
- O(1) token arithmetic with overflow checking
- Single-pass aggregation
- Pre-sorted output using BTreeMap

## Error Handling

Comprehensive error handling with `thiserror`:

```rust
pub enum CcstatError {
    IoError(std::io::Error),
    ParseError(serde_json::Error),
    InvalidDate(String),
    UnknownModel(ModelName),
    // ... more variants
}
```

All errors bubble up with context preservation.

## Async Architecture

### Tokio Runtime
- Multi-threaded runtime for concurrent I/O
- Stream-based processing with `futures::Stream`
- Backpressure handling in data pipeline

### Concurrent File Processing
```rust
// Parallel file discovery
let files = discover_files().await?;

// Concurrent parsing with bounded parallelism
let stream = futures::stream::iter(files)
    .map(|file| parse_file(file))
    .buffer_unordered(num_cpus::get());
```

## Configuration

### Build-time Configuration
- Feature flags for optional dependencies
- Profile-based optimization settings
- Cross-compilation support

### Runtime Configuration
- Environment variables (CLAUDE_DATA_PATH, RUST_LOG)
- CLI arguments with clap
- Platform-specific defaults

## Testing Strategy

### Unit Tests
- Module-level testing with mocks
- Property-based testing with proptest
- Edge case coverage

### Integration Tests
- End-to-end data flow testing
- Cross-platform path handling
- Error scenario testing

### Performance Tests
- Criterion benchmarks for hot paths
- Memory usage profiling
- Large dataset stress testing

## Security Considerations

1. **Path Traversal**: Validated file paths
2. **JSON Parsing**: Size limits on input
3. **Network Security**: HTTPS-only for API calls
4. **No Sensitive Data**: No credentials stored

## Future Considerations

### Extensibility Points
1. Additional output formats (CSV, Excel)
2. Plugin system for custom aggregations
3. Real-time monitoring capabilities
4. Database export options

### Performance Improvements
1. SIMD optimization for token arithmetic
2. Memory-mapped file support
3. Compression for cached data
4. GPU acceleration for large aggregations

## Platform-Specific Considerations

### macOS
- Keychain integration for API keys (future)
- Spotlight indexing exemption

### Linux
- XDG base directory compliance
- systemd service support (future)

### Windows
- Properly handles path separators
- Windows credential store (future)

## MCP Server Architecture

The MCP (Model Context Protocol) server provides API access:

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│ MCP Client  │────▶│  MCP Server  │────▶│  Aggregator  │
└─────────────┘     └──────────────┘     └──────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │   Transport   │
                    │ (stdio/HTTP)  │
                    └──────────────┘
```

## Conclusion

ccstat's architecture balances performance, maintainability, and extensibility. The modular design allows for easy testing and future enhancements while the streaming architecture ensures scalability to large datasets.