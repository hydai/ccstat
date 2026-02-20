//! CLI interface for ccstat
//!
//! This module defines the command-line interface using clap, providing
//! a two-level subcommand structure: `ccstat [provider] <report> [flags]`.
//!
//! When the provider is omitted, it defaults to `claude`. This means:
//! - `ccstat daily` is equivalent to `ccstat claude daily`
//! - `ccstat codex daily` explicitly selects the Codex provider
//!
//! # Example
//!
//! ```bash
//! # Show daily Claude usage for January 2024
//! ccstat daily --since 2024-01-01 --until 2024-01-31
//!
//! # Explicit provider selection
//! ccstat claude monthly --json
//!
//! # Show active billing blocks with token warnings
//! ccstat blocks --active --token-limit 80%
//! ```

use crate::error::{CcstatError, Result};
use crate::types::CostMode;
use clap::{Args, Parser, Subcommand};

/// Analyze AI coding tool usage data
#[derive(Parser, Debug, Clone)]
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

    /// Enable live monitoring mode (auto-refresh on changes)
    #[arg(long, short = 'w', global = true)]
    pub watch: bool,

    /// Refresh interval in seconds for watch mode (default: 5)
    #[arg(long, default_value = "5", global = true)]
    pub interval: u64,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Command>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Supported usage data providers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Codex,
    Opencode,
    Amp,
    Pi,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Claude => write!(f, "claude"),
            Provider::Codex => write!(f, "codex"),
            Provider::Opencode => write!(f, "opencode"),
            Provider::Amp => write!(f, "amp"),
            Provider::Pi => write!(f, "pi"),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared argument structs (reused by both shortcut and provider subcommands)
// ---------------------------------------------------------------------------

/// Arguments for the daily report
#[derive(Args, Debug, Clone)]
pub struct DailyArgs {
    /// Show per-instance breakdown
    #[arg(long, short = 'i')]
    pub instances: bool,

    /// Show detailed token information per entry
    #[arg(long, short = 'd', conflicts_with = "instances")]
    pub detailed: bool,
}

/// Arguments for the weekly report
#[derive(Args, Debug, Clone)]
pub struct WeeklyArgs {
    /// Day to start the week (default: sunday)
    #[arg(long, default_value = "sunday")]
    pub start_of_week: String,
}

/// Arguments for the session report
#[derive(Args, Debug, Clone)]
pub struct SessionArgs {}

/// Arguments for the blocks report
#[derive(Args, Debug, Clone)]
pub struct BlocksArgs {
    /// Show only active blocks
    #[arg(long)]
    pub active: bool,

    /// Show only recent blocks (last 24h)
    #[arg(long)]
    pub recent: bool,

    /// Token limit for warnings
    #[arg(long)]
    pub token_limit: Option<String>,

    /// Session duration in hours for billing blocks
    #[arg(long, default_value = "5.0")]
    pub session_duration: f64,

    /// Maximum cost limit in USD for progress calculations (defaults to historical maximum)
    #[arg(long)]
    pub max_cost: Option<f64>,
}

/// Arguments for the statusline command
#[derive(Args, Debug, Clone)]
pub struct StatuslineArgs {
    /// Monthly subscription fee in USD (default: 200)
    #[arg(long, default_value = "200")]
    pub monthly_fee: f64,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,

    /// Show date and time
    #[arg(long)]
    pub show_date: bool,

    /// Show git branch
    #[arg(long)]
    pub show_git: bool,
}

/// Arguments for the watch command (hidden alias)
#[derive(Args, Debug, Clone)]
pub struct WatchArgs {
    /// Maximum cost limit in USD for progress calculations (defaults to historical maximum)
    #[arg(long)]
    pub max_cost: Option<f64>,
}

// ---------------------------------------------------------------------------
// Report subcommand (nested under each provider)
// ---------------------------------------------------------------------------

/// Report types available within a provider
#[derive(Subcommand, Debug, Clone)]
pub enum Report {
    /// Show daily usage summary
    Daily(DailyArgs),
    /// Show monthly usage summary
    Monthly,
    /// Show weekly usage summary
    Weekly(WeeklyArgs),
    /// Show session-based usage
    Session(SessionArgs),
    /// Show 5-hour billing blocks
    Blocks(BlocksArgs),
    /// Generate statusline output
    Statusline(StatuslineArgs),
}

// ---------------------------------------------------------------------------
// Top-level command (providers + report shortcuts + special commands)
// ---------------------------------------------------------------------------

