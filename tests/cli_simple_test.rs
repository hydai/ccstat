//! Simple integration tests for ccstat CLI commands

use ccstat::cli::{Cli, Command, McpTransport};
use ccstat::types::CostMode;
use clap::Parser;

#[test]
fn test_cli_daily_command_basic() {
    let args = vec!["ccstat", "daily"];
    let cli = Cli::parse_from(args);
    
    match cli.command.unwrap() {
        Command::Daily { mode, json, .. } => {
            assert_eq!(mode, CostMode::Auto);
            assert!(!json);
        }
        _ => panic!("Expected Daily command"),
    }
}

#[test]
fn test_cli_daily_with_options() {
    let args = vec![
        "ccstat",
        "daily",
        "--mode", "calculate",
        "--json",
        "--since", "2024-01-01",
        "--until", "2024-01-31",
        "--project", "test-project",
    ];
    
    let cli = Cli::parse_from(args);
    match cli.command.unwrap() {
        Command::Daily {
            mode,
            json,
            since,
            until,
            project,
            ..
        } => {
            assert_eq!(mode, CostMode::Calculate);
            assert!(json);
            assert_eq!(since, Some("2024-01-01".to_string()));
            assert_eq!(until, Some("2024-01-31".to_string()));
            assert_eq!(project, Some("test-project".to_string()));
        }
        _ => panic!("Expected Daily command"),
    }
}

#[test]
fn test_cli_monthly_command() {
    let args = vec![
        "ccstat",
        "monthly",
        "--mode", "display",
        "--json",
    ];
    
    let cli = Cli::parse_from(args);
    match cli.command.unwrap() {
        Command::Monthly { mode, json, .. } => {
            assert_eq!(mode, CostMode::Display);
            assert!(json);
        }
        _ => panic!("Expected Monthly command"),
    }
}

#[test]
fn test_cli_session_command() {
    let args = vec![
        "ccstat",
        "session",
        "--since", "2024-01-01",
        "--until", "2024-01-31",
    ];
    
    let cli = Cli::parse_from(args);
    match cli.command.unwrap() {
        Command::Session { since, until, .. } => {
            assert_eq!(since, Some("2024-01-01".to_string()));
            assert_eq!(until, Some("2024-01-31".to_string()));
        }
        _ => panic!("Expected Session command"),
    }
}

#[test]
fn test_cli_blocks_command() {
    let args = vec![
        "ccstat",
        "blocks",
        "--active",
        "--recent",
    ];
    
    let cli = Cli::parse_from(args);
    match cli.command.unwrap() {
        Command::Blocks { active, recent, .. } => {
            assert!(active);
            assert!(recent);
        }
        _ => panic!("Expected Blocks command"),
    }
}

#[test]
fn test_cli_mcp_command() {
    let args = vec![
        "ccstat",
        "mcp",
        "--transport", "http",
        "--port", "8080",
    ];
    
    let cli = Cli::parse_from(args);
    match cli.command.unwrap() {
        Command::Mcp { transport, port } => {
            assert_eq!(transport, McpTransport::Http);
            assert_eq!(port, 8080);
        }
        _ => panic!("Expected Mcp command"),
    }
}

#[test]
fn test_cost_mode_parsing() {
    let modes = ["auto", "calculate", "display"];
    
    for mode_str in &modes {
        let args = vec![
            "ccstat",
            "daily",
            "--mode", mode_str,
        ];
        
        let cli = Cli::parse_from(args);
        match cli.command.unwrap() {
            Command::Daily { mode, .. } => {
                match *mode_str {
                    "auto" => assert_eq!(mode, CostMode::Auto),
                    "calculate" => assert_eq!(mode, CostMode::Calculate),
                    "display" => assert_eq!(mode, CostMode::Display),
                    _ => unreachable!(),
                }
            }
            _ => panic!("Expected Daily command"),
        }
    }
}

#[test]
fn test_mcp_transport_parsing() {
    let transports = [("stdio", McpTransport::Stdio), ("http", McpTransport::Http)];
    
    for (transport_str, expected) in &transports {
        let args = vec![
            "ccstat",
            "mcp",
            "--transport", transport_str,
        ];
        
        let cli = Cli::parse_from(args);
        match cli.command.unwrap() {
            Command::Mcp { transport, .. } => {
                assert_eq!(transport, *expected);
            }
            _ => panic!("Expected Mcp command"),
        }
    }
}

#[test]
fn test_cli_help() {
    let result = Cli::try_parse_from(vec!["ccstat", "--help"]);
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    let help_text = err.to_string();
    
    assert!(help_text.contains("ccstat"));
    assert!(help_text.contains("Commands:"));
}

#[test]
fn test_cli_version() {
    let result = Cli::try_parse_from(vec!["ccstat", "--version"]);
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    let version_text = err.to_string();
    
    assert!(version_text.contains("ccstat"));
}