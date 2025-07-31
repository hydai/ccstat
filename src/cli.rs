//! CLI interface for ccstat
//!
//! This module defines the command-line interface using clap, providing
//! structured access to all ccstat functionality.
//!
//! # Commands
//!
//! - `daily` - Show daily usage summary with optional date filters
//! - `monthly` - Show monthly rollups with month filters
//! - `session` - Show individual session details
//! - `blocks` - Show 5-hour billing blocks
//! - `mcp` - Start an MCP server for API access
//!
//! # Example
//!
//! ```bash
//! # Show daily usage for January 2024
//! ccstat daily --since 2024-01-01 --until 2024-01-31
//!
//! # Show monthly usage as JSON
//! ccstat monthly --json
//!
//! # Show active billing blocks with token warnings
//! ccstat blocks --active --token-limit 80%
//! ```

use crate::error::{CcstatError, Result};
use crate::types::CostMode;
use clap::{Parser, Subcommand};

/// Analyze Claude Code usage data from local JSONL files
#[derive(Parser, Debug)]
#[command(name = "ccstat")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available commands
///
/// Each command provides different views and aggregations of usage data,
/// with flexible filtering and output options.
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

        /// Enable live monitoring mode
        #[arg(long, short = 'w')]
        watch: bool,

        /// Refresh interval in seconds (default: 5)
        #[arg(long, default_value = "5")]
        interval: u64,

        /// Use parallel file processing
        #[arg(long)]
        parallel: bool,

        /// Enable string interning for memory optimization
        #[arg(long)]
        intern: bool,

        /// Enable arena allocation for parsing
        #[arg(long)]
        arena: bool,

        /// Show detailed token information per entry
        #[arg(long, short = 'v')]
        verbose: bool,
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
///
/// Defines how the MCP server communicates with clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// Standard input/output - for direct process communication
    Stdio,
    /// HTTP server - for network-based access
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
///
/// Expects dates in YYYY-MM-DD format.
///
/// # Arguments
///
/// * `date_str` - Date string to parse
///
/// # Returns
///
/// A parsed `NaiveDate` or an error if the format is invalid
///
/// # Example
///
/// ```
/// use ccstat::cli::parse_date_filter;
/// use chrono::Datelike;
///
/// let date = parse_date_filter("2024-01-15").unwrap();
/// assert_eq!(date.year(), 2024);
/// ```
pub fn parse_date_filter(date_str: &str) -> Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| CcstatError::InvalidDate(format!("Invalid date format '{date_str}': {e}")))
}

/// Parse month filter from string
///
/// Expects months in YYYY-MM format.
///
/// # Arguments
///
/// * `month_str` - Month string to parse
///
/// # Returns
///
/// A tuple of (year, month) or an error if the format is invalid
///
/// # Example
///
/// ```
/// use ccstat::cli::parse_month_filter;
///
/// let (year, month) = parse_month_filter("2024-01").unwrap();
/// assert_eq!(year, 2024);
/// assert_eq!(month, 1);
/// ```
pub fn parse_month_filter(month_str: &str) -> Result<(i32, u32)> {
    let parts: Vec<&str> = month_str.split('-').collect();
    if parts.len() != 2 {
        return Err(CcstatError::InvalidDate(format!(
            "Invalid month format '{month_str}', expected YYYY-MM"
        )));
    }

    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| CcstatError::InvalidDate(format!("Invalid year in '{month_str}'")))?;
    let month = parts[1]
        .parse::<u32>()
        .map_err(|_| CcstatError::InvalidDate(format!("Invalid month in '{month_str}'")))?;

    if !(1..=12).contains(&month) {
        return Err(CcstatError::InvalidDate(format!(
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
        let cli = Cli::parse_from(["ccstat", "daily", "--json"]);
        match cli.command {
            Some(Command::Daily { json, .. }) => assert!(json),
            _ => panic!("Expected Daily command"),
        }
    }

    #[test]
    fn test_cost_mode_parsing() {
        let cli = Cli::parse_from(["ccstat", "daily", "--mode", "calculate"]);
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