/// Available commands
///
/// Provider subcommands (`claude`, `codex`, etc.) accept a nested report.
/// Report names used directly (`daily`, `monthly`, etc.) default to the
/// Claude provider.
#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    // -- Provider subcommands ------------------------------------------------
    /// Claude Code usage data
    Claude {
        #[command(subcommand)]
        report: Report,
    },
    /// Codex usage data
    Codex {
        #[command(subcommand)]
        report: Report,
    },
    /// OpenCode usage data
    Opencode {
        #[command(subcommand)]
        report: Report,
    },
    /// Amp usage data
    Amp {
        #[command(subcommand)]
        report: Report,
    },
    /// Pi Agent usage data
    Pi {
        #[command(subcommand)]
        report: Report,
    },

    // -- Report shortcuts (implicit Claude provider) -------------------------
    /// Show daily usage summary (provider: claude)
    Daily(DailyArgs),
    /// Show monthly usage summary (provider: claude)
    Monthly,
    /// Show weekly usage summary (provider: claude)
    Weekly(WeeklyArgs),
    /// Show session-based usage (provider: claude)
    Session(SessionArgs),
    /// Show 5-hour billing blocks (provider: claude)
    Blocks(BlocksArgs),
    /// Generate statusline output for Claude Code
    Statusline(StatuslineArgs),

    // -- Special commands ----------------------------------------------------
    /// Start MCP server
    Mcp,

    /// Live monitor for active billing blocks (alias for blocks --watch --active)
    #[command(hide = true)]
    Watch(WatchArgs),
}

// ---------------------------------------------------------------------------
// Resolution and validation
// ---------------------------------------------------------------------------

/// Resolve a top-level command into a (Provider, Report) pair.
///
/// Returns `None` for special commands (Mcp, Watch) that need separate handling.
pub fn resolve_provider_report(cmd: &Command) -> Option<(Provider, Report)> {
    match cmd.clone() {
        // Provider subcommands
        Command::Claude { report } => Some((Provider::Claude, report)),
        Command::Codex { report } => Some((Provider::Codex, report)),
        Command::Opencode { report } => Some((Provider::Opencode, report)),
        Command::Amp { report } => Some((Provider::Amp, report)),
        Command::Pi { report } => Some((Provider::Pi, report)),

        // Report shortcuts → Claude
        Command::Daily(args) => Some((Provider::Claude, Report::Daily(args))),
        Command::Monthly => Some((Provider::Claude, Report::Monthly)),
        Command::Weekly(args) => Some((Provider::Claude, Report::Weekly(args))),
        Command::Session(args) => Some((Provider::Claude, Report::Session(args))),
        Command::Blocks(args) => Some((Provider::Claude, Report::Blocks(args))),
        Command::Statusline(args) => Some((Provider::Claude, Report::Statusline(args))),

        // Special commands
        Command::Mcp | Command::Watch(_) => None,
    }
}

/// Validate that a provider supports the given report type.
///
/// Returns an error for unsupported combinations per the provider-report matrix.
pub fn validate_provider_report(provider: Provider, report: &Report) -> Result<()> {
    let supported = match (&provider, report) {
        // All providers support daily, monthly, session
        (_, Report::Daily(_) | Report::Monthly | Report::Session(_)) => true,

        // Weekly: only Claude and OpenCode
        (Provider::Claude | Provider::Opencode, Report::Weekly(_)) => true,

        // Blocks: only Claude
        (Provider::Claude, Report::Blocks(_)) => true,

        // Statusline: only Claude
        (Provider::Claude, Report::Statusline(_)) => true,

        // Everything else is unsupported
        _ => false,
    };

    if supported {
        Ok(())
    } else {
        let report_name = match report {
            Report::Daily(_) => "daily",
            Report::Monthly => "monthly",
            Report::Weekly(_) => "weekly",
            Report::Session(_) => "session",
            Report::Blocks(_) => "blocks",
            Report::Statusline(_) => "statusline",
        };
        Err(CcstatError::Config(format!(
            "The '{report_name}' report is not supported for the '{provider}' provider"
        )))
    }
}

/// Check whether a command targets the statusline (used to skip logging init).
pub fn is_statusline_command(cmd: &Option<Command>) -> bool {
    matches!(
        cmd,
        Some(
            Command::Statusline(_)
                | Command::Claude {
                    report: Report::Statusline(_),
                    ..
                }
        )
    )
}

