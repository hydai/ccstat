//! Output formatting module for ccstat
//!
//! This module provides formatters for displaying usage data in different formats:
//! - Table format for human-readable terminal output
//! - JSON format for machine-readable output and integration with other tools
//!
//! # Examples
//!
//! ```no_run
//! use ccstat::output::get_formatter;
//! use ccstat::aggregation::{DailyUsage, Totals};
//! use ccstat::types::{DailyDate, TokenCounts};
//! use chrono::NaiveDate;
//!
//! let daily_data = vec![
//!     DailyUsage {
//!         date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
//!         tokens: TokenCounts::new(1000, 500, 100, 50),
//!         total_cost: 0.025,
//!         models_used: vec!["claude-3-opus".to_string()],
//!         entries: None,
//!     },
//! ];
//!
//! let totals = Totals::from_daily(&daily_data);
//!
//! // Get table formatter for human-readable output
//! let formatter = get_formatter(false);
//! println!("{}", formatter.format_daily(&daily_data, &totals));
//!
//! // Get JSON formatter for machine-readable output
//! let json_formatter = get_formatter(true);
//! println!("{}", json_formatter.format_daily(&daily_data, &totals));
//! ```

use crate::aggregation::{
    DailyInstanceUsage, DailyUsage, MonthlyUsage, SessionBlock, SessionUsage, Totals,
};
use prettytable::{Cell, Row, Table, format, row};
use serde_json::json;

/// Trait for output formatters
///
/// This trait defines the interface for formatting various types of usage data.
/// Implementations can provide different output formats (table, JSON, CSV, etc.).
///
/// # Example Implementation
///
/// ```
/// use ccstat::output::OutputFormatter;
/// use ccstat::aggregation::{DailyUsage, DailyInstanceUsage, SessionUsage, MonthlyUsage, SessionBlock, Totals};
///
/// struct CustomFormatter;
///
/// impl OutputFormatter for CustomFormatter {
///     fn format_daily(&self, data: &[DailyUsage], totals: &Totals) -> String {
///         format!("Total days: {}, Total cost: ${:.2}", data.len(), totals.total_cost)
///     }
///     
///     fn format_daily_by_instance(&self, data: &[DailyInstanceUsage], totals: &Totals) -> String {
///         format!("Total instances: {}", data.len())
///     }
///     
///     fn format_sessions(&self, data: &[SessionUsage], totals: &Totals) -> String {
///         format!("Total sessions: {}", data.len())
///     }
///     
///     fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String {
///         format!("Total months: {}", data.len())
///     }
///     
///     fn format_blocks(&self, data: &[SessionBlock]) -> String {
///         format!("Total blocks: {}", data.len())
///     }
/// }
/// ```
pub trait OutputFormatter {
    /// Format daily usage data with totals
    fn format_daily(&self, data: &[DailyUsage], totals: &Totals) -> String;

    /// Format daily usage data grouped by instance
    fn format_daily_by_instance(&self, data: &[DailyInstanceUsage], totals: &Totals) -> String;

    /// Format session usage data with totals
    fn format_sessions(&self, data: &[SessionUsage], totals: &Totals) -> String;

    /// Format monthly usage data with totals
    fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String;

    /// Format billing blocks (5-hour windows)
    fn format_blocks(&self, data: &[SessionBlock]) -> String;
}

/// Table formatter for human-readable output
///
/// Produces nicely formatted ASCII tables suitable for terminal display.
/// Numbers are formatted with thousands separators and costs are shown
/// with dollar signs for clarity.
pub struct TableFormatter;

impl TableFormatter {
    /// Format a number with thousands separators
    fn format_number(n: u64) -> String {
        let s = n.to_string();
        let mut result = String::new();

        for (count, ch) in s.chars().rev().enumerate() {
            if count > 0 && count % 3 == 0 {
                result.push(',');
            }
            result.push(ch);
        }

        result.chars().rev().collect()
    }

    /// Format currency with dollar sign
    fn format_currency(amount: f64) -> String {
        format!("${amount:.2}")
    }

    /// Create a totals row for tables
    fn format_totals_row(totals: &Totals) -> Row {
        row![
            b -> "TOTAL",
            b -> Self::format_number(totals.tokens.input_tokens),
            b -> Self::format_number(totals.tokens.output_tokens),
            b -> Self::format_number(totals.tokens.cache_creation_tokens),
            b -> Self::format_number(totals.tokens.cache_read_tokens),
            b -> Self::format_number(totals.tokens.total()),
            b -> Self::format_currency(totals.total_cost),
            ""
        ]
    }
}

