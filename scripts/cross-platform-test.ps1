# Cross-platform testing script for ccstat (Windows PowerShell version)
# Tests all major functionality to ensure consistency across platforms

$ErrorActionPreference = "Continue"

# Test counters
$script:TestsPassed = 0
$script:TestsFailed = 0

# Function to print test results
function Write-TestResult {
    param(
        [string]$TestName,
        [string]$Result
    )

    if ($Result -eq "PASS") {
        Write-Host "✓ $TestName" -ForegroundColor Green
        $script:TestsPassed++
    } else {
        Write-Host "✗ $TestName" -ForegroundColor Red
        $script:TestsFailed++
    }
}

# Function to run a test
function Test-Command {
    param(
        [string]$TestName,
        [string]$Command,
        [string]$ExpectedPattern = ""
    )

    Write-Host -NoNewline "Testing: $TestName... "

    try {
        $output = Invoke-Expression $Command 2>&1 | Out-String

        if ($LASTEXITCODE -eq 0) {
            if ($ExpectedPattern -eq "" -or $output -match $ExpectedPattern) {
                Write-TestResult $TestName "PASS"
            } else {
                Write-TestResult $TestName "FAIL"
                Write-Host "  Expected pattern: $ExpectedPattern"
                Write-Host "  Got: $($output.Substring(0, [Math]::Min(200, $output.Length)))"
            }
        } else {
            Write-TestResult $TestName "FAIL"
            Write-Host "  Command failed with exit code: $LASTEXITCODE"
            Write-Host "  Error: $($output.Substring(0, [Math]::Min(200, $output.Length)))"
        }
    } catch {
        Write-TestResult $TestName "FAIL"
        Write-Host "  Exception: $_"
    }
}

# Check if ccstat is available
try {
    $null = Get-Command ccstat -ErrorAction Stop
} catch {
    Write-Host "Error: ccstat not found in PATH" -ForegroundColor Red
    Write-Host "Please build and install ccstat first:"
    Write-Host "  cargo build --release"
    Write-Host "  cargo install --path ."
    exit 1
}

Write-Host "=== ccstat Cross-Platform Test Suite ===" -ForegroundColor Cyan
Write-Host "Platform: Windows"
Write-Host "Architecture: $env:PROCESSOR_ARCHITECTURE"
Write-Host "ccstat version: $(ccstat --version)"
Write-Host ""

# Basic functionality tests
Write-Host "=== Basic Command Tests ===" -ForegroundColor Yellow
Test-Command "Help command" "ccstat --help" "Analyze Claude Code usage"
Test-Command "Version command" "ccstat --version" "ccstat"
Test-Command "Daily command (no args)" "ccstat daily"
Test-Command "Monthly command (no args)" "ccstat monthly"
Test-Command "Session command (no args)" "ccstat session"
Test-Command "Blocks command (no args)" "ccstat blocks"

# Test with date filters
Write-Host "`n=== Date Filter Tests ===" -ForegroundColor Yellow
Test-Command "Daily with since date" "ccstat daily --since 2024-01-01"
Test-Command "Daily with date range" "ccstat daily --since 2024-01-01 --until 2024-01-31"
Test-Command "Monthly with month filter" "ccstat monthly --since 2024-01 --until 2024-12"

# Test output formats
Write-Host "`n=== Output Format Tests ===" -ForegroundColor Yellow
Test-Command "JSON output for daily" "ccstat daily --json" '"daily"'
Test-Command "JSON output for monthly" "ccstat monthly --json" '"monthly"'
Test-Command "JSON output for sessions" "ccstat session --json" '"sessions"'
Test-Command "JSON output for blocks" "ccstat blocks --json" '"blocks"'

# Test cost modes
Write-Host "`n=== Cost Mode Tests ===" -ForegroundColor Yellow
Test-Command "Auto cost mode" "ccstat daily --mode auto"
Test-Command "Calculate cost mode" "ccstat daily --mode calculate"
Test-Command "Display cost mode" "ccstat daily --mode display"

# Test performance flags
Write-Host "`n=== Performance Flag Tests ===" -ForegroundColor Yellow
Test-Command "Parallel processing" "ccstat daily --parallel"
Test-Command "String interning" "ccstat daily --intern"
Test-Command "Arena allocation" "ccstat daily --arena"
Test-Command "All performance flags" "ccstat daily --parallel --intern --arena"

# Test verbose mode
Write-Host "`n=== Verbose Mode Tests ===" -ForegroundColor Yellow
Test-Command "Verbose daily output" "ccstat daily --verbose --since 2024-01-01 --until 2024-01-01"

# Test project filtering
Write-Host "`n=== Project Filter Tests ===" -ForegroundColor Yellow
Test-Command "Project filter" "ccstat daily --project test-project"

# Test instance grouping
Write-Host "`n=== Instance Grouping Tests ===" -ForegroundColor Yellow
Test-Command "Daily by instance" "ccstat daily --by-instance"

# Test session options
Write-Host "`n=== Session Command Tests ===" -ForegroundColor Yellow
Test-Command "Sessions with models" "ccstat session --models"

# Test billing blocks options
Write-Host "`n=== Billing Blocks Tests ===" -ForegroundColor Yellow
Test-Command "Active blocks only" "ccstat blocks --active"
Test-Command "Recent blocks" "ccstat blocks --recent"
Test-Command "Blocks with limit" "ccstat blocks --limit 1000000"

# Platform-specific tests
Write-Host "`n=== Platform-Specific Tests ===" -ForegroundColor Yellow
Write-Host "Testing Windows-specific paths..."
$env:RUST_LOG = "ccstat=debug"
Test-Command "Windows data discovery" "ccstat daily 2>&1" "AppData"
$env:RUST_LOG = ""

# Test environment variable override
Write-Host "`n=== Environment Variable Tests ===" -ForegroundColor Yellow
$env:CLAUDE_DATA_PATH = "C:\temp\test"
Test-Command "CLAUDE_DATA_PATH override" "ccstat daily 2>&1"
$env:CLAUDE_DATA_PATH = ""

# Summary
Write-Host "`n=== Test Summary ===" -ForegroundColor Cyan
Write-Host "Tests passed: $script:TestsPassed" -ForegroundColor Green
Write-Host "Tests failed: $script:TestsFailed" -ForegroundColor Red

if ($script:TestsFailed -eq 0) {
    Write-Host "`nAll tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`nSome tests failed!" -ForegroundColor Red
    exit 1
}
