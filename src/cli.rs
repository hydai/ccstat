//! CLI interface for ccusage

use crate::error::{CcusageError, Result};
use crate::types::CostMode;
use clap::{Parser, Subcommand};

/// Analyze Claude Code usage data from local JSONL files
#[derive(Parser, Debug)]
#[command(name = "ccusage")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show daily usage summary
    Daily {
        /// Cost calculation mode
        #[arg(long, value_enum, default_value = "auto")]
        mode: CostMode,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Filter by start date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,

        /// Filter by end date (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,

        /// Show per-instance breakdown
        #[arg(long, short = 'i')]
        instances: bool,

        /// Filter by project name
        #[arg(long, short = 'p')]
        project: Option<String>,
    },

    /// Show monthly usage summary
    Monthly {
        /// Cost calculation mode
        #[arg(long, value_enum, default_value = "auto")]
        mode: CostMode,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Filter by start month (YYYY-MM)
        #[arg(long)]
        since: Option<String>,

        /// Filter by end month (YYYY-MM)
        #[arg(long)]
        until: Option<String>,
    },

    /// Show session-based usage
    Session {
        /// Cost calculation mode
        #[arg(long, value_enum, default_value = "auto")]
        mode: CostMode,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Filter by start date
        #[arg(long)]
        since: Option<String>,

        /// Filter by end date
        #[arg(long)]
        until: Option<String>,
    },

    /// Show 5-hour billing blocks
    Blocks {
        /// Cost calculation mode
        #[arg(long, value_enum, default_value = "auto")]
        mode: CostMode,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show only active blocks
        #[arg(long)]
        active: bool,

        /// Show only recent blocks (last 24h)
        #[arg(long)]
        recent: bool,

        /// Token limit for warnings
        #[arg(long)]
        token_limit: Option<String>,
    },

    /// Start MCP server
    Mcp {
        /// Transport type
        #[arg(long, value_enum, default_value = "stdio")]
        transport: McpTransport,

        /// Port for HTTP transport
        #[arg(long, default_value = "8080")]
        port: u16,
    },
}

/// MCP transport options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// Standard input/output
    Stdio,
    /// HTTP server
    Http,
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio => write!(f, "stdio"),
            Self::Http => write!(f, "http"),
        }
    }
}

impl std::str::FromStr for McpTransport {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdio" => Ok(Self::Stdio),
            "http" => Ok(Self::Http),
            _ => Err(format!("Invalid transport: {s}")),
        }
    }
}

impl Command {
    /// Get the cost mode for this command
    pub fn cost_mode(&self) -> CostMode {
        match self {
            Self::Daily { mode, .. } => *mode,
            Self::Monthly { mode, .. } => *mode,
            Self::Session { mode, .. } => *mode,
            Self::Blocks { mode, .. } => *mode,
            Self::Mcp { .. } => CostMode::Auto,
        }
    }

    /// Check if JSON output is requested
    pub fn is_json(&self) -> bool {
        match self {
            Self::Daily { json, .. } => *json,
            Self::Monthly { json, .. } => *json,
            Self::Session { json, .. } => *json,
            Self::Blocks { json, .. } => *json,
            Self::Mcp { .. } => false,
        }
    }
}

/// Parse date filter from string
pub fn parse_date_filter(date_str: &str) -> Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
        CcusageError::InvalidDate(format!("Invalid date format '{date_str}': {e}"))
    })
}

/// Parse month filter from string
pub fn parse_month_filter(month_str: &str) -> Result<(i32, u32)> {
    let parts: Vec<&str> = month_str.split('-').collect();
    if parts.len() != 2 {
        return Err(CcusageError::InvalidDate(format!(
            "Invalid month format '{month_str}', expected YYYY-MM"
        )));
    }

    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| CcusageError::InvalidDate(format!("Invalid year in '{month_str}'")))?;
    let month = parts[1]
        .parse::<u32>()
        .map_err(|_| CcusageError::InvalidDate(format!("Invalid month in '{month_str}'")))?;

    if !(1..=12).contains(&month) {
        return Err(CcusageError::InvalidDate(format!(
            "Month must be between 1-12, got {month}"
        )));
    }

    Ok((year, month))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from(["ccusage", "daily", "--json"]);
        match cli.command {
            Some(Command::Daily { json, .. }) => assert!(json),
            _ => panic!("Expected Daily command"),
        }
    }

    #[test]
    fn test_cost_mode_parsing() {
        let cli = Cli::parse_from(["ccusage", "daily", "--mode", "calculate"]);
        match cli.command {
            Some(Command::Daily { mode, .. }) => assert_eq!(mode, CostMode::Calculate),
            _ => panic!("Expected Daily command"),
        }
    }

    #[test]
    fn test_date_parsing() {
        assert!(parse_date_filter("2024-01-15").is_ok());
        assert!(parse_date_filter("invalid").is_err());
    }

    #[test]
    fn test_month_parsing() {
        assert_eq!(parse_month_filter("2024-01").unwrap(), (2024, 1));
        assert!(parse_month_filter("2024-13").is_err());
        assert!(parse_month_filter("invalid").is_err());
    }
}