impl OutputFormatter for TableFormatter {
    fn format_daily(&self, data: &[DailyUsage], totals: &Totals) -> String {
        let mut output = String::new();

        // Check if we have verbose entries
        let is_verbose = data.iter().any(|d| d.entries.is_some());

        if is_verbose {
            // Verbose mode: show detailed entries for each day
            for daily in data {
                // Day header
                output.push_str(&format!("\n=== {} ===\n", daily.date.format("%Y-%m-%d")));

                if let Some(ref entries) = daily.entries {
                    let mut table = Table::new();
                    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

                    table.set_titles(row![
                        b -> "Time",
                        b -> "Session ID",
                        b -> "Model",
                        b -> "Input",
                        b -> "Output",
                        b -> "Cache Create",
                        b -> "Cache Read",
                        b -> "Total",
                        b -> "Cost"
                    ]);

                    for entry in entries {
                        table.add_row(row![
                            entry.timestamp.format("%H:%M:%S"),
                            entry.session_id,
                            entry.model,
                            r -> Self::format_number(entry.tokens.input_tokens),
                            r -> Self::format_number(entry.tokens.output_tokens),
                            r -> Self::format_number(entry.tokens.cache_creation_tokens),
                            r -> Self::format_number(entry.tokens.cache_read_tokens),
                            r -> Self::format_number(entry.tokens.total()),
                            r -> Self::format_currency(entry.cost)
                        ]);
                    }

                    output.push_str(&table.to_string());
                }

                // Day summary
                output.push_str(&format!(
                    "\nDay Total: {} tokens, {}\n",
                    Self::format_number(daily.tokens.total()),
                    Self::format_currency(daily.total_cost)
                ));
            }

            // Overall summary
            output.push_str("\n=== OVERALL SUMMARY ===\n");
        }

        // Regular summary table (shown in both verbose and non-verbose modes)
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b -> "Date",
            b -> "Input",
            b -> "Output",
            b -> "Cache Create",
            b -> "Cache Read",
            b -> "Total",
            b -> "Cost",
            b -> "Models"
        ]);

        for entry in data {
            table.add_row(row![
                entry.date.format("%Y-%m-%d"),
                r -> Self::format_number(entry.tokens.input_tokens),
                r -> Self::format_number(entry.tokens.output_tokens),
                r -> Self::format_number(entry.tokens.cache_creation_tokens),
                r -> Self::format_number(entry.tokens.cache_read_tokens),
                r -> Self::format_number(entry.tokens.total()),
                r -> Self::format_currency(entry.total_cost),
                entry.models_used.join(", ")
            ]);
        }

        // Add separator
        table.add_row(Row::new(vec![Cell::new(""); 8]));

        // Add totals row
        table.add_row(Self::format_totals_row(totals));

        output.push_str(&table.to_string());
        output
    }

    fn format_daily_by_instance(&self, data: &[DailyInstanceUsage], totals: &Totals) -> String {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b -> "Date",
            b -> "Instance",
            b -> "Input",
            b -> "Output",
            b -> "Cache Create",
            b -> "Cache Read",
            b -> "Total Tokens",
            b -> "Cost",
            b -> "Models"
        ]);

        for entry in data {
            table.add_row(row![
                entry.date.format("%Y-%m-%d"),
                entry.instance_id,
                r -> Self::format_number(entry.tokens.input_tokens),
                r -> Self::format_number(entry.tokens.output_tokens),
                r -> Self::format_number(entry.tokens.cache_creation_tokens),
                r -> Self::format_number(entry.tokens.cache_read_tokens),
                r -> Self::format_number(entry.tokens.total()),
                r -> Self::format_currency(entry.total_cost),
                entry.models_used.join(", ")
            ]);
        }

        // Add separator
        table.add_row(Row::new(vec![Cell::new(""); 9]));

        // Add totals row with extra column for instance
        table.add_row(row![
            b -> "TOTAL",
            "",
            b -> Self::format_number(totals.tokens.input_tokens),
            b -> Self::format_number(totals.tokens.output_tokens),
            b -> Self::format_number(totals.tokens.cache_creation_tokens),
            b -> Self::format_number(totals.tokens.cache_read_tokens),
            b -> Self::format_number(totals.tokens.total()),
            b -> Self::format_currency(totals.total_cost),
            ""
        ]);

        table.to_string()
    }

    fn format_sessions(&self, data: &[SessionUsage], totals: &Totals) -> String {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b -> "Session ID",
            b -> "Start Time",
            b -> "Duration",
            b -> "Input",
            b -> "Output",
            b -> "Total Tokens",
            b -> "Cost",
            b -> "Model"
        ]);

        for session in data {
            let duration = session.end_time - session.start_time;
            let duration_str =
                format!("{}h {}m", duration.num_hours(), duration.num_minutes() % 60);

            table.add_row(row![
                session.session_id.as_str(),
                session.start_time.format("%Y-%m-%d %H:%M"),
                duration_str,
                r -> Self::format_number(session.tokens.input_tokens),
                r -> Self::format_number(session.tokens.output_tokens),
                r -> Self::format_number(session.tokens.total()),
                r -> Self::format_currency(session.total_cost),
                session.model.as_str()
            ]);
        }

        // Add separator
        table.add_row(Row::new(vec![Cell::new(""); 8]));

        // Add totals row
        table.add_row(row![
            b -> "TOTAL",
            "",
            "",
            b -> Self::format_number(totals.tokens.input_tokens),
            b -> Self::format_number(totals.tokens.output_tokens),
            b -> Self::format_number(totals.tokens.total()),
            b -> Self::format_currency(totals.total_cost),
            ""
        ]);

        table.to_string()
    }

    fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b -> "Month",
            b -> "Input",
            b -> "Output",
            b -> "Cache Create",
            b -> "Cache Read",
            b -> "Total",
            b -> "Cost",
            b -> "Active Days"
        ]);

        for entry in data {
            table.add_row(row![
                entry.month,
                r -> Self::format_number(entry.tokens.input_tokens),
                r -> Self::format_number(entry.tokens.output_tokens),
                r -> Self::format_number(entry.tokens.cache_creation_tokens),
                r -> Self::format_number(entry.tokens.cache_read_tokens),
                r -> Self::format_number(entry.tokens.total()),
                r -> Self::format_currency(entry.total_cost),
                c -> entry.active_days
            ]);
        }

        // Add separator
        table.add_row(Row::new(vec![Cell::new(""); 8]));

        // Add totals row
        table.add_row(Self::format_totals_row(totals));

        table.to_string()
    }

    fn format_blocks(&self, data: &[SessionBlock]) -> String {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        table.set_titles(row![
            b -> "Block Start",
            b -> "Status",
            b -> "Sessions",
            b -> "Input",
            b -> "Output",
            b -> "Total Tokens",
            b -> "Cost",
            b -> "Time Remaining"
        ]);

        for block in data {
            let status = if block.is_active {
                "ACTIVE"
            } else {
                "Complete"
            };
            let time_remaining = if block.is_active {
                let remaining = block.end_time - chrono::Utc::now();
                if remaining.num_seconds() > 0 {
                    format!(
                        "{}h {}m",
                        remaining.num_hours(),
                        remaining.num_minutes() % 60
                    )
                } else {
                    "Expired".to_string()
                }
            } else {
                "-".to_string()
            };

            table.add_row(row![
                block.start_time.format("%Y-%m-%d %H:%M"),
                status,
                c -> block.sessions.len(),
                r -> Self::format_number(block.tokens.input_tokens),
                r -> Self::format_number(block.tokens.output_tokens),
                r -> Self::format_number(block.tokens.total()),
                r -> Self::format_currency(block.total_cost),
                time_remaining
            ]);
        }

        table.to_string()
    }
}

