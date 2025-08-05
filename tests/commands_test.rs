//! Integration tests for the commands module

use ccstat::{
    commands::{
        execute_blocks, execute_daily, execute_default, execute_monthly, execute_session,
        BlocksConfig, DailyConfig, MonthlyConfig, SessionConfig,
    },
    types::CostMode,
};
use serial_test::serial;
use std::env;
use std::fs;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

/// Create a test environment with temporary data directory
async fn setup_test_env() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let data_path = temp_dir.path().join("claude_data");
    fs::create_dir_all(&data_path).unwrap();
    
    // Also clear HOME to prevent loading user's real data
    unsafe {
        env::set_var("CLAUDE_DATA_PATH", &data_path);
        env::set_var("HOME", temp_dir.path());
        env::set_var("USERPROFILE", temp_dir.path()); // Windows
    }
    
    // Create a sample JSONL file
    let jsonl_path = data_path.join("usage.jsonl");
    let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
    
    // Write test data
    let test_data = vec![
        r#"{"sessionId":"test-session-1","timestamp":"2024-01-01T10:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}},"cwd":"/home/user/project","cost_usd":0.05}"#,
        r#"{"sessionId":"test-session-1","timestamp":"2024-01-01T10:30:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":2000,"output_tokens":1000}},"cost_usd":0.10}"#,
        r#"{"sessionId":"test-session-2","timestamp":"2024-01-02T14:00:00Z","type":"assistant","message":{"model":"claude-3-sonnet","usage":{"input_tokens":500,"output_tokens":250}},"cost_usd":0.02}"#,
        r#"{"sessionId":"test-session-3","timestamp":"2024-02-15T09:00:00Z","type":"assistant","message":{"model":"claude-3-haiku","usage":{"input_tokens":300,"output_tokens":150}},"cost_usd":0.01}"#,
    ];
    
    for line in test_data {
        file.write_all(line.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
    }
    
    temp_dir
}

/// Clean up test environment
fn cleanup_test_env() {
    unsafe {
        env::remove_var("CLAUDE_DATA_PATH");
        // Don't remove HOME/USERPROFILE as these are needed by the system
    }
}

#[tokio::test]
#[serial]
async fn test_execute_default_with_data() {
    let _temp_dir = setup_test_env().await;
    
    // Capture stdout
    let result = execute_default().await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_daily_basic() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_daily_with_filters() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Calculate,
        json: true,
        since: Some("2024-01-01".to_string()),
        until: Some("2024-01-31".to_string()),
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_daily_with_instances() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: true,
        since: None,
        until: None,
        instances: true,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_daily_parallel() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: true,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_daily_verbose() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: true,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_monthly_basic() {
    let _temp_dir = setup_test_env().await;
    
    let config = MonthlyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
    };
    
    let result = execute_monthly(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_monthly_with_filters() {
    let _temp_dir = setup_test_env().await;
    
    let config = MonthlyConfig {
        mode: CostMode::Calculate,
        json: true,
        since: Some("2024-01".to_string()),
        until: Some("2024-02".to_string()),
    };
    
    let result = execute_monthly(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_session_basic() {
    let _temp_dir = setup_test_env().await;
    
    let config = SessionConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
    };
    
    let result = execute_session(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_session_with_filters() {
    let _temp_dir = setup_test_env().await;
    
    let config = SessionConfig {
        mode: CostMode::Display,
        json: true,
        since: Some("2024-01-01".to_string()),
        until: Some("2024-01-31".to_string()),
    };
    
    let result = execute_session(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_blocks_basic() {
    let _temp_dir = setup_test_env().await;
    
    let config = BlocksConfig {
        mode: CostMode::Auto,
        json: false,
        active: false,
        recent: false,
        token_limit: None,
    };
    
    let result = execute_blocks(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_blocks_with_filters() {
    let _temp_dir = setup_test_env().await;
    
    let config = BlocksConfig {
        mode: CostMode::Calculate,
        json: true,
        active: true,
        recent: true,
        token_limit: Some("80%".to_string()),
    };
    
    let result = execute_blocks(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_execute_blocks_with_absolute_token_limit() {
    let _temp_dir = setup_test_env().await;
    
    let config = BlocksConfig {
        mode: CostMode::Auto,
        json: false,
        active: false,
        recent: false,
        token_limit: Some("1000000".to_string()),
    };
    
    let result = execute_blocks(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_daily_with_memory_optimizations() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: true,
        intern: true,
        arena: true,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_daily_with_project_filter() {
    let _temp_dir = setup_test_env().await;
    
    // Create data with project information
    let data_path = temp_dir_path();
    let jsonl_path = data_path.join("claude_data/project_usage.jsonl");
    let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();
    
    let project_data = r#"{"sessionId":"proj-session","timestamp":"2024-01-01T10:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":1000,"output_tokens":500}},"cwd":"/home/user/test-project","cost_usd":0.05}"#;
    file.write_all(project_data.as_bytes()).await.unwrap();
    file.write_all(b"\n").await.unwrap();
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: None,
        until: None,
        instances: false,
        project: Some("test-project".to_string()),
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

fn temp_dir_path() -> std::path::PathBuf {
    std::env::var("CLAUDE_DATA_PATH")
        .unwrap()
        .parse::<std::path::PathBuf>()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[tokio::test]
#[serial]
async fn test_invalid_date_filter() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: false,
        since: Some("invalid-date".to_string()),
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_err());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_invalid_month_filter() {
    let _temp_dir = setup_test_env().await;
    
    let config = MonthlyConfig {
        mode: CostMode::Auto,
        json: false,
        since: Some("2024-13".to_string()), // Invalid month
        until: None,
    };
    
    let result = execute_monthly(config).await;
    assert!(result.is_err());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_blocks_invalid_token_limit() {
    let _temp_dir = setup_test_env().await;
    
    let config = BlocksConfig {
        mode: CostMode::Auto,
        json: false,
        active: false,
        recent: false,
        token_limit: Some("invalid".to_string()),
    };
    
    let result = execute_blocks(config).await;
    assert!(result.is_err());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_daily_instances_parallel() {
    let _temp_dir = setup_test_env().await;
    
    let config = DailyConfig {
        mode: CostMode::Auto,
        json: true,
        since: None,
        until: None,
        instances: true,
        project: None,
        watch: false,
        interval: 5,
        parallel: true,
        intern: false,
        arena: false,
        verbose: false,
    };
    
    let result = execute_daily(config).await;
    assert!(result.is_ok());
    
    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_all_cost_modes() {
    let _temp_dir = setup_test_env().await;
    
    // Test Auto mode
    let config_auto = DailyConfig {
        mode: CostMode::Auto,
        json: true,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    assert!(execute_daily(config_auto).await.is_ok());
    
    // Test Calculate mode
    let config_calc = DailyConfig {
        mode: CostMode::Calculate,
        json: true,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    assert!(execute_daily(config_calc).await.is_ok());
    
    // Test Display mode
    let config_display = DailyConfig {
        mode: CostMode::Display,
        json: true,
        since: None,
        until: None,
        instances: false,
        project: None,
        watch: false,
        interval: 5,
        parallel: false,
        intern: false,
        arena: false,
        verbose: false,
    };
    assert!(execute_daily(config_display).await.is_ok());
    
    cleanup_test_env();
}