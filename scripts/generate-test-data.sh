#!/bin/bash
# Generate test JSONL data for ccstat testing

set -e

# Default values
OUTPUT_DIR="${1:-test-data}"
NUM_DAYS="${2:-30}"
SESSIONS_PER_DAY="${3:-5}"
CALLS_PER_SESSION="${4:-10}"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Models to use
MODELS=("claude-3-opus" "claude-3-sonnet" "claude-3-haiku" "claude-3.5-sonnet")

# Function to generate a UUID
generate_uuid() {
    if command -v uuidgen &> /dev/null; then
        uuidgen | tr '[:upper:]' '[:lower:]'
    else
        # Fallback UUID generation
        printf '%08x-%04x-%04x-%04x-%012x' \
            $RANDOM $RANDOM $RANDOM $RANDOM $RANDOM$RANDOM
    fi
}

# Function to generate a JSONL entry
generate_entry() {
    local session_id="$1"
    local timestamp="$2"
    local model="$3"
    local input_tokens=$((RANDOM % 10000 + 100))
    local output_tokens=$((input_tokens / 2 + RANDOM % 1000))
    local cache_creation=$((RANDOM % 100))
    local cache_read=$((RANDOM % 500))
    local cost=$(echo "scale=6; ($input_tokens * 0.00001 + $output_tokens * 0.00003)" | bc)
    
    cat <<EOF
{"sessionId":"$session_id","timestamp":"$timestamp","message":{"model":"$model","usage":{"input_tokens":$input_tokens,"output_tokens":$output_tokens,"cache_creation_input_tokens":$cache_creation,"cache_read_input_tokens":$cache_read},"id":"msg_$(generate_uuid)"},"type":"assistant","uuid":"$(generate_uuid)","cwd":"/home/user/test-project","costUSD":$cost,"version":"0.1.0"}
EOF
}

echo "Generating test data..."
echo "Output directory: $OUTPUT_DIR"
echo "Days: $NUM_DAYS"
echo "Sessions per day: $SESSIONS_PER_DAY"
echo "Calls per session: $CALLS_PER_SESSION"

# Generate data for each day
for day in $(seq 0 $((NUM_DAYS - 1))); do
    # Calculate date
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        date_str=$(date -v -${day}d '+%Y-%m-%d')
    else
        # Linux
        date_str=$(date -d "$day days ago" '+%Y-%m-%d')
    fi
    
    output_file="$OUTPUT_DIR/usage-$date_str.jsonl"
    
    echo "Generating data for $date_str..."
    
    # Clear file
    > "$output_file"
    
    # Generate sessions for this day
    for session in $(seq 1 $SESSIONS_PER_DAY); do
        session_id=$(generate_uuid)
        
        # Random start hour for session
        start_hour=$((RANDOM % 20))
        
        # Generate calls within session
        for call in $(seq 1 $CALLS_PER_SESSION); do
            # Calculate timestamp with some randomness
            minutes=$((call * 5 + RANDOM % 10))
            timestamp="${date_str}T$(printf %02d $start_hour):$(printf %02d $minutes):00Z"
            
            # Pick a random model
            model=${MODELS[$((RANDOM % ${#MODELS[@]}))]}
            
            # Generate entry
            generate_entry "$session_id" "$timestamp" "$model" >> "$output_file"
        done
    done
    
    echo "  Generated $(wc -l < "$output_file") entries"
done

# Generate a summary
echo
echo "Test data generation complete!"
echo "Total files created: $(ls -1 "$OUTPUT_DIR"/*.jsonl 2>/dev/null | wc -l)"
echo "Total entries: $(cat "$OUTPUT_DIR"/*.jsonl 2>/dev/null | wc -l)"
echo
echo "To use this test data, set:"
echo "  export CLAUDE_DATA_PATH=$OUTPUT_DIR"
echo "  ccstat daily"