/// JSON formatter for machine-readable output
///
/// Produces structured JSON output that can be easily parsed by other tools
/// or used in automation pipelines. All data is preserved in its raw form
/// for maximum flexibility.
pub struct JsonFormatter;

impl OutputFormatter for JsonFormatter {
    fn format_daily(&self, data: &[DailyUsage], totals: &Totals) -> String {
        let output = json!({
            "daily": data.iter().map(|d| {
                let mut day_json = json!({
                    "date": d.date.format("%Y-%m-%d"),
                    "tokens": {
                        "input_tokens": d.tokens.input_tokens,
                        "output_tokens": d.tokens.output_tokens,
                        "cache_creation_tokens": d.tokens.cache_creation_tokens,
                        "cache_read_tokens": d.tokens.cache_read_tokens,
                        "total": d.tokens.total(),
                    },
                    "total_cost": d.total_cost,
                    "models_used": d.models_used,
                });

                // Add verbose entries if available
                if let Some(ref entries) = d.entries {
                    day_json["entries"] = json!(entries.iter().map(|e| json!({
                        "timestamp": e.timestamp.to_rfc3339(),
                        "session_id": e.session_id,
                        "model": e.model,
                        "tokens": {
                            "input_tokens": e.tokens.input_tokens,
                            "output_tokens": e.tokens.output_tokens,
                            "cache_creation_tokens": e.tokens.cache_creation_tokens,
                            "cache_read_tokens": e.tokens.cache_read_tokens,
                            "total": e.tokens.total(),
                        },
                        "cost": e.cost,
                    })).collect::<Vec<_>>());
                }

                day_json
            }).collect::<Vec<_>>(),
            "totals": {
                "tokens": {
                    "input_tokens": totals.tokens.input_tokens,
                    "output_tokens": totals.tokens.output_tokens,
                    "cache_creation_tokens": totals.tokens.cache_creation_tokens,
                    "cache_read_tokens": totals.tokens.cache_read_tokens,
                    "total": totals.tokens.total(),
                },
                "total_cost": totals.total_cost,
            }
        });

        serde_json::to_string_pretty(&output).unwrap()
    }

