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
    /// Show informational output (default is quiet mode with only warnings and errors)
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    /// Cost calculation mode
    #[arg(long, value_enum, default_value = "auto", global = true)]
    pub mode: CostMode,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Filter by start date (YYYY-MM-DD or YYYY-MM)
    #[arg(long, global = true)]
    pub since: Option<String>,

    /// Filter by end date (YYYY-MM-DD or YYYY-MM)
    #[arg(long, global = true)]
    pub until: Option<String>,

    /// Filter by project name
    #[arg(long, short = 'p', global = true)]
    pub project: Option<String>,

    /// Timezone for date grouping (e.g. "America/New_York", "Asia/Tokyo", "UTC")
    /// If not specified, uses the system's local timezone
    #[arg(long, short = 'z', global = true)]
    pub timezone: Option<String>,

    /// Use UTC for date grouping (overrides --timezone)
    #[arg(long, global = true)]
    pub utc: bool,

    /// Show full model names instead of shortened versions
    #[arg(long, global = true)]
    pub full_model_names: bool,

    /// Enable string interning for memory optimization
    #[arg(long, global = true)]
    pub intern: bool,

    /// Enable arena allocation for parsing
    #[arg(long, global = true)]
    pub arena: bool,

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
        /// Show per-instance breakdown
        #[arg(long, short = 'i')]
        instances: bool,

        /// Enable live monitoring mode
        #[arg(long, short = 'w')]
        watch: bool,

        /// Refresh interval in seconds (default: 5)
        #[arg(long, default_value = "5")]
        interval: u64,

        /// Show detailed token information per entry
        #[arg(long, short = 'd')]
        detailed: bool,
    },

    /// Show monthly usage summary
    Monthly,

    /// Show session-based usage
    Session,

    /// Show 5-hour billing blocks
    Blocks {
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

    /// Generate statusline output for Claude Code
    Statusline {
        /// Monthly subscription fee in USD (default: 200)
        #[arg(long, default_value = "200")]
        monthly_fee: f64,

        /// Disable colored output
        #[arg(long)]
        no_color: bool,

        /// Show date and time
        #[arg(long)]
        show_date: bool,

        /// Show git branch
        #[arg(long)]
        show_git: bool,
    },
}

/// Parse date filter from string
///
/// Accepts dates in YYYY-MM-DD or YYYY-MM format.
/// For YYYY-MM format, defaults to the first day of the month.
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
/// assert_eq!(date.day(), 15);
///
/// let date = parse_date_filter("2024-01").unwrap();
/// assert_eq!(date.year(), 2024);
/// assert_eq!(date.month(), 1);
/// assert_eq!(date.day(), 1);
/// ```
pub fn parse_date_filter(date_str: &str) -> Result<chrono::NaiveDate> {
    // Try YYYY-MM-DD format first
    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Ok(date);
    }

    // Try YYYY-MM format (convert to first day of month)
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() == 2 {
        let year = parts[0]
            .parse::<i32>()
            .map_err(|_| CcstatError::InvalidDate(format!("Invalid year in '{date_str}'")))?;
        let month = parts[1]
            .parse::<u32>()
            .map_err(|_| CcstatError::InvalidDate(format!("Invalid month in '{date_str}'")))?;

        if !(1..=12).contains(&month) {
            return Err(CcstatError::InvalidDate(format!(
                "Month must be between 1-12, got {month}"
            )));
        }

        chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| CcstatError::InvalidDate(format!("Invalid date: {date_str}")))
    } else {
        Err(CcstatError::InvalidDate(format!(
            "Invalid date format '{}', expected YYYY-MM-DD or YYYY-MM",
            date_str
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_cli_parsing() {
        // Test global JSON flag
        let cli = Cli::parse_from(["ccstat", "--json"]);
        assert!(cli.json);

        // Test with daily command
        let cli = Cli::parse_from(["ccstat", "daily", "--instances"]);
        match cli.command {
            Some(Command::Daily { instances, .. }) => assert!(instances),
            _ => panic!("Expected Daily command"),
        }
    }

    #[test]
    fn test_cost_mode_parsing() {
        // Test global mode flag
        let cli = Cli::parse_from(["ccstat", "--mode", "calculate"]);
        assert_eq!(cli.mode, CostMode::Calculate);
    }

    #[test]
    fn test_date_parsing() {
        // Test YYYY-MM-DD format
        let date = parse_date_filter("2024-01-15").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 15);

        // Test YYYY-MM format (should default to first day)
        let date = parse_date_filter("2024-01").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 1);

        // Test invalid formats
        assert!(parse_date_filter("invalid").is_err());
        assert!(parse_date_filter("2024-13").is_err());
        assert!(parse_date_filter("2024").is_err());
    }
}
