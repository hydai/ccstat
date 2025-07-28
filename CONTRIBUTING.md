# Contributing to ccusage

Thank you for your interest in contributing to ccusage! This document provides guidelines and instructions for contributing to the project.

## Code of Conduct

By participating in this project, you agree to abide by our code of conduct: be respectful, inclusive, and constructive in all interactions.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/yourusername/ccusage.git
   cd ccusage
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/originalrepo/ccusage.git
   ```
4. Create a feature branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Setup

### Prerequisites

- Rust 1.75.0 or later
- Cargo (comes with Rust)
- Git

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run the program
cargo run -- daily
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests in release mode
cargo test --release
```

### Code Quality

Before submitting a PR, ensure your code passes all checks:

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Check documentation
cargo doc --no-deps

# Run all checks
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Project Structure

```
ccusage/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library root
│   ├── error.rs         # Error types
│   ├── types.rs         # Domain types
│   ├── data_loader.rs   # JSONL file loading
│   ├── pricing_fetcher.rs # Pricing data
│   ├── cost_calculator.rs # Cost calculations
│   ├── aggregation.rs   # Data aggregation
│   ├── cli.rs           # CLI parsing
│   ├── output.rs        # Output formatting
│   └── mcp.rs           # MCP server
├── tests/               # Integration tests
├── benches/             # Benchmarks
├── examples/            # Example usage
└── embedded/            # Embedded data files
```

## Making Changes

### Coding Standards

1. **Follow Rust conventions**: Use `cargo fmt` and follow Rust API guidelines
2. **Use strong typing**: Prefer newtypes over primitive types
3. **Handle errors properly**: Use `Result<T, E>` and avoid `unwrap()` in production code
4. **Write tests**: Every new feature should have tests
5. **Document public APIs**: All public functions need documentation
6. **Keep it simple**: Prefer clarity over cleverness

### Commit Messages

Follow conventional commits format:

```
type(scope): subject

body

footer
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Test additions or fixes
- `chore`: Build process or auxiliary tool changes

Example:
```
feat(aggregation): add support for hourly reports

Implement hourly aggregation to complement existing daily/monthly reports.
This allows for more granular usage tracking.

Closes #123
```

### Pull Request Process

1. **Update your fork**:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Make your changes**:
   - Write clean, documented code
   - Add tests for new functionality
   - Update documentation if needed

3. **Test thoroughly**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

4. **Push to your fork**:
   ```bash
   git push origin feature/your-feature-name
   ```

5. **Create Pull Request**:
   - Use a clear, descriptive title
   - Reference any related issues
   - Describe what changes you made and why
   - Include screenshots for UI changes

### Pull Request Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing completed

## Checklist
- [ ] Code follows project style
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] Tests added/updated
- [ ] No warnings from clippy
```

## Testing Guidelines

### Unit Tests

Place unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Test implementation
    }
}
```

### Integration Tests

Place integration tests in `tests/` directory:

```rust
// tests/integration_test.rs
use ccusage::*;

#[test]
fn test_end_to_end_workflow() {
    // Test implementation
}
```

### Benchmarks

Add benchmarks for performance-critical code:

```rust
// benches/parsing.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_jsonl_parsing(c: &mut Criterion) {
    c.bench_function("parse 1000 entries", |b| {
        b.iter(|| {
            // Benchmark code
        })
    });
}
```

## Documentation

### Code Documentation

Document all public items:

```rust
/// Calculate the cost for token usage.
///
/// # Arguments
///
/// * `tokens` - Token counts for the usage
/// * `model_name` - Name of the model used
///
/// # Returns
///
/// The calculated cost in USD
///
/// # Errors
///
/// Returns `CcusageError::UnknownModel` if the model pricing is not found
pub async fn calculate_cost(
    &self,
    tokens: &TokenCounts,
    model_name: &ModelName,
) -> Result<f64> {
    // Implementation
}
```

### README Updates

Update the README when adding:
- New features
- New command-line options
- Breaking changes
- New dependencies

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Create a git tag: `git tag -a v0.1.0 -m "Release version 0.1.0"`
4. Push tag: `git push upstream v0.1.0`
5. GitHub Actions will build and create release

## Getting Help

- Open an issue for bugs or feature requests
- Join discussions in existing issues
- Ask questions in pull requests
- Check existing documentation

## Recognition

Contributors will be recognized in:
- The project README
- Release notes
- GitHub contributors page

Thank you for contributing to ccusage!
