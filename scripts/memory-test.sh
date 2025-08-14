#!/bin/bash
# Memory leak testing script for ccstat
# Uses valgrind and other tools to check for memory issues

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check for required tools
check_tools() {
    local missing_tools=()

    if ! command -v valgrind &> /dev/null; then
        missing_tools+=("valgrind")
    fi

    if [ ${#missing_tools[@]} -ne 0 ]; then
        echo -e "${RED}Error: Missing required tools:${NC}"
        printf '%s\n' "${missing_tools[@]}"
        echo
        echo "Install on Ubuntu/Debian: sudo apt-get install valgrind"
        echo "Install on macOS: brew install valgrind"
        echo "Install on Fedora: sudo dnf install valgrind"
        exit 1
    fi
}

# Function to run valgrind test
run_valgrind_test() {
    local test_name="$1"
    local command="$2"

    echo -e "\n${YELLOW}Testing: $test_name${NC}"

    # Run with valgrind
    valgrind \
        --leak-check=full \
        --show-leak-kinds=all \
        --track-origins=yes \
        --verbose \
        --log-file=valgrind-$test_name.log \
        $command 2>&1 | tail -20

    # Check results
    if grep -q "ERROR SUMMARY: 0 errors" valgrind-$test_name.log; then
        echo -e "${GREEN}✓ No memory errors detected${NC}"

        # Check for leaks
        if grep -q "definitely lost: 0 bytes" valgrind-$test_name.log && \
           grep -q "indirectly lost: 0 bytes" valgrind-$test_name.log; then
            echo -e "${GREEN}✓ No memory leaks detected${NC}"
        else
            echo -e "${YELLOW}⚠ Possible memory leaks detected${NC}"
            grep "LEAK SUMMARY" -A 5 valgrind-$test_name.log
        fi
    else
        echo -e "${RED}✗ Memory errors detected${NC}"
        grep "ERROR SUMMARY" valgrind-$test_name.log
        return 1
    fi
}

# Function to run stress test
run_stress_test() {
    echo -e "\n${YELLOW}=== Running Memory Stress Test ===${NC}"

    # Generate large test dataset
    echo "Generating large test dataset..."
    ./scripts/generate-test-data.sh stress-test-data 100 20 50

    # Monitor memory usage
    echo "Running stress test with memory monitoring..."

    # Get initial memory
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        initial_mem=$(ps -o rss= -p $$ | awk '{print $1}')
    else
        # Linux
        initial_mem=$(ps -o rss= -p $$ | awk '{print $1}')
    fi

    # Run ccstat multiple times
    export CLAUDE_DATA_PATH=stress-test-data
    for i in {1..10}; do
        echo -n "Run $i: "
        ccstat daily --intern --arena > /dev/null 2>&1

        # Check memory after each run
        if [[ "$OSTYPE" == "darwin"* ]]; then
            current_mem=$(ps -o rss= -p $$ | awk '{print $1}')
        else
            current_mem=$(ps -o rss= -p $$ | awk '{print $1}')
        fi

        mem_increase=$((current_mem - initial_mem))
        echo "Memory increase: ${mem_increase}KB"

        # If memory keeps growing significantly, there might be a leak
        if [ $mem_increase -gt 100000 ]; then
            echo -e "${RED}⚠ Significant memory increase detected${NC}"
        fi
    done

    # Cleanup
    rm -rf stress-test-data
}

# Function to test with miri (if available)
run_miri_test() {
    echo -e "\n${YELLOW}=== Running Miri Test ===${NC}"

    # Check if miri is installed
    if ! rustup component list | grep -q "miri.*installed"; then
        echo "Miri not installed. Install with: rustup +nightly component add miri"
        echo "Skipping miri tests..."
        return
    fi

    # Run tests under miri
    echo "Running unit tests under miri..."
    cargo +nightly miri test --lib 2>&1 | tee miri-test.log

    if grep -q "test result: ok" miri-test.log; then
        echo -e "${GREEN}✓ Miri tests passed${NC}"
    else
        echo -e "${RED}✗ Miri tests failed${NC}"
    fi
}

# Function to profile memory allocations
profile_memory() {
    echo -e "\n${YELLOW}=== Memory Profiling ===${NC}"

    # Use heaptrack if available
    if command -v heaptrack &> /dev/null; then
        echo "Running heaptrack profiling..."
        heaptrack ccstat daily --since 2024-01-01 --until 2024-01-31
        echo "Analyze with: heaptrack_gui heaptrack.ccstat.*.gz"
    else
        echo "heaptrack not available. Install for detailed memory profiling."
    fi

    # Basic memory stats using /usr/bin/time
    if command -v /usr/bin/time &> /dev/null; then
        echo -e "\nRunning with time -v for memory stats..."
        /usr/bin/time -v ccstat daily 2>&1 | grep -E "(Maximum resident|Major|Minor)"
    fi
}

# Main execution
echo "=== ccstat Memory Testing Suite ==="
echo "This script tests for memory leaks and issues"
echo

# Check for required tools
check_tools

# Check if ccstat is built
if [ ! -f "target/release/ccstat" ]; then
    echo "Building ccstat in release mode..."
    cargo build --release
fi

# Create test data if needed
if [ ! -d "test-data" ]; then
    echo "Creating test data..."
    ./scripts/generate-test-data.sh
fi

# Set up environment
export CLAUDE_DATA_PATH=test-data

# Run valgrind tests
echo -e "\n${YELLOW}=== Valgrind Memory Tests ===${NC}"
run_valgrind_test "daily" "target/release/ccstat daily"
run_valgrind_test "monthly" "target/release/ccstat monthly"
run_valgrind_test "session" "target/release/ccstat session"
run_valgrind_test "blocks" "target/release/ccstat blocks"
run_valgrind_test "performance" "target/release/ccstat daily --intern --arena"

# Run stress test
run_stress_test

# Run miri test (if available)
run_miri_test

# Profile memory
profile_memory

# Cleanup
echo -e "\n${YELLOW}=== Cleanup ===${NC}"
rm -f valgrind-*.log miri-test.log

echo -e "\n${GREEN}=== Memory Testing Complete ===${NC}"
echo "Check valgrind-*.log files for detailed reports"
