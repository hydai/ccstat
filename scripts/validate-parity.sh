#!/bin/bash
# Feature parity validation script
# Compares ccstat output with expected results

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test data directory
TEST_DATA_DIR="${TEST_DATA_DIR:-test-data}"

# Function to create reference test data
create_reference_data() {
    echo "Creating reference test data..."
    
    # Create a simple test case with known values
    mkdir -p "$TEST_DATA_DIR"
    
    # Create a single day's data with predictable values
    cat > "$TEST_DATA_DIR/usage-2024-01-15.jsonl" <<'EOF'
{"sessionId":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2024-01-15T10:00:00Z","message":{"model":"claude-3-opus","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}},"type":"assistant","uuid":"123e4567-e89b-12d3-a456-426614174000","costUSD":0.0255}
{"sessionId":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2024-01-15T10:30:00Z","message":{"model":"claude-3-opus","usage":{"input_tokens":2000,"output_tokens":1000,"cache_creation_input_tokens":200,"cache_read_input_tokens":100}},"type":"assistant","uuid":"223e4567-e89b-12d3-a456-426614174001","costUSD":0.051}
{"sessionId":"660e8400-e29b-41d4-a716-446655440001","timestamp":"2024-01-15T14:00:00Z","message":{"model":"claude-3-sonnet","usage":{"input_tokens":500,"output_tokens":250,"cache_creation_input_tokens":50,"cache_read_input_tokens":25}},"type":"assistant","uuid":"323e4567-e89b-12d3-a456-426614174002","costUSD":0.00375}
EOF
}

# Function to validate daily aggregation
validate_daily() {
    echo -e "\n${YELLOW}=== Validating Daily Aggregation ===${NC}"
    
    # Run ccstat
    output=$(CLAUDE_DATA_PATH="$TEST_DATA_DIR" ccstat daily --json --since 2024-01-15 --until 2024-01-15)
    
    # Extract values using jq or grep
    if command -v jq &> /dev/null; then
        input_tokens=$(echo "$output" | jq -r '.daily[0].tokens.input_tokens')
        output_tokens=$(echo "$output" | jq -r '.daily[0].tokens.output_tokens')
        total_cost=$(echo "$output" | jq -r '.daily[0].total_cost')
        models_count=$(echo "$output" | jq -r '.daily[0].models_used | length')
    else
        # Fallback to grep if jq not available
        input_tokens=$(echo "$output" | grep -o '"input_tokens":[0-9]*' | head -1 | cut -d: -f2)
        output_tokens=$(echo "$output" | grep -o '"output_tokens":[0-9]*' | head -1 | cut -d: -f2)
        total_cost=$(echo "$output" | grep -o '"total_cost":[0-9.]*' | head -1 | cut -d: -f2)
    fi
    
    # Expected values
    expected_input=3500
    expected_output=1750
    expected_cost=0.08025  # Based on known pricing
    
    # Validate
    echo "Input tokens: $input_tokens (expected: $expected_input)"
    echo "Output tokens: $output_tokens (expected: $expected_output)"
    echo "Total cost: $total_cost (expected: ~$expected_cost)"
    
    if [ "$input_tokens" = "$expected_input" ]; then
        echo -e "${GREEN}✓ Input tokens match${NC}"
    else
        echo -e "${RED}✗ Input tokens mismatch${NC}"
        return 1
    fi
    
    if [ "$output_tokens" = "$expected_output" ]; then
        echo -e "${GREEN}✓ Output tokens match${NC}"
    else
        echo -e "${RED}✗ Output tokens mismatch${NC}"
        return 1
    fi
    
    # Check cost within tolerance (floating point)
    if command -v bc &> /dev/null; then
        cost_diff=$(echo "scale=6; $total_cost - $expected_cost" | bc | tr -d -)
        if (( $(echo "$cost_diff < 0.01" | bc -l) )); then
            echo -e "${GREEN}✓ Cost calculation correct${NC}"
        else
            echo -e "${RED}✗ Cost calculation mismatch${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠ Skipping cost validation (bc not available)${NC}"
    fi
}

