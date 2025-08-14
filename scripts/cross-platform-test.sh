#!/bin/bash
# Cross-platform testing script for ccstat
# Tests all major functionality to ensure consistency across platforms

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Function to print test results
print_test_result() {
    local test_name="$1"
    local result="$2"

    if [ "$result" = "PASS" ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗${NC} $test_name"
        ((TESTS_FAILED++))
    fi
}

# Function to run a test
run_test() {
    local test_name="$1"
    local command="$2"
    local expected_pattern="$3"

    echo -n "Testing: $test_name... "

    if output=$($command 2>&1); then
        if echo "$output" | grep -q "$expected_pattern" 2>/dev/null || [ -z "$expected_pattern" ]; then
            print_test_result "$test_name" "PASS"
        else
            print_test_result "$test_name" "FAIL"
            echo "  Expected pattern: $expected_pattern"
            echo "  Got: $output" | head -5
        fi
    else
        print_test_result "$test_name" "FAIL"
        echo "  Command failed: $command"
        echo "  Error: $output" | head -5
    fi
}

# Check if ccstat is available
if ! command -v ccstat &> /dev/null; then
    echo -e "${RED}Error: ccstat not found in PATH${NC}"
    echo "Please build and install ccstat first:"
    echo "  cargo build --release"
    echo "  cargo install --path ."
    exit 1
fi

echo "=== ccstat Cross-Platform Test Suite ==="
echo "Platform: $(uname -s)"
echo "Architecture: $(uname -m)"
echo "ccstat version: $(ccstat --version)"
echo

# Basic functionality tests
echo "=== Basic Command Tests ==="
run_test "Help command" "ccstat --help" "Analyze Claude Code usage"
run_test "Version command" "ccstat --version" "ccstat"
run_test "Daily command (no args)" "ccstat daily" ""
run_test "Monthly command (no args)" "ccstat monthly" ""
run_test "Session command (no args)" "ccstat session" ""
run_test "Blocks command (no args)" "ccstat blocks" ""

# Test with date filters
echo -e "\n=== Date Filter Tests ==="
run_test "Daily with since date" "ccstat daily --since 2024-01-01" ""
run_test "Daily with date range" "ccstat daily --since 2024-01-01 --until 2024-01-31" ""
run_test "Monthly with month filter" "ccstat monthly --since 2024-01 --until 2024-12" ""
run_test "Invalid date format" "ccstat daily --since invalid-date" "Invalid date" || true

# Test output formats
echo -e "\n=== Output Format Tests ==="
run_test "JSON output for daily" "ccstat daily --json" '"daily"'
run_test "JSON output for monthly" "ccstat monthly --json" '"monthly"'
run_test "JSON output for sessions" "ccstat session --json" '"sessions"'
run_test "JSON output for blocks" "ccstat blocks --json" '"blocks"'

# Test cost modes
echo -e "\n=== Cost Mode Tests ==="
run_test "Auto cost mode" "ccstat daily --mode auto" ""
run_test "Calculate cost mode" "ccstat daily --mode calculate" ""
run_test "Display cost mode" "ccstat daily --mode display" ""
run_test "Invalid cost mode" "ccstat daily --mode invalid" "Invalid value" || true

# Test performance flags
echo -e "\n=== Performance Flag Tests ==="
run_test "String interning" "ccstat daily --intern" ""
run_test "Arena allocation" "ccstat daily --arena" ""
run_test "All performance flags" "ccstat daily --intern --arena" ""

# Test detailed mode
echo -e "\n=== Detailed Mode Tests ==="
run_test "Detailed daily output" "ccstat daily --detailed --since 2024-01-01 --until 2024-01-01" ""

# Test project filtering
echo -e "\n=== Project Filter Tests ==="
run_test "Project filter" "ccstat daily --project test-project" ""

# Test instance grouping
echo -e "\n=== Instance Grouping Tests ==="
run_test "Daily by instance" "ccstat daily --by-instance" ""

# Test session options
echo -e "\n=== Session Command Tests ==="
run_test "Sessions with models" "ccstat session --models" ""

# Test billing blocks options
echo -e "\n=== Billing Blocks Tests ==="
run_test "Active blocks only" "ccstat blocks --active" ""
run_test "Recent blocks" "ccstat blocks --recent" ""
run_test "Blocks with limit" "ccstat blocks --limit 1000000" ""

# Test error handling
echo -e "\n=== Error Handling Tests ==="
run_test "Invalid subcommand" "ccstat invalid-command" "unrecognized subcommand" || true
run_test "Conflicting date args" "ccstat daily --until 2024-01-01 --since 2024-12-31" "" || true

# Platform-specific tests
echo -e "\n=== Platform-Specific Tests ==="
case "$(uname -s)" in
    Darwin)
        echo "Testing macOS-specific paths..."
        run_test "macOS data discovery" "RUST_LOG=ccstat=debug ccstat daily 2>&1" "Library/Application Support/Claude"
        ;;
    Linux)
        echo "Testing Linux-specific paths..."
        run_test "Linux data discovery" "RUST_LOG=ccstat=debug ccstat daily 2>&1" ".config/Claude"
        ;;
    MINGW*|CYGWIN*|MSYS*)
        echo "Testing Windows-specific paths..."
        run_test "Windows data discovery" "RUST_LOG=ccstat=debug ccstat daily 2>&1" "AppData"
        ;;
esac

# Test environment variable override
echo -e "\n=== Environment Variable Tests ==="
run_test "CLAUDE_DATA_PATH override" "CLAUDE_DATA_PATH=/tmp/test ccstat daily 2>&1" ""

# Summary
echo -e "\n=== Test Summary ==="
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
fi