// ---------------------------------------------------------------------------
// Date parsing
// ---------------------------------------------------------------------------

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
        // Test global JSON flag (no command → None)
        let cli = Cli::parse_from(["ccstat", "--json"]);
        assert!(cli.json);
        assert!(cli.command.is_none());

        // Test with daily command (shortcut)
        let cli = Cli::parse_from(["ccstat", "daily", "--instances"]);
        match &cli.command {
            Some(Command::Daily(args)) => assert!(args.instances),
            _ => panic!("Expected Daily command"),
        }
    }

    #[test]
    fn test_provider_subcommand() {
        // ccstat claude daily --instances
        let cli = Cli::parse_from(["ccstat", "claude", "daily", "--instances"]);
        match &cli.command {
            Some(Command::Claude {
                report: Report::Daily(args),
            }) => assert!(args.instances),
            _ => panic!("Expected Claude Daily command"),
        }

        // ccstat codex monthly
        let cli = Cli::parse_from(["ccstat", "codex", "monthly"]);
        match &cli.command {
            Some(Command::Codex {
                report: Report::Monthly,
            }) => {}
            _ => panic!("Expected Codex Monthly command"),
        }
    }

    #[test]
    fn test_resolve_provider_report() {
        // Shortcut: daily → (Claude, Daily)
        let cmd = Command::Daily(DailyArgs {
            instances: false,
            detailed: false,
        });
        let (provider, report) = resolve_provider_report(&cmd).unwrap();
        assert_eq!(provider, Provider::Claude);
        assert!(matches!(report, Report::Daily(_)));

        // Explicit: codex daily → (Codex, Daily)
        let cmd = Command::Codex {
            report: Report::Daily(DailyArgs {
                instances: false,
                detailed: false,
            }),
        };
        let (provider, report) = resolve_provider_report(&cmd).unwrap();
        assert_eq!(provider, Provider::Codex);
        assert!(matches!(report, Report::Daily(_)));

        // Special commands return None
        assert!(resolve_provider_report(&Command::Mcp).is_none());
    }

    #[test]
    fn test_validate_provider_report_matrix() {
        // Claude supports everything
        assert!(
            validate_provider_report(
                Provider::Claude,
                &Report::Daily(DailyArgs {
                    instances: false,
                    detailed: false
                })
            )
            .is_ok()
        );
        assert!(validate_provider_report(Provider::Claude, &Report::Monthly).is_ok());
        assert!(
            validate_provider_report(
                Provider::Claude,
                &Report::Weekly(WeeklyArgs {
                    start_of_week: "sunday".into()
                })
            )
            .is_ok()
        );
        assert!(
            validate_provider_report(
                Provider::Claude,
                &Report::Blocks(BlocksArgs {
                    active: false,
                    recent: false,
                    token_limit: None,
                    session_duration: 5.0,
                    max_cost: None
                })
            )
            .is_ok()
        );

        // Codex does NOT support weekly, blocks, statusline
        assert!(
            validate_provider_report(
                Provider::Codex,
                &Report::Weekly(WeeklyArgs {
                    start_of_week: "sunday".into()
                })
            )
            .is_err()
        );
        assert!(
            validate_provider_report(
                Provider::Codex,
                &Report::Blocks(BlocksArgs {
                    active: false,
                    recent: false,
                    token_limit: None,
                    session_duration: 5.0,
                    max_cost: None
                })
            )
            .is_err()
        );

        // OpenCode supports weekly
        assert!(
            validate_provider_report(
                Provider::Opencode,
                &Report::Weekly(WeeklyArgs {
                    start_of_week: "sunday".into()
                })
            )
            .is_ok()
        );

        // Amp does not support weekly
        assert!(
            validate_provider_report(
                Provider::Amp,
                &Report::Weekly(WeeklyArgs {
                    start_of_week: "sunday".into()
                })
            )
            .is_err()
        );
    }

    #[test]
    fn test_is_statusline_command() {
        // Direct shortcut
        assert!(is_statusline_command(&Some(Command::Statusline(
            StatuslineArgs {
                monthly_fee: 200.0,
                no_color: false,
                show_date: false,
                show_git: false,
            }
        ))));

        // Via provider
        assert!(is_statusline_command(&Some(Command::Claude {
            report: Report::Statusline(StatuslineArgs {
                monthly_fee: 200.0,
                no_color: false,
                show_date: false,
                show_git: false,
            })
        })));

        // Not statusline
        assert!(!is_statusline_command(&Some(Command::Daily(DailyArgs {
            instances: false,
            detailed: false,
        }))));
        assert!(!is_statusline_command(&None));
    }

    #[test]
    fn test_cost_mode_parsing() {
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
