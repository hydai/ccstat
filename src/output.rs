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

use crate::aggregation::{DailyInstanceUsage, DailyUsage, MonthlyUsage, SessionBlock, SessionUsage, Totals};
use prettytable::{format, row, Cell, Row, Table};
use serde_json::json;

/// Trait for output formatters
///
/// This trait defines the interface for formatting various types of usage data.
/// Implementations can provide different output formats (table, JSON, CSV, etc.).
pub trait OutputFormatter {
    /// Format daily usage data
    fn format_daily(&self, data: &[DailyUsage], totals: &Totals) -> String;

    /// Format daily usage data grouped by instance
    fn format_daily_by_instance(&self, data: &[DailyInstanceUsage], totals: &Totals) -> String;

    /// Format session usage data
    fn format_sessions(&self, data: &[SessionUsage], totals: &Totals) -> String;

    /// Format monthly usage data
    fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String;

    /// Format billing blocks
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

        table.to_string()
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
            "daily": data.iter().map(|d| json!({
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
/// # Arguments
///
/// * `json` - If true, returns a JSON formatter; otherwise returns a table formatter
///
/// # Returns
///
/// A boxed trait object implementing the OutputFormatter trait
///
/// # Example
///
/// ```
/// use ccstat::output::get_formatter;
///
/// let table_formatter = get_formatter(false);
/// let json_formatter = get_formatter(true);
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

    #[test]
    fn test_number_formatting() {
        assert_eq!(TableFormatter::format_number(1234567), "1,234,567");
        assert_eq!(TableFormatter::format_number(999), "999");
        assert_eq!(TableFormatter::format_number(0), "0");
    }

    #[test]
    fn test_currency_formatting() {
        assert_eq!(TableFormatter::format_currency(12.345), "$12.35");
        assert_eq!(TableFormatter::format_currency(0.0), "$0.00");
        assert_eq!(TableFormatter::format_currency(1000.0), "$1000.00");
    }
}
