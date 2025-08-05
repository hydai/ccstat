//! ccstat - Analyze Claude Code usage data from local JSONL files

use ccstat::{
    cli::{Cli, Command, McpTransport},
    commands::{
        execute_blocks, execute_daily, execute_default, execute_monthly, execute_session,
        BlocksConfig, DailyConfig, MonthlyConfig, SessionConfig,
    },
    error::Result,
    mcp::McpServer,
};
use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ccstat=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle commands
    match cli.command {
        Some(Command::Daily {
            mode,
            json,
            since,
            until,
            instances,
            project,
            watch,
            interval,
            parallel,
            intern,
            arena,
            verbose,
        }) => {
            let config = DailyConfig {
                mode,
                json,
                since,
                until,
                instances,
                project,
                watch,
                interval,
                parallel,
                intern,
                arena,
                verbose,
            };
            execute_daily(config).await?;
        }

        Some(Command::Monthly {
            mode,
            json,
            since,
            until,
        }) => {
            let config = MonthlyConfig {
                mode,
                json,
                since,
                until,
            };
            execute_monthly(config).await?;
        }

        Some(Command::Session {
            mode,
            json,
            since,
            until,
        }) => {
            let config = SessionConfig {
                mode,
                json,
                since,
                until,
            };
            execute_session(config).await?;
        }

        Some(Command::Blocks {
            mode,
            json,
            active,
            recent,
            token_limit,
        }) => {
            let config = BlocksConfig {
                mode,
                json,
                active,
                recent,
                token_limit,
            };
            execute_blocks(config).await?;
        }

        Some(Command::Mcp { transport, port }) => {
            info!("Starting MCP server");

            let server = McpServer::new().await?;

            match transport {
                McpTransport::Stdio => {
                    server.run_stdio().await?;
                }
                McpTransport::Http => {
                    server.run_http(port).await?;
                }
            }
        }

        None => {
            execute_default().await?;
        }
    }

    Ok(())
}
