# ccstat User Guide

This guide provides comprehensive documentation for using ccstat to analyze your Claude API usage.

## Table of Contents

- [Getting Started](#getting-started)
- [Command Reference](#command-reference)
- [Common Use Cases](#common-use-cases)
- [Advanced Features](#advanced-features)
- [Tips and Tricks](#tips-and-tricks)
- [FAQ](#faq)

## Getting Started

### First Run

After installation, verify ccstat can find your Claude data:

```bash
# Check if data is found
ccstat daily

# If no data is found, check the discovery paths
RUST_LOG=ccstat=debug ccstat daily
```

### Basic Commands

The four main commands you'll use most often:

1. **Daily usage**: `ccstat daily`
2. **Monthly summary**: `ccstat monthly`
3. **Session details**: `ccstat session`
4. **Billing blocks**: `ccstat blocks`

## Command Reference

### Global Options

These options work with all commands:

- `--json`: Output in JSON format instead of tables
- `--help`: Show help for any command

### Daily Command

Show token usage aggregated by day.

```bash
ccstat daily [OPTIONS]
```

**Options:**
- `--since <DATE>`: Start date (YYYY-MM-DD)
- `--until <DATE>`: End date (YYYY-MM-DD)
- `--project <NAME>`: Filter by project name
- `--mode <MODE>`: Cost calculation mode (auto/calculate/display)
- `--verbose`: Show individual API calls
- `--parallel`: Enable parallel processing
- `--intern`: Use string interning
- `--arena`: Use arena allocation
- `--by-instance`: Group by instance ID

**Examples:**

```bash
# Today's usage
ccstat daily

# Last 7 days
ccstat daily --since $(date -d '7 days ago' +%Y-%m-%d)

# Specific project in January
ccstat daily --since 2024-01-01 --until 2024-01-31 --project my-project

# Detailed breakdown with individual calls
ccstat daily --verbose

# Optimized for large datasets
ccstat daily --parallel --intern --arena
```

### Monthly Command

Show usage aggregated by month.

```bash
ccstat monthly [OPTIONS]
```

**Options:**
- `--since <YYYY-MM>`: Start month
- `--until <YYYY-MM>`: End month
- `--project <NAME>`: Filter by project
- `--mode <MODE>`: Cost calculation mode

**Examples:**

```bash
# Current month
ccstat monthly

# Q1 2024
ccstat monthly --since 2024-01 --until 2024-03

# Full year 2024
ccstat monthly --since 2024-01 --until 2024-12
```

### Session Command

Show individual Claude sessions with duration and costs.

```bash
ccstat session [OPTIONS]
```

**Options:**
- `--since <DATE>`: Start date filter
- `--until <DATE>`: End date filter
- `--project <NAME>`: Filter by project
- `--mode <MODE>`: Cost calculation mode
- `--models`: Show models used in each session

**Examples:**

```bash
# All sessions
ccstat session

# Today's sessions
ccstat session --since $(date +%Y-%m-%d)

# Sessions with model details
ccstat session --models

# Export sessions for analysis
ccstat session --json > sessions.json
```

### Blocks Command

Show 5-hour billing blocks to track usage within billing periods.

```bash
ccstat blocks [OPTIONS]
```

**Options:**
- `--active`: Show only active blocks
- `--recent`: Show blocks from last 24 hours
- `--project <NAME>`: Filter by project
- `--limit <N>`: Token limit for warnings

**Examples:**

```bash
# All billing blocks
ccstat blocks

# Current active block
ccstat blocks --active

# Recent blocks with warnings for high usage
ccstat blocks --recent --limit 10000000
```

## Common Use Cases

### Daily Reporting

Create a daily usage report:

```bash
#!/bin/bash
# daily-report.sh

DATE=$(date +%Y-%m-%d)
echo "Claude Usage Report for $DATE"
echo "=============================="

# Daily summary
ccstat daily --since $DATE --until $DATE

# Active sessions
echo -e "\nActive Sessions:"
ccstat session --since $DATE --models

# Current billing block
echo -e "\nCurrent Billing Block:"
ccstat blocks --active
```

### Monthly Cost Tracking

Track costs over time:

```bash
# Current month costs
ccstat monthly

# Compare with previous month
CURRENT_MONTH=$(date +%Y-%m)
LAST_MONTH=$(date -d '1 month ago' +%Y-%m)

echo "This month:"
ccstat monthly --since $CURRENT_MONTH --until $CURRENT_MONTH

echo "Last month:"
ccstat monthly --since $LAST_MONTH --until $LAST_MONTH
```

### Project-Based Analysis

Analyze usage by project:

```bash
# List all projects (using jq)
ccstat daily --json | jq -r '.daily[].project' | sort -u

# Project-specific report
PROJECT="my-project"
ccstat daily --project $PROJECT
ccstat monthly --project $PROJECT
```

### Export for Spreadsheets

Export data for Excel/Google Sheets:

```bash
# Export daily data as CSV (using jq)
ccstat daily --json | jq -r '
  ["Date","Input","Output","Cache Create","Cache Read","Total","Cost"],
  (.daily[] | [
    .date,
    .tokens.input_tokens,
    .tokens.output_tokens,
    .tokens.cache_creation_tokens,
    .tokens.cache_read_tokens,
    .tokens.total,
    .total_cost
  ])
  | @csv'
```

## Advanced Features

### MCP Server Integration

Use ccstat as an MCP server for integration with other tools:

```bash
# Start MCP server on stdio
ccstat mcp

# HTTP server mode
ccstat mcp --transport http --port 8080
```

Example client request:

```python
import requests
import json

# Query daily usage via MCP
response = requests.post('http://localhost:8080/mcp', json={
    'jsonrpc': '2.0',
    'method': 'daily',
    'params': {
        'since': '2024-01-01',
        'until': '2024-01-31',
        'costMode': 'calculate'
    },
    'id': 1
})

data = response.json()
print(f"Total cost: ${data['result']['totals']['total_cost']:.2f}")
```

### Performance Optimization

For large datasets (millions of entries):

```bash
# Maximum performance mode
ccstat daily --parallel --intern --arena

# Benchmark different modes
time ccstat daily > /dev/null
time ccstat daily --parallel > /dev/null
time ccstat daily --parallel --intern --arena > /dev/null
```

### Custom Date Ranges

Flexible date filtering:

```bash
# Last 30 days
ccstat daily --since $(date -d '30 days ago' +%Y-%m-%d)

# Previous week
WEEK_START=$(date -d 'last monday' +%Y-%m-%d)
WEEK_END=$(date -d 'last sunday' +%Y-%m-%d)
ccstat daily --since $WEEK_START --until $WEEK_END

# Year to date
ccstat daily --since $(date +%Y)-01-01
```

## Tips and Tricks

### 1. Shell Aliases

Add to your `.bashrc` or `.zshrc`:

```bash
alias cctoday='ccstat daily --since $(date +%Y-%m-%d)'
alias ccweek='ccstat daily --since $(date -d "7 days ago" +%Y-%m-%d)'
alias ccmonth='ccstat monthly --since $(date +%Y-%m) --until $(date +%Y-%m)'
alias ccactive='ccstat blocks --active'
```

### 2. Watch Live Usage

Monitor usage in real-time:

```bash
# Update every 60 seconds
watch -n 60 'ccstat blocks --active'

# Or create a monitoring script
while true; do
  clear
  echo "Claude Usage Monitor - $(date)"
  echo "================================"
  ccstat blocks --active
  echo -e "\nToday's Usage:"
  ccstat daily --since $(date +%Y-%m-%d)
  sleep 300  # Update every 5 minutes
done
```

### 3. Cost Alerts

Create a script for cost alerts:

```bash
#!/bin/bash
# cost-alert.sh

LIMIT=100.0  # Daily cost limit
TODAY=$(date +%Y-%m-%d)

COST=$(ccstat daily --since $TODAY --json | jq -r '.totals.total_cost')

if (( $(echo "$COST > $LIMIT" | bc -l) )); then
  echo "ALERT: Daily cost ($COST) exceeds limit ($LIMIT)"
  # Send notification (email, slack, etc.)
fi
```

### 4. Backup Usage Data

Regularly backup your usage data:

```bash
# Create monthly backups
BACKUP_DIR="$HOME/claude-usage-backups"
MONTH=$(date +%Y-%m)

mkdir -p "$BACKUP_DIR"
ccstat monthly --json > "$BACKUP_DIR/usage-$MONTH.json"
```

## FAQ

### Q: How accurate are the cost calculations?

A: ccstat uses pricing data from LiteLLM's API. In 'calculate' mode, costs are computed based on current pricing. In 'auto' mode, pre-calculated costs from the usage logs are preferred when available.

### Q: Why don't I see any data?

A: Common reasons:
1. Claude Code hasn't been used yet
2. Data is in a non-standard location (set `CLAUDE_DATA_PATH`)
3. Permission issues (check directory read permissions)

### Q: How can I reduce memory usage for large datasets?

A: Use the optimization flags:
- `--parallel`: Process data in parallel
- `--intern`: Deduplicate strings in memory
- `--arena`: Use arena allocation for better memory layout

### Q: Can I use ccstat in scripts?

A: Yes! Use `--json` output for easy parsing. Exit codes:
- 0: Success
- 1: Error (check stderr for details)

### Q: How do billing blocks work?

A: Claude uses 5-hour rolling windows for billing. The `blocks` command shows these windows, helping you track usage within billing periods.

### Q: Can I filter by multiple projects?

A: Currently, only one project filter is supported at a time. Use multiple commands or process JSON output for multi-project analysis.

## Getting Help

- Run `ccstat --help` for general help
- Run `ccstat <command> --help` for command-specific help
- Enable debug logging: `RUST_LOG=ccstat=debug ccstat <command>`
- Report issues: [GitHub Issues](https://github.com/yourusername/ccstat/issues)