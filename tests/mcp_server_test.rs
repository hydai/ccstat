//! Comprehensive tests for the MCP server module

use ccstat::mcp::McpServer;
use serde_json::{json, Value};
use serial_test::serial;
use std::env;
use std::fs;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

/// Create a test environment with sample data
async fn setup_test_env() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    // Create claude directory structure
    let claude_dir = temp_dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Create usage directory
    let usage_dir = claude_dir.join("usage");
    fs::create_dir_all(&usage_dir).unwrap();

    unsafe {
        env::set_var("CLAUDE_DATA_PATH", &usage_dir);
        env::set_var("HOME", temp_dir.path());
        env::set_var("USERPROFILE", temp_dir.path());
    }

    // Create test JSONL data
    let jsonl_path = usage_dir.join("test_data.jsonl");
    let mut file = tokio::fs::File::create(&jsonl_path).await.unwrap();

    let test_entries = vec![
        r#"{"sessionId":"session-1","timestamp":"2024-01-15T10:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}},"cwd":"/project/alpha","cost_usd":0.05}"#,
        r#"{"sessionId":"session-1","timestamp":"2024-01-15T10:30:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":2000,"output_tokens":1000}},"cost_usd":0.10}"#,
        r#"{"sessionId":"session-2","timestamp":"2024-01-16T14:00:00Z","type":"assistant","message":{"model":"claude-3-sonnet","usage":{"input_tokens":500,"output_tokens":250}},"cwd":"/project/beta","cost_usd":0.02}"#,
        r#"{"sessionId":"session-3","timestamp":"2024-02-01T09:00:00Z","type":"assistant","message":{"model":"claude-3-haiku","usage":{"input_tokens":300,"output_tokens":150}},"cost_usd":0.01}"#,
        r#"{"sessionId":"session-4","timestamp":"2024-02-15T16:00:00Z","type":"assistant","message":{"model":"claude-3-opus","usage":{"input_tokens":5000,"output_tokens":2500}},"cost_usd":0.25}"#,
    ];

    for entry in test_entries {
        file.write_all(entry.as_bytes()).await.unwrap();
        file.write_all(b"\n").await.unwrap();
    }

    temp_dir
}

fn cleanup_test_env() {
    unsafe {
        env::remove_var("CLAUDE_DATA_PATH");
    }
}