    fn format_daily_by_instance(&self, data: &[DailyInstanceUsage], totals: &Totals) -> String {
        let output = json!({
            "daily_by_instance": data.iter().map(|d| json!({
                "date": d.date.format("%Y-%m-%d"),
                "instance_id": d.instance_id,
                "tokens": {
                    "input_tokens": d.tokens.input_tokens,
                    "output_tokens": d.tokens.output_tokens,
                    "cache_creation_tokens": d.tokens.cache_creation_tokens,
                    "cache_read_tokens": d.tokens.cache_read_tokens,
                    "total": d.tokens.total(),
                },
                "total_cost": d.total_cost,
                "models_used": d.models_used,
            })).collect::<Vec<_>>(),
            "totals": {
                "tokens": {
                    "input_tokens": totals.tokens.input_tokens,
                    "output_tokens": totals.tokens.output_tokens,
                    "cache_creation_tokens": totals.tokens.cache_creation_tokens,
                    "cache_read_tokens": totals.tokens.cache_read_tokens,
                    "total": totals.tokens.total(),
                },
                "total_cost": totals.total_cost,
            }
        });

        serde_json::to_string_pretty(&output).unwrap()
    }

    fn format_sessions(&self, data: &[SessionUsage], totals: &Totals) -> String {
        let output = json!({
            "sessions": data.iter().map(|s| json!({
                "session_id": s.session_id.as_str(),
                "start_time": s.start_time.to_rfc3339(),
                "end_time": s.end_time.to_rfc3339(),
                "duration_seconds": (s.end_time - s.start_time).num_seconds(),
                "tokens": {
                    "input_tokens": s.tokens.input_tokens,
                    "output_tokens": s.tokens.output_tokens,
                    "cache_creation_tokens": s.tokens.cache_creation_tokens,
                    "cache_read_tokens": s.tokens.cache_read_tokens,
                    "total": s.tokens.total(),
                },
                "total_cost": s.total_cost,
                "model": s.model.as_str(),
            })).collect::<Vec<_>>(),
            "totals": {
                "tokens": {
                    "input_tokens": totals.tokens.input_tokens,
                    "output_tokens": totals.tokens.output_tokens,
                    "cache_creation_tokens": totals.tokens.cache_creation_tokens,
                    "cache_read_tokens": totals.tokens.cache_read_tokens,
                    "total": totals.tokens.total(),
                },
                "total_cost": totals.total_cost,
            }
        });

        serde_json::to_string_pretty(&output).unwrap()
    }

    fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String {
        let output = json!({
            "monthly": data.iter().map(|m| json!({
                "month": m.month,
                "tokens": {
                    "input_tokens": m.tokens.input_tokens,
                    "output_tokens": m.tokens.output_tokens,
                    "cache_creation_tokens": m.tokens.cache_creation_tokens,
                    "cache_read_tokens": m.tokens.cache_read_tokens,
                    "total": m.tokens.total(),
                },
                "total_cost": m.total_cost,
                "active_days": m.active_days,
            })).collect::<Vec<_>>(),
            "totals": {
                "tokens": {
                    "input_tokens": totals.tokens.input_tokens,
                    "output_tokens": totals.tokens.output_tokens,
                    "cache_creation_tokens": totals.tokens.cache_creation_tokens,
                    "cache_read_tokens": totals.tokens.cache_read_tokens,
                    "total": totals.tokens.total(),
                },
                "total_cost": totals.total_cost,
            }
        });

        serde_json::to_string_pretty(&output).unwrap()
    }

