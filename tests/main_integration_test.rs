//! Integration tests for ccstat main CLI functionality

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a test JSONL file with sample data
fn create_test_jsonl(dir: &Path, filename: &str) -> std::io::Result<()> {
    let content = r#"{"session_id":"test-session-1","timestamp":"2024-01-01T10:00:00Z","model":"claude-3-opus","input_tokens":1000,"output_tokens":500,"cache_input_tokens":100,"cache_output_tokens":50,"total_cost":0.025,"project":"test-project","instance_id":"test-instance"}
{"session_id":"test-session-1","timestamp":"2024-01-01T10:30:00Z","model":"claude-3-opus","input_tokens":2000,"output_tokens":1000,"cache_input_tokens":200,"cache_output_tokens":100,"total_cost":0.050}
{"session_id":"test-session-2","timestamp":"2024-01-01T14:00:00Z","model":"claude-3-sonnet","input_tokens":3000,"output_tokens":1500,"total_cost":0.060,"project":"test-project"}
{"session_id":"test-session-3","timestamp":"2024-01-02T09:00:00Z","model":"claude-3-haiku","input_tokens":5000,"output_tokens":2500,"total_cost":0.008,"instance_id":"test-instance"}
{"session_id":"test-session-4","timestamp":"2024-01-02T15:00:00Z","model":"claude-3-opus","input_tokens":4000,"output_tokens":2000,"total_cost":0.100,"project":"another-project"}"#;
    
    let path = dir.join(filename);
    fs::write(path, content)
}

/// Helper to create a test directory structure with JSONL files
fn setup_test_data() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join("claude_data");
    fs::create_dir_all(&data_dir).unwrap();
    
    create_test_jsonl(&data_dir, "usage_2024_01.jsonl").unwrap();
    create_test_jsonl(&data_dir, "usage_2024_02.jsonl").unwrap();
    
    temp_dir
}

#[test]
fn test_daily_command_basic() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"))
        .stdout(predicate::str::contains("2024-01-01"))
        .stdout(predicate::str::contains("2024-01-02"))
        .stdout(predicate::str::contains("Total"));
}

#[test]
fn test_daily_command_with_json() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    let output = cmd
        .env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    
    let json_str = String::from_utf8(output).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    
    assert!(json["dates"].is_array());
    assert!(json["total_input_tokens"].is_number());
    assert!(json["total_cost"].is_number());
    assert!(json["daily_usage"].is_array());
}

#[test]
fn test_daily_command_with_date_filter() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--since")
        .arg("2024-01-01")
        .arg("--until")
        .arg("2024-01-01")
        .assert()
        .success()
        .stdout(predicate::str::contains("2024-01-01"))
        .stdout(predicate::str::contains("2024-01-02").not());
}

#[test]
fn test_daily_command_with_project_filter() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--project")
        .arg("test-project")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-project"));
}

#[test]
fn test_daily_command_with_instances() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--instances")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Instance Usage Report"))
        .stdout(predicate::str::contains("test-instance"));
}

#[test]
fn test_daily_command_verbose_mode() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-session-1"))
        .stdout(predicate::str::contains("10:00:00"));
}

#[test]
fn test_monthly_command() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("monthly")
        .assert()
        .success()
        .stdout(predicate::str::contains("Monthly Usage Report"))
        .stdout(predicate::str::contains("2024-01"));
}

#[test]
fn test_monthly_command_with_json() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    let output = cmd
        .env("CLAUDE_DATA_PATH", data_path)
        .arg("monthly")
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    
    let json_str = String::from_utf8(output).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    
    assert!(json["months"].is_array());
    assert!(json["monthly_usage"].is_array());
}

#[test]
fn test_session_command() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("session")
        .assert()
        .success()
        .stdout(predicate::str::contains("Session Usage Report"))
        .stdout(predicate::str::contains("test-session-1"))
        .stdout(predicate::str::contains("Duration"));
}

#[test]
fn test_session_command_with_date_filter() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("session")
        .arg("--since")
        .arg("2024-01-02")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-session-3"))
        .stdout(predicate::str::contains("test-session-1").not());
}

#[test]
fn test_blocks_command() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("blocks")
        .assert()
        .success()
        .stdout(predicate::str::contains("Session Blocks Report"));
}

#[test]
fn test_blocks_command_with_token_limit() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("blocks")
        .arg("--token-limit")
        .arg("10K")
        .assert()
        .success();
}

#[test]
fn test_mcp_command_stdio() {
    // Test that MCP server can start in stdio mode
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    
    // Send a simple JSON-RPC request and verify response format
    let request = r#"{"jsonrpc":"2.0","method":"server_info","id":1}"#;
    
    cmd.arg("mcp")
        .arg("--transport")
        .arg("stdio")
        .write_stdin(request)
        .assert()
        .success()
        .stdout(predicate::str::contains("jsonrpc"))
        .stdout(predicate::str::contains("server_name"));
}

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Analyze Claude Code usage"))
        .stdout(predicate::str::contains("Commands:"))
        .stdout(predicate::str::contains("daily"))
        .stdout(predicate::str::contains("monthly"))
        .stdout(predicate::str::contains("session"))
        .stdout(predicate::str::contains("blocks"))
        .stdout(predicate::str::contains("mcp"));
}

#[test]
fn test_version_command() {
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ccstat"));
}

#[test]
fn test_no_data_available() {
    let temp_dir = TempDir::new().unwrap();
    let empty_dir = temp_dir.path().join("empty_claude_data");
    fs::create_dir_all(&empty_dir).unwrap();
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", empty_dir)
        .arg("daily")
        .assert()
        .success()
        .stdout(predicate::str::contains("Total"))
        .stdout(predicate::str::contains("0"));
}

#[test]
fn test_cost_mode_calculate() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--mode")
        .arg("calculate")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"));
}

#[test]
fn test_cost_mode_display() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--mode")
        .arg("display")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"));
}

#[test]
fn test_invalid_date_format() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--since")
        .arg("invalid-date")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid date format"));
}

#[test]
fn test_invalid_month_format() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("monthly")
        .arg("--since")
        .arg("2024-13")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid month"));
}

#[test]
fn test_corrupted_jsonl_handling() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().join("claude_data");
    fs::create_dir_all(&data_dir).unwrap();
    
    // Create a file with corrupted JSON
    let corrupted_content = r#"{"session_id":"test-session-1","timestamp":"2024-01-01T10:00:00Z","model":"claude-3-opus","input_tokens":1000,"output_tokens":500}
{invalid json content}
{"session_id":"test-session-2","timestamp":"2024-01-01T14:00:00Z","model":"claude-3-sonnet","input_tokens":3000,"output_tokens":1500}"#;
    
    fs::write(data_dir.join("corrupted.jsonl"), corrupted_content).unwrap();
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_dir)
        .arg("daily")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"))
        .stdout(predicate::str::contains("2024-01-01"));
}

#[test]
fn test_parallel_processing() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--parallel")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"));
}

#[test]
fn test_memory_optimization_flags() {
    let temp_dir = setup_test_data();
    let data_path = temp_dir.path().join("claude_data");
    
    let mut cmd = Command::cargo_bin("ccstat").unwrap();
    cmd.env("CLAUDE_DATA_PATH", data_path)
        .arg("daily")
        .arg("--intern")
        .arg("--arena")
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily Usage Report"));
}