# Function to validate session aggregation
validate_sessions() {
    echo -e "\n${YELLOW}=== Validating Session Aggregation ===${NC}"
    
    # Run ccstat
    output=$(CLAUDE_DATA_PATH="$TEST_DATA_DIR" ccstat session --json --since 2024-01-15 --until 2024-01-15)
    
    if command -v jq &> /dev/null; then
        session_count=$(echo "$output" | jq -r '.sessions | length')
        first_session_id=$(echo "$output" | jq -r '.sessions[0].session_id')
        duration_seconds=$(echo "$output" | jq -r '.sessions[0].duration_seconds')
    else
        session_count=$(echo "$output" | grep -o '"session_id"' | wc -l)
    fi
    
    echo "Session count: $session_count (expected: 2)"
    
    if [ "$session_count" = "2" ]; then
        echo -e "${GREEN}✓ Session count correct${NC}"
    else
        echo -e "${RED}✗ Session count mismatch${NC}"
        return 1
    fi
}

# Function to validate monthly aggregation
validate_monthly() {
    echo -e "\n${YELLOW}=== Validating Monthly Aggregation ===${NC}"
    
    # Run ccstat
    output=$(CLAUDE_DATA_PATH="$TEST_DATA_DIR" ccstat monthly --json --since 2024-01 --until 2024-01)
    
    if command -v jq &> /dev/null; then
        month=$(echo "$output" | jq -r '.monthly[0].month')
        active_days=$(echo "$output" | jq -r '.monthly[0].active_days')
    else
        month=$(echo "$output" | grep -o '"month":"[^"]*"' | head -1 | cut -d'"' -f4)
        active_days=$(echo "$output" | grep -o '"active_days":[0-9]*' | head -1 | cut -d: -f2)
    fi
    
    echo "Month: $month (expected: 2024-01)"
    echo "Active days: $active_days (expected: 1)"
    
    if [ "$month" = "2024-01" ] && [ "$active_days" = "1" ]; then
        echo -e "${GREEN}✓ Monthly aggregation correct${NC}"
    else
        echo -e "${RED}✗ Monthly aggregation mismatch${NC}"
        return 1
    fi
}

# Function to validate billing blocks
validate_blocks() {
    echo -e "\n${YELLOW}=== Validating Billing Blocks ===${NC}"
    
    # Run ccstat
    output=$(CLAUDE_DATA_PATH="$TEST_DATA_DIR" ccstat blocks --json)
    
    if command -v jq &> /dev/null; then
        block_count=$(echo "$output" | jq -r '.blocks | length')
    else
        block_count=$(echo "$output" | grep -o '"start_time"' | wc -l)
    fi
    
    echo "Billing blocks: $block_count"
    
    if [ "$block_count" -ge "1" ]; then
        echo -e "${GREEN}✓ Billing blocks generated${NC}"
    else
        echo -e "${RED}✗ No billing blocks found${NC}"
        return 1
    fi
}

# Function to test error handling
validate_error_handling() {
    echo -e "\n${YELLOW}=== Validating Error Handling ===${NC}"
    
    # Test invalid date
    if ccstat daily --since "invalid-date" 2>&1 | grep -q "Invalid date"; then
        echo -e "${GREEN}✓ Invalid date error handling${NC}"
    else
        echo -e "${RED}✗ Invalid date not caught${NC}"
    fi
    
    # Test future date range
    if ccstat daily --since "2025-01-01" --until "2024-01-01" 2>&1 | grep -q -E "(Invalid|Error)"; then
        echo -e "${GREEN}✓ Invalid date range handling${NC}"
    else
        echo -e "${YELLOW}⚠ Invalid date range not caught${NC}"
    fi
}

# Main execution
echo "=== ccstat Feature Parity Validation ==="
echo "This script validates ccstat output against expected values"
echo

# Check if ccstat is available
if ! command -v ccstat &> /dev/null; then
    echo -e "${RED}Error: ccstat not found in PATH${NC}"
    exit 1
fi

# Create reference data
create_reference_data

# Run validations
validate_daily
validate_sessions
validate_monthly
validate_blocks
validate_error_handling

echo -e "\n${GREEN}=== Validation Complete ===${NC}"
echo "Note: This validates basic functionality. For complete parity testing,"
echo "compare outputs with the TypeScript implementation using identical data."