    fn format_blocks(&self, data: &[SessionBlock]) -> String {
        let output = json!({
            "blocks": data.iter().map(|b| json!({
                "start_time": b.start_time.to_rfc3339(),
                "end_time": b.end_time.to_rfc3339(),
                "is_active": b.is_active,
                "session_count": b.sessions.len(),
                "tokens": {
                    "input_tokens": b.tokens.input_tokens,
                    "output_tokens": b.tokens.output_tokens,
                    "cache_creation_tokens": b.tokens.cache_creation_tokens,
                    "cache_read_tokens": b.tokens.cache_read_tokens,
                    "total": b.tokens.total(),
                },
                "total_cost": b.total_cost,
                "sessions": b.sessions.iter().map(|s| s.session_id.as_str()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()
        });

        serde_json::to_string_pretty(&output).unwrap()
    }
}

/// Get appropriate formatter based on JSON flag
///
/// This is the main entry point for obtaining a formatter. It returns the appropriate
/// formatter based on whether JSON output is requested.
///
/// # Arguments
///
/// * `json` - If true, returns a JSON formatter; otherwise returns a table formatter
///
/// # Returns
///
/// A boxed trait object implementing the OutputFormatter trait
///
/// # Examples
///
/// ```
/// use ccstat::output::{get_formatter, OutputFormatter};
/// use ccstat::aggregation::{DailyUsage, Totals};
/// use ccstat::types::{DailyDate, TokenCounts};
/// use chrono::NaiveDate;
///
/// // Get table formatter for human-readable output
/// let formatter = get_formatter(false);
///
/// // Get JSON formatter for machine-readable output
/// let json_formatter = get_formatter(true);
///
/// // Use with data
/// let daily_data = vec![
///     DailyUsage {
///         date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
///         tokens: TokenCounts::new(1000, 500, 0, 0),
///         total_cost: 0.025,
///         models_used: vec!["claude-3-opus".to_string()],
///         entries: None,
///     },
/// ];
/// let totals = Totals::from_daily(&daily_data);
///
/// let output = formatter.format_daily(&daily_data, &totals);
/// ```
pub fn get_formatter(json: bool) -> Box<dyn OutputFormatter> {
    if json {
        Box::new(JsonFormatter)
    } else {
        Box::new(TableFormatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregation::VerboseEntry;
    use crate::types::{DailyDate, ModelName, SessionId, TokenCounts};
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_number_formatting() {
        assert_eq!(TableFormatter::format_number(1234567), "1,234,567");
        assert_eq!(TableFormatter::format_number(999), "999");
        assert_eq!(TableFormatter::format_number(0), "0");
        assert_eq!(TableFormatter::format_number(1000), "1,000");
        assert_eq!(TableFormatter::format_number(10000), "10,000");
        assert_eq!(TableFormatter::format_number(100000), "100,000");
        assert_eq!(TableFormatter::format_number(1000000), "1,000,000");
        assert_eq!(TableFormatter::format_number(u64::MAX), "18,446,744,073,709,551,615");
    }

    #[test]
    fn test_currency_formatting() {
        assert_eq!(TableFormatter::format_currency(12.345), "$12.35");
        assert_eq!(TableFormatter::format_currency(0.0), "$0.00");
        assert_eq!(TableFormatter::format_currency(1000.0), "$1000.00");
        assert_eq!(TableFormatter::format_currency(0.001), "$0.00");
        assert_eq!(TableFormatter::format_currency(0.005), "$0.01");
        assert_eq!(TableFormatter::format_currency(0.994), "$0.99");
        assert_eq!(TableFormatter::format_currency(0.996), "$1.00");
        assert_eq!(TableFormatter::format_currency(99999.99), "$99999.99");
        assert_eq!(TableFormatter::format_currency(-10.0), "$-10.00");
    }

    #[test]
    fn test_table_formatter_daily_verbose() {
        let formatter = TableFormatter;
        
        let timestamp = Utc.with_ymd_and_hms(2024, 1, 1, 10, 30, 0).unwrap();
        let daily_data = vec![
            DailyUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                tokens: TokenCounts::new(1000, 500, 100, 50),
                total_cost: 0.025,
                models_used: vec!["claude-3-opus".to_string()],
                entries: Some(vec![
                    VerboseEntry {
                        timestamp,
                        session_id: "test-session-1".to_string(),
                        model: "claude-3-opus".to_string(),
                        tokens: TokenCounts::new(500, 250, 50, 25),
                        cost: 0.0125,
                    },
                    VerboseEntry {
                        timestamp: timestamp + chrono::Duration::hours(1),
                        session_id: "test-session-2".to_string(),
                        model: "claude-3-opus".to_string(),
                        tokens: TokenCounts::new(500, 250, 50, 25),
                        cost: 0.0125,
                    },
                ]),
            },
        ];
        
        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);
        
        // Check verbose output structure
        assert!(output.contains("=== 2024-01-01 ==="));
        assert!(output.contains("test-session-1"));
        assert!(output.contains("test-session-2"));
        assert!(output.contains("Day Total"));
        assert!(output.contains("OVERALL SUMMARY"));
    }

    #[test]
    fn test_table_formatter_sessions_duration() {
        let formatter = TableFormatter;
        
        // Test various duration formats
        let base_time = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let sessions = vec![
            SessionUsage {
                session_id: SessionId::new("short-session"),
                start_time: base_time,
                end_time: base_time + chrono::Duration::minutes(30),
                tokens: TokenCounts::new(100, 50, 0, 0),
                total_cost: 0.005,
                model: ModelName::new("claude-3-opus"),
            },
            SessionUsage {
                session_id: SessionId::new("long-session"),
                start_time: base_time,
                end_time: base_time + chrono::Duration::hours(5) + chrono::Duration::minutes(45),
                tokens: TokenCounts::new(1000, 500, 0, 0),
                total_cost: 0.025,
                model: ModelName::new("claude-3-opus"),
            },
        ];
        
        let totals = Totals::from_sessions(&sessions);
        let output = formatter.format_sessions(&sessions, &totals);
        
        // Check duration formatting
        assert!(output.contains("0h 30m")); // 30 minutes
        assert!(output.contains("5h 45m")); // 5 hours 45 minutes
    }

    #[test]
    fn test_table_formatter_blocks_time_remaining() {
        let formatter = TableFormatter;
        
        let now = chrono::Utc::now();
        let blocks = vec![
            // Active block with time remaining
            SessionBlock {
                start_time: now - chrono::Duration::hours(2),
                end_time: now + chrono::Duration::hours(3),
                sessions: vec![],
                tokens: TokenCounts::new(1000, 500, 0, 0),
                total_cost: 0.025,
                is_active: true,
                warning: None,
            },
            // Active block that's expired
            SessionBlock {
                start_time: now - chrono::Duration::hours(6),
                end_time: now - chrono::Duration::hours(1),
                sessions: vec![],
                tokens: TokenCounts::new(2000, 1000, 0, 0),
                total_cost: 0.050,
                is_active: true,
                warning: None,
            },
            // Inactive block
            SessionBlock {
                start_time: now - chrono::Duration::hours(10),
                end_time: now - chrono::Duration::hours(5),
                sessions: vec![],
                tokens: TokenCounts::new(3000, 1500, 0, 0),
                total_cost: 0.075,
                is_active: false,
                warning: None,
            },
        ];
        
        let output = formatter.format_blocks(&blocks);
        
        // Check statuses and time remaining
        assert!(output.contains("ACTIVE"));
        assert!(output.contains("Complete"));
        assert!(output.contains("Expired"));
        assert!(output.contains("-")); // No time remaining for completed blocks
    }

    #[test]
    fn test_json_formatter_daily() {
        let formatter = JsonFormatter;
        
        let daily_data = vec![
            DailyUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                tokens: TokenCounts::new(1000, 500, 100, 50),
                total_cost: 0.025,
                models_used: vec!["claude-3-opus".to_string()],
                entries: None,
            },
            DailyUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()),
                tokens: TokenCounts::new(2000, 1000, 200, 100),
                total_cost: 0.050,
                models_used: vec!["claude-3-sonnet".to_string(), "claude-3-haiku".to_string()],
                entries: None,
            },
        ];
        
        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);
        
        // Parse JSON to verify structure
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        // Check daily array
        assert!(json["daily"].is_array());
        assert_eq!(json["daily"].as_array().unwrap().len(), 2);
        
        // Check first day
        assert_eq!(json["daily"][0]["date"], "2024-01-01");
        assert_eq!(json["daily"][0]["tokens"]["input_tokens"], 1000);
        assert_eq!(json["daily"][0]["tokens"]["output_tokens"], 500);
        assert_eq!(json["daily"][0]["tokens"]["total"], 1650);
        assert_eq!(json["daily"][0]["total_cost"], 0.025);
        assert_eq!(json["daily"][0]["models_used"][0], "claude-3-opus");
        
        // Check totals
        assert_eq!(json["totals"]["tokens"]["input_tokens"], 3000);
        assert_eq!(json["totals"]["tokens"]["output_tokens"], 1500);
        // Use approximate comparison for floating point
        let total_cost = json["totals"]["total_cost"].as_f64().unwrap();
        assert!((total_cost - 0.075).abs() < 1e-10);
    }