#[tokio::test]
#[serial]
async fn test_mcp_server_creation() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await;
    assert!(server.is_ok());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_daily_method() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    // Test basic daily request
    let request = r#"{"jsonrpc":"2.0","method":"daily","params":{"mode":"Auto"},"id":1}"#;
    let response = handler.handle_request(request).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], 1);
    assert!(parsed["result"].is_object());

    let result = &parsed["result"];
    assert!(result["daily"].is_array());
    assert!(result["totals"].is_object());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_daily_with_filters() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "daily",
        "params": {
            "mode": "Calculate",
            "since": "2024-01-01",
            "until": "2024-01-31",
            "project": "alpha"
        },
        "id": 2
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 2);

    let daily_entries = parsed["result"]["daily"].as_array().unwrap();
    // Should only have January data
    for entry in daily_entries {
        let date = entry["date"].as_str().unwrap();
        assert!(date.starts_with("2024-01"));
    }

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_monthly_method() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "monthly",
        "params": {
            "mode": "Auto"
        },
        "id": 3
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 3);
    assert!(parsed["result"].is_object());
    assert!(parsed["result"]["monthly"].is_array());
    assert!(parsed["result"]["totals"].is_object());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_session_method() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "session",
        "params": {
            "mode": "Display",
            "since": "2024-01-01",
            "until": "2024-12-31"
        },
        "id": 4
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 4);
    assert!(parsed["result"].is_object());
    assert!(parsed["result"]["sessions"].is_array());

    let sessions = parsed["result"]["sessions"].as_array().unwrap();
    assert!(!sessions.is_empty());

    // Check session structure
    for session in sessions {
        assert!(session["session_id"].is_string());
        assert!(session["start_time"].is_string());
        assert!(session["end_time"].is_string());
        assert!(session["tokens"].is_object());
        assert!(session["total_cost"].is_number());
        // Sessions don't have a duration field in the MCP server response
        // (unlike the JSON formatter which adds duration_seconds)
    }

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_server_info_method() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "server_info",
        "id": 5
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 5);
    assert!(parsed["result"].is_object());
    assert!(parsed["result"]["name"].is_string());
    assert!(parsed["result"]["version"].is_string());
    assert!(parsed["result"]["methods"].is_array());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_invalid_method() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "invalid_method",
        "id": 6
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 6);
    assert!(parsed["error"].is_object());
    assert_eq!(parsed["error"]["code"], -32601); // Method not found

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_malformed_request() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let malformed = r#"{"invalid": "json", missing_fields}"#;
    let response = handler.handle_request(malformed).await;

    // The handler should return an error response for malformed JSON
    assert!(response.is_some());
    let parsed: Value = serde_json::from_str(&response.unwrap()).unwrap();
    assert!(parsed["error"].is_object());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_missing_params() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    // Call daily without any parameters (should use defaults)
    let request = json!({
        "jsonrpc": "2.0",
        "method": "daily",
        "params": {},
        "id": 7
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 7);
    assert!(parsed["result"].is_object());

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_invalid_date_format() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "daily",
        "params": {
            "mode": "Auto",
            "since": "invalid-date",
            "until": "2024-01-31"
        },
        "id": 8
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 8);
    assert!(parsed["error"].is_object());
    assert_eq!(parsed["error"]["code"], -32602); // Invalid params

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_monthly_with_filters() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "monthly",
        "params": {
            "mode": "Calculate",
            "since": "2024-01",
            "until": "2024-02"
        },
        "id": 9
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 9);
    assert!(parsed["result"].is_object());

    let monthly_entries = parsed["result"]["monthly"].as_array().unwrap();
    // Check that we have the expected months
    let months: Vec<_> = monthly_entries.iter()
        .map(|e| e["month"].as_str().unwrap())
        .collect();

    assert!(months.contains(&"2024-01") || months.contains(&"2024-02"));

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_all_cost_modes() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    for (i, mode) in ["Auto", "Calculate", "Display"].iter().enumerate() {
        let request = json!({
            "jsonrpc": "2.0",
            "method": "daily",
            "params": {
                "mode": mode
            },
            "id": 10 + i
        });

        let response = handler.handle_request(&request.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();

        assert_eq!(parsed["id"], 10 + i);
        assert!(parsed["result"].is_object(), "Mode {} failed", mode);
    }

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_empty_data_handling() {
    let temp_dir = TempDir::new().unwrap();

    // Create claude directory structure
    let claude_dir = temp_dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Create empty usage directory
    let empty_data_path = claude_dir.join("usage");
    fs::create_dir_all(&empty_data_path).unwrap();

    unsafe {
        env::set_var("CLAUDE_DATA_PATH", &empty_data_path);
        env::set_var("HOME", temp_dir.path());
        env::set_var("USERPROFILE", temp_dir.path());
    }

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    let request = json!({
        "jsonrpc": "2.0",
        "method": "daily",
        "params": {"mode": "Auto"},
        "id": 20
    });

    let response = handler.handle_request(&request.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&response).unwrap();

    assert_eq!(parsed["id"], 20);
    assert!(parsed["result"].is_object());

    // Should return empty data gracefully
    assert_eq!(parsed["result"]["daily"].as_array().unwrap().len(), 0);

    // Check the totals structure - it should have a tokens object
    assert!(parsed["result"]["totals"].is_object());
    assert!(parsed["result"]["totals"]["tokens"].is_object());

    // Check individual token fields - TokenCounts serializes these fields
    let tokens = &parsed["result"]["totals"]["tokens"];
    assert_eq!(tokens["input_tokens"], 0);
    assert_eq!(tokens["output_tokens"], 0);
    assert_eq!(tokens["cache_creation_tokens"], 0);
    assert_eq!(tokens["cache_read_tokens"], 0);

    // Check total cost
    assert_eq!(parsed["result"]["totals"]["total_cost"], 0.0);

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_batch_request() {
    let _temp_dir = setup_test_env().await;

    let server = McpServer::new().await.unwrap();
    let handler = server.create_handler();

    // JSON-RPC batch request
    let batch = json!([
        {
            "jsonrpc": "2.0",
            "method": "server_info",
            "id": 30
        },
        {
            "jsonrpc": "2.0",
            "method": "daily",
            "params": {"mode": "Auto"},
            "id": 31
        }
    ]);

    let response = handler.handle_request(&batch.to_string()).await;

    // jsonrpc-core may not support batch requests, so we check if we get a response
    if let Some(resp) = response {
        let parsed: Result<Vec<Value>, _> = serde_json::from_str(&resp);
        if let Ok(responses) = parsed {
            // If batch is supported, we should get 2 responses
            assert_eq!(responses.len(), 2);
        } else {
            // If batch is not supported, we might get an error
            let single: Value = serde_json::from_str(&resp).unwrap();
            assert!(single["error"].is_object());
        }
    }

    cleanup_test_env();
}

#[tokio::test]
#[serial]
async fn test_concurrent_requests() {
    use std::sync::Arc;
    use tokio::task;

    let _temp_dir = setup_test_env().await;

    let server = Arc::new(McpServer::new().await.unwrap());
    let handler = Arc::new(server.create_handler());

    // Create multiple concurrent requests
    let mut handles = vec![];

    for i in 0..5 {
        let handler_clone = Arc::clone(&handler);
        let handle = task::spawn(async move {
            let request = json!({
                "jsonrpc": "2.0",
                "method": "daily",
                "params": {"mode": "Auto"},
                "id": 100 + i
            });

            handler_clone.handle_request(&request.to_string()).await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        let response = handle.await.unwrap();
        assert!(response.is_some());

        let parsed: Value = serde_json::from_str(&response.unwrap()).unwrap();
        assert!(parsed["result"].is_object());
    }

    cleanup_test_env();
}
