#!/bin/bash
# Coverage analysis script for ccstat

echo "Running code coverage analysis..."

# Run tests with coverage
echo "Generating coverage data..."
cargo llvm-cov --no-report

# Generate coverage report
echo "Creating coverage report..."
cargo llvm-cov report --summary-only

# Generate detailed HTML report
echo "Creating HTML report..."
cargo llvm-cov --html

echo "Coverage report generated in target/llvm-cov/html/index.html"

# Display summary
echo ""
echo "Coverage Summary:"
cargo llvm-cov report --summary-only | tail -1