    #[test]
    fn test_json_formatter_daily_with_verbose() {
        let formatter = JsonFormatter;
        
        let timestamp = Utc.with_ymd_and_hms(2024, 1, 1, 10, 30, 0).unwrap();
        let daily_data = vec![
            DailyUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                tokens: TokenCounts::new(1000, 500, 100, 50),
                total_cost: 0.025,
                models_used: vec!["claude-3-opus".to_string()],
                entries: Some(vec![
                    VerboseEntry {
                        timestamp,
                        session_id: "test-session-1".to_string(),
                        model: "claude-3-opus".to_string(),
                        tokens: TokenCounts::new(1000, 500, 100, 50),
                        cost: 0.025,
                    },
                ]),
            },
        ];
        
        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        // Check verbose entries exist
        assert!(json["daily"][0]["entries"].is_array());
        assert_eq!(json["daily"][0]["entries"][0]["session_id"], "test-session-1");
        assert_eq!(json["daily"][0]["entries"][0]["model"], "claude-3-opus");
        assert_eq!(json["daily"][0]["entries"][0]["tokens"]["input_tokens"], 1000);
        assert_eq!(json["daily"][0]["entries"][0]["cost"], 0.025);
    }

    #[test]
    fn test_json_formatter_daily_by_instance() {
        let formatter = JsonFormatter;
        
        let instance_data = vec![
            DailyInstanceUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                instance_id: "instance-a".to_string(),
                tokens: TokenCounts::new(1000, 500, 0, 0),
                total_cost: 0.025,
                models_used: vec!["claude-3-opus".to_string()],
            },
            DailyInstanceUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                instance_id: "instance-b".to_string(),
                tokens: TokenCounts::new(2000, 1000, 0, 0),
                total_cost: 0.050,
                models_used: vec!["claude-3-sonnet".to_string()],
            },
        ];
        
        let totals = Totals::from_daily_instances(&instance_data);
        let output = formatter.format_daily_by_instance(&instance_data, &totals);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        assert!(json["daily_by_instance"].is_array());
        assert_eq!(json["daily_by_instance"].as_array().unwrap().len(), 2);
        
        // Check instances
        assert_eq!(json["daily_by_instance"][0]["instance_id"], "instance-a");
        assert_eq!(json["daily_by_instance"][1]["instance_id"], "instance-b");
        
        // Check totals
        assert_eq!(json["totals"]["tokens"]["input_tokens"], 3000);
        let total_cost = json["totals"]["total_cost"].as_f64().unwrap();
        assert!((total_cost - 0.075).abs() < 1e-10);
    }

    #[test]
    fn test_json_formatter_sessions() {
        let formatter = JsonFormatter;
        
        let start_time = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let end_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 30, 0).unwrap();
        
        let sessions = vec![
            SessionUsage {
                session_id: SessionId::new("session-1"),
                start_time,
                end_time,
                tokens: TokenCounts::new(5000, 2500, 500, 250),
                total_cost: 0.1275,
                model: ModelName::new("claude-3-opus"),
            },
        ];
        
        let totals = Totals::from_sessions(&sessions);
        let output = formatter.format_sessions(&sessions, &totals);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        assert!(json["sessions"].is_array());
        assert_eq!(json["sessions"][0]["session_id"], "session-1");
        assert_eq!(json["sessions"][0]["duration_seconds"], 9000); // 2.5 hours
        assert_eq!(json["sessions"][0]["tokens"]["input_tokens"], 5000);
        assert_eq!(json["sessions"][0]["model"], "claude-3-opus");
    }

    #[test]
    fn test_json_formatter_monthly() {
        let formatter = JsonFormatter;
        
        let monthly_data = vec![
            MonthlyUsage {
                month: "2024-01".to_string(),
                tokens: TokenCounts::new(500000, 250000, 50000, 25000),
                total_cost: 12.75,
                active_days: 20,
            },
            MonthlyUsage {
                month: "2024-02".to_string(),
                tokens: TokenCounts::new(600000, 300000, 60000, 30000),
                total_cost: 15.30,
                active_days: 18,
            },
        ];
        
        let totals = Totals::from_monthly(&monthly_data);
        let output = formatter.format_monthly(&monthly_data, &totals);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        assert!(json["monthly"].is_array());
        assert_eq!(json["monthly"].as_array().unwrap().len(), 2);
        
        // Check first month
        assert_eq!(json["monthly"][0]["month"], "2024-01");
        assert_eq!(json["monthly"][0]["active_days"], 20);
        assert_eq!(json["monthly"][0]["tokens"]["input_tokens"], 500000);
        
        // Check totals
        assert_eq!(json["totals"]["tokens"]["input_tokens"], 1100000);
        assert_eq!(json["totals"]["total_cost"], 28.05);
    }

    #[test]
    fn test_json_formatter_blocks() {
        let formatter = JsonFormatter;
        
        let start_time = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        let end_time = start_time + chrono::Duration::hours(5);
        
        let blocks = vec![
            SessionBlock {
                start_time,
                end_time,
                sessions: vec![
                    SessionUsage {
                        session_id: SessionId::new("s1"),
                        start_time,
                        end_time: start_time + chrono::Duration::hours(1),
                        tokens: TokenCounts::new(1000, 500, 0, 0),
                        total_cost: 0.025,
                        model: ModelName::new("claude-3-opus"),
                    },
                    SessionUsage {
                        session_id: SessionId::new("s2"),
                        start_time: start_time + chrono::Duration::hours(2),
                        end_time: start_time + chrono::Duration::hours(3),
                        tokens: TokenCounts::new(2000, 1000, 0, 0),
                        total_cost: 0.050,
                        model: ModelName::new("claude-3-opus"),
                    },
                ],
                tokens: TokenCounts::new(3000, 1500, 0, 0),
                total_cost: 0.075,
                is_active: false,
                warning: None,
            },
        ];
        
        let output = formatter.format_blocks(&blocks);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        assert!(json["blocks"].is_array());
        assert_eq!(json["blocks"][0]["session_count"], 2);
        assert_eq!(json["blocks"][0]["is_active"], false);
        assert_eq!(json["blocks"][0]["tokens"]["input_tokens"], 3000);
        assert_eq!(json["blocks"][0]["sessions"][0], "s1");
        assert_eq!(json["blocks"][0]["sessions"][1], "s2");
    }

    #[test]
    fn test_json_formatter_empty_data() {
        let formatter = JsonFormatter;
        
        // Test empty daily data
        let daily_data: Vec<DailyUsage> = vec![];
        let totals = Totals::default();
        let output = formatter.format_daily(&daily_data, &totals);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["daily"].as_array().unwrap().len(), 0);
        assert_eq!(json["totals"]["total_cost"], 0.0);
        
        // Test empty sessions
        let sessions: Vec<SessionUsage> = vec![];
        let output = formatter.format_sessions(&sessions, &totals);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["sessions"].as_array().unwrap().len(), 0);
        
        // Test empty monthly
        let monthly: Vec<MonthlyUsage> = vec![];
        let output = formatter.format_monthly(&monthly, &totals);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["monthly"].as_array().unwrap().len(), 0);
        
        // Test empty blocks
        let blocks: Vec<SessionBlock> = vec![];
        let output = formatter.format_blocks(&blocks);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["blocks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_json_formatter_large_numbers() {
        let formatter = JsonFormatter;
        
        let daily_data = vec![
            DailyUsage {
                date: DailyDate::new(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                tokens: TokenCounts::new(10_000_000, 5_000_000, 1_000_000, 500_000),
                total_cost: 255.50,
                models_used: vec!["claude-3-opus".to_string()],
                entries: None,
            },
        ];
        
        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);
        
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        
        // Verify large numbers are preserved accurately
        assert_eq!(json["daily"][0]["tokens"]["input_tokens"], 10_000_000);
        assert_eq!(json["daily"][0]["tokens"]["output_tokens"], 5_000_000);
        assert_eq!(json["daily"][0]["tokens"]["cache_creation_tokens"], 1_000_000);
        assert_eq!(json["daily"][0]["tokens"]["cache_read_tokens"], 500_000);
        assert_eq!(json["daily"][0]["tokens"]["total"], 16_500_000);
        assert_eq!(json["daily"][0]["total_cost"], 255.50);
    }

    #[test]
    fn test_get_formatter() {
        // Test table formatter returns correct type
        let formatter = get_formatter(false);
        // Verify it works by calling a method
        let empty_daily: Vec<DailyUsage> = vec![];
        let totals = Totals::default();
        let output = formatter.format_daily(&empty_daily, &totals);
        assert!(output.contains("TOTAL"));
        
        // Test JSON formatter returns correct type
        let formatter = get_formatter(true);
        let output = formatter.format_daily(&empty_daily, &totals);
        // Verify it's valid JSON
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(json.is_object());
    }
}
