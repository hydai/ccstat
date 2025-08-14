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
//! let formatter = get_formatter(false, false);
//! println!("{}", formatter.format_daily(&daily_data, &totals));
//!
//! // Get JSON formatter for machine-readable output
//! let json_formatter = get_formatter(true, false);
//! println!("{}", json_formatter.format_daily(&daily_data, &totals));
//! ```

use crate::aggregation::{
    DailyInstanceUsage, DailyUsage, MonthlyUsage, SessionBlock, SessionUsage, Totals,
};
use crate::model_formatter::{format_model_list, format_model_name};
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
///     fn format_sessions(&self, data: &[SessionUsage], totals: &Totals, _tz: &chrono_tz::Tz) -> String {
///         format!("Total sessions: {}", data.len())
///     }
///
///     fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String {
///         format!("Total months: {}", data.len())
///     }
///
///     fn format_blocks(&self, data: &[SessionBlock], _tz: &chrono_tz::Tz, _show_entries: bool) -> String {
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
    fn format_sessions(&self, data: &[SessionUsage], totals: &Totals, tz: &chrono_tz::Tz)
    -> String;

    /// Format monthly usage data with totals
    fn format_monthly(&self, data: &[MonthlyUsage], totals: &Totals) -> String;

    /// Format billing blocks (5-hour windows)
    fn format_blocks(
        &self,
        data: &[SessionBlock],
        tz: &chrono_tz::Tz,
        show_entries: bool,
    ) -> String;
}

/// Table formatter for human-readable output
///
/// Produces nicely formatted ASCII tables suitable for terminal display.
/// Numbers are formatted with thousands separators and costs are shown
/// with dollar signs for clarity.
pub struct TableFormatter {
    /// Whether to show full model names or shortened versions
    pub full_model_names: bool,
}

impl TableFormatter {
    /// Create a new TableFormatter
    pub fn new(full_model_names: bool) -> Self {
        Self { full_model_names }
    }

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

    /// Format a datetime with the specified timezone
    fn format_datetime_with_tz(dt: &chrono::DateTime<chrono::Utc>, tz: &chrono_tz::Tz) -> String {
        dt.with_timezone(tz).format("%Y-%m-%d %H:%M %Z").to_string()
    }

    /// Format blocks with custom current time (for testing)
    pub(crate) fn format_blocks_with_now(
        &self,
        data: &[SessionBlock],
        tz: &chrono_tz::Tz,
        now: chrono::DateTime<chrono::Utc>,
        show_entries: bool,
    ) -> String {
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
                let remaining = block.end_time - now;
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

            let formatted_start = Self::format_datetime_with_tz(&block.start_time, tz);

            table.add_row(row![
                formatted_start,
                status,
                c -> block.sessions.len(),
                r -> Self::format_number(block.tokens.input_tokens),
                r -> Self::format_number(block.tokens.output_tokens),
                r -> Self::format_number(block.tokens.total()),
                r -> Self::format_currency(block.total_cost),
                time_remaining
            ]);

            // Show individual session entries if requested
            if show_entries && !block.sessions.is_empty() {
                for session in &block.sessions {
                    let session_start = Self::format_datetime_with_tz(&session.start_time, tz);
                    let session_end = session
                        .end_time
                        .with_timezone(tz)
                        .format("%H:%M")
                        .to_string();
                    let duration = session.end_time - session.start_time;
                    let duration_str = format!("{}m", duration.num_minutes());

                    let model_name = if self.full_model_names {
                        session.model.to_string()
                    } else {
                        crate::model_formatter::format_model_name(&session.model.to_string(), false)
                    };

                    table.add_row(row![
                        format!("  └─ {}-{}", session_start, session_end),
                        duration_str,
                        session.session_id.as_str(),
                        r -> Self::format_number(session.tokens.input_tokens),
                        r -> Self::format_number(session.tokens.output_tokens),
                        r -> Self::format_number(session.tokens.total()),
                        r -> Self::format_currency(session.total_cost),
                        model_name
                    ]);
                }
            }
        }

        table.to_string()
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
                            format_model_name(&entry.model, self.full_model_names),
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
                format_model_list(&entry.models_used, self.full_model_names, ", ")
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
                format_model_list(&entry.models_used, self.full_model_names, ", ")
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

    fn format_sessions(
        &self,
        data: &[SessionUsage],
        totals: &Totals,
        tz: &chrono_tz::Tz,
    ) -> String {
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

            let formatted_start = Self::format_datetime_with_tz(&session.start_time, tz);

            table.add_row(row![
                session.session_id.as_str(),
                formatted_start,
                duration_str,
                r -> Self::format_number(session.tokens.input_tokens),
                r -> Self::format_number(session.tokens.output_tokens),
                r -> Self::format_number(session.tokens.total()),
                r -> Self::format_currency(session.total_cost),
                format_model_name(session.model.as_str(), self.full_model_names)
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

    fn format_blocks(
        &self,
        data: &[SessionBlock],
        tz: &chrono_tz::Tz,
        show_entries: bool,
    ) -> String {
        self.format_blocks_with_now(data, tz, chrono::Utc::now(), show_entries)
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

    fn format_sessions(
        &self,
        data: &[SessionUsage],
        totals: &Totals,
        _tz: &chrono_tz::Tz,
    ) -> String {
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

    fn format_blocks(
        &self,
        data: &[SessionBlock],
        _tz: &chrono_tz::Tz,
        show_entries: bool,
    ) -> String {
        let output = json!({
            "blocks": data.iter().map(|b| {
                let mut block_json = json!({
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
                });

                // Include full session details if requested, otherwise just session IDs
                if show_entries {
                    block_json["sessions"] = json!(b.sessions.iter().map(|s| json!({
                        "session_id": s.session_id.as_str(),
                        "start_time": s.start_time.to_rfc3339(),
                        "end_time": s.end_time.to_rfc3339(),
                        "model": s.model.to_string(),
                        "tokens": {
                            "input_tokens": s.tokens.input_tokens,
                            "output_tokens": s.tokens.output_tokens,
                            "cache_creation_tokens": s.tokens.cache_creation_tokens,
                            "cache_read_tokens": s.tokens.cache_read_tokens,
                            "total": s.tokens.total(),
                        },
                        "total_cost": s.total_cost,
                    })).collect::<Vec<_>>());
                } else {
                    block_json["sessions"] = json!(b.sessions.iter().map(|s| s.session_id.as_str()).collect::<Vec<_>>());
                }

                block_json
            }).collect::<Vec<_>>()
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
/// * `full_model_names` - If true, shows full model names; otherwise shows shortened versions
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
/// let formatter = get_formatter(false, false);
///
/// // Get JSON formatter for machine-readable output
/// let json_formatter = get_formatter(true, false);
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
pub fn get_formatter(json: bool, full_model_names: bool) -> Box<dyn OutputFormatter> {
    if json {
        Box::new(JsonFormatter)
    } else {
        Box::new(TableFormatter::new(full_model_names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregation::{MonthlyUsage, SessionBlock, SessionUsage};
    use crate::types::{DailyDate, ModelName, SessionId, TokenCounts};
    use chrono::{NaiveDate, TimeZone, Utc};

    #[test]
    fn test_number_formatting() {
        assert_eq!(TableFormatter::format_number(1234567), "1,234,567");
        assert_eq!(TableFormatter::format_number(999), "999");
        assert_eq!(TableFormatter::format_number(0), "0");
        assert_eq!(TableFormatter::format_number(1000000000), "1,000,000,000");
        assert_eq!(TableFormatter::format_number(42), "42");
    }

    #[test]
    fn test_currency_formatting() {
        assert_eq!(TableFormatter::format_currency(12.345), "$12.35");
        assert_eq!(TableFormatter::format_currency(0.0), "$0.00");
        assert_eq!(TableFormatter::format_currency(1000.0), "$1000.00");
        assert_eq!(TableFormatter::format_currency(0.001), "$0.00");
        assert_eq!(TableFormatter::format_currency(999999.99), "$999999.99");
    }

    #[test]
    fn test_get_formatter() {
        // Test JSON formatter
        let json_formatter = get_formatter(true, false);
        assert!(
            json_formatter
                .format_daily(&[], &Totals::default())
                .contains("\"daily\"")
        );

        // Test table formatter with full model names
        let table_formatter = get_formatter(false, true);
        let daily_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            tokens: TokenCounts::new(100, 50, 10, 5),
            total_cost: 1.25,
            models_used: vec!["claude-3-opus".to_string()],
            entries: None,
        }];
        let totals = Totals::from_daily(&daily_data);
        let output = table_formatter.format_daily(&daily_data, &totals);
        assert!(output.contains("2024-01-01"));
    }

    #[test]
    fn test_table_formatter_daily() {
        let formatter = TableFormatter::new(false);

        // Test with empty data
        let empty_totals = Totals::default();
        let empty_output = formatter.format_daily(&[], &empty_totals);
        assert!(empty_output.contains("TOTAL"));

        // Test with single day
        let daily_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
            tokens: TokenCounts::new(1000, 500, 100, 50),
            total_cost: 2.50,
            models_used: vec!["claude-3-opus".to_string(), "claude-3-sonnet".to_string()],
            entries: None,
        }];
        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);

        assert!(output.contains("2024-03-15"));
        assert!(output.contains("1,000"));
        assert!(output.contains("500"));
        assert!(output.contains("$2.50"));
        assert!(output.contains("TOTAL"));

        // Test with multiple days
        let multi_day_data = vec![
            DailyUsage {
                date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
                tokens: TokenCounts::new(1000, 500, 0, 0),
                total_cost: 1.50,
                models_used: vec!["claude-3-opus".to_string()],
                entries: None,
            },
            DailyUsage {
                date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()),
                tokens: TokenCounts::new(2000, 1000, 200, 100),
                total_cost: 3.00,
                models_used: vec!["claude-3-sonnet".to_string()],
                entries: None,
            },
        ];
        let multi_totals = Totals::from_daily(&multi_day_data);
        let multi_output = formatter.format_daily(&multi_day_data, &multi_totals);

        assert!(multi_output.contains("2024-03-15"));
        assert!(multi_output.contains("2024-03-16"));
        assert!(multi_output.contains("3,000")); // Total input tokens
    }

    #[test]
    fn test_table_formatter_daily_verbose() {
        let formatter = TableFormatter::new(false);

        // Create verbose entries
        let timestamp = Utc.with_ymd_and_hms(2024, 3, 15, 10, 30, 0).unwrap();
        let verbose_entry = crate::aggregation::VerboseEntry {
            timestamp,
            session_id: "test-session".to_string(),
            model: "claude-3-opus".to_string(),
            tokens: TokenCounts::new(100, 50, 10, 5),
            cost: 0.25,
        };

        let daily_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
            tokens: TokenCounts::new(100, 50, 10, 5),
            total_cost: 0.25,
            models_used: vec!["claude-3-opus".to_string()],
            entries: Some(vec![verbose_entry]),
        }];

        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);

        // Verbose mode should show detailed entries
        assert!(output.contains("=== 2024-03-15 ==="));
        assert!(output.contains("test-session"));
        assert!(output.contains("10:30:00"));
        assert!(output.contains("Day Total"));
        assert!(output.contains("OVERALL SUMMARY"));
    }

    #[test]
    fn test_table_formatter_daily_by_instance() {
        let formatter = TableFormatter::new(false);

        let instance_data = vec![
            DailyInstanceUsage {
                date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
                instance_id: "instance-1".to_string(),
                tokens: TokenCounts::new(1000, 500, 0, 0),
                total_cost: 1.50,
                models_used: vec!["claude-3-opus".to_string()],
            },
            DailyInstanceUsage {
                date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
                instance_id: "instance-2".to_string(),
                tokens: TokenCounts::new(2000, 1000, 100, 50),
                total_cost: 3.00,
                models_used: vec!["claude-3-sonnet".to_string()],
            },
        ];

        let totals = Totals::from_daily_instances(&instance_data);
        let output = formatter.format_daily_by_instance(&instance_data, &totals);

        assert!(output.contains("instance-1"));
        assert!(output.contains("instance-2"));
        assert!(output.contains("2024-03-15"));
        assert!(output.contains("3,000")); // Total input tokens
        assert!(output.contains("$4.50")); // Total cost
    }

    #[test]
    fn test_table_formatter_sessions() {
        let formatter = TableFormatter::new(false);
        let tz = chrono_tz::UTC;

        let start_time = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let end_time = Utc.with_ymd_and_hms(2024, 3, 15, 12, 30, 0).unwrap();

        let sessions = vec![SessionUsage {
            session_id: SessionId::new("session-123"),
            start_time,
            end_time,
            tokens: TokenCounts::new(5000, 2500, 500, 250),
            total_cost: 7.50,
            model: ModelName::new("claude-3-opus"),
        }];

        let totals = Totals::from_sessions(&sessions);
        let output = formatter.format_sessions(&sessions, &totals, &tz);

        assert!(output.contains("session-123"));
        assert!(output.contains("2h 30m")); // Duration
        assert!(output.contains("5,000")); // Input tokens
        assert!(output.contains("$7.50"));
        assert!(output.contains("Opus")); // Model name (shortened)
    }

    #[test]
    fn test_table_formatter_monthly() {
        let formatter = TableFormatter::new(true); // Test with full model names

        let monthly_data = vec![
            MonthlyUsage {
                month: "2024-01".to_string(),
                tokens: TokenCounts::new(100000, 50000, 10000, 5000),
                total_cost: 150.00,
                active_days: 15,
            },
            MonthlyUsage {
                month: "2024-02".to_string(),
                tokens: TokenCounts::new(200000, 100000, 20000, 10000),
                total_cost: 300.00,
                active_days: 20,
            },
        ];

        let totals = Totals::from_monthly(&monthly_data);
        let output = formatter.format_monthly(&monthly_data, &totals);

        assert!(output.contains("2024-01"));
        assert!(output.contains("2024-02"));
        assert!(output.contains("100,000"));
        assert!(output.contains("200,000"));
        assert!(output.contains("$450.00")); // Total cost
        assert!(output.contains("15")); // Active days
        assert!(output.contains("20"));
    }

    #[test]
    fn test_table_formatter_blocks() {
        let formatter = TableFormatter::new(false);
        let tz = chrono_tz::US::Eastern;

        // Use fixed time for deterministic testing
        let now = Utc.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();

        // Create sessions for the blocks
        let session1 = SessionUsage {
            session_id: SessionId::new("session-1"),
            start_time: now - chrono::Duration::hours(2),
            end_time: now - chrono::Duration::hours(1),
            tokens: TokenCounts::new(1500, 750, 150, 75),
            total_cost: 2.25,
            model: ModelName::new("claude-3-opus"),
        };

        let session2 = SessionUsage {
            session_id: SessionId::new("session-2"),
            start_time: now - chrono::Duration::hours(1),
            end_time: now,
            tokens: TokenCounts::new(1500, 750, 150, 75),
            total_cost: 2.25,
            model: ModelName::new("claude-3-sonnet"),
        };

        let session3 = SessionUsage {
            session_id: SessionId::new("session-3"),
            start_time: now - chrono::Duration::hours(10),
            end_time: now - chrono::Duration::hours(9),
            tokens: TokenCounts::new(1000, 500, 100, 50),
            total_cost: 1.50,
            model: ModelName::new("claude-3-haiku"),
        };

        let active_block = SessionBlock {
            start_time: now - chrono::Duration::hours(2),
            end_time: now + chrono::Duration::hours(3),
            is_active: true,
            sessions: vec![session1, session2],
            tokens: TokenCounts::new(3000, 1500, 300, 150),
            total_cost: 4.50,
            warning: None,
        };

        let expired_block = SessionBlock {
            start_time: now - chrono::Duration::hours(10),
            end_time: now - chrono::Duration::hours(5),
            is_active: false,
            sessions: vec![session3],
            tokens: TokenCounts::new(1000, 500, 100, 50),
            total_cost: 1.50,
            warning: None,
        };

        let blocks = vec![active_block, expired_block];
        let output = formatter.format_blocks_with_now(&blocks, &tz, now, false);

        assert!(output.contains("ACTIVE"));
        assert!(output.contains("Complete"));
        assert!(output.contains("3,000"));
        assert!(output.contains("1,000"));
        assert!(output.contains("$4.50"));
        assert!(output.contains("$1.50"));
        // Now we can reliably test time remaining with fixed timestamp
        assert!(output.contains("3h 0m")); // Active block has 3 hours remaining
    }

    #[test]
    fn test_json_formatter_daily() {
        let formatter = JsonFormatter;

        let daily_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
            tokens: TokenCounts::new(1000, 500, 100, 50),
            total_cost: 2.50,
            models_used: vec!["claude-3-opus".to_string()],
            entries: None,
        }];

        let totals = Totals::from_daily(&daily_data);
        let output = formatter.format_daily(&daily_data, &totals);

        // Parse JSON to verify structure
        let json: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");
        assert_eq!(json["daily"][0]["date"], "2024-03-15");
        assert_eq!(json["daily"][0]["tokens"]["input_tokens"], 1000);
        assert_eq!(json["daily"][0]["total_cost"], 2.5);
        assert_eq!(json["totals"]["total_cost"], 2.5);
    }

    #[test]
    fn test_json_formatter_daily_by_instance() {
        let formatter = JsonFormatter;

        let instance_data = vec![DailyInstanceUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap()),
            instance_id: "instance-1".to_string(),
            tokens: TokenCounts::new(1000, 500, 0, 0),
            total_cost: 1.50,
            models_used: vec!["claude-3-opus".to_string()],
        }];

        let totals = Totals::from_daily_instances(&instance_data);
        let output = formatter.format_daily_by_instance(&instance_data, &totals);

        let json: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");
        assert_eq!(json["daily_by_instance"][0]["instance_id"], "instance-1");
        assert_eq!(json["daily_by_instance"][0]["tokens"]["input_tokens"], 1000);
    }

    #[test]
    fn test_json_formatter_sessions() {
        let formatter = JsonFormatter;
        let tz = chrono_tz::UTC;

        let start_time = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let end_time = Utc.with_ymd_and_hms(2024, 3, 15, 12, 30, 0).unwrap();

        let sessions = vec![SessionUsage {
            session_id: SessionId::new("session-123"),
            start_time,
            end_time,
            tokens: TokenCounts::new(5000, 2500, 0, 0),
            total_cost: 7.50,
            model: ModelName::new("claude-3-opus"),
        }];

        let totals = Totals::from_sessions(&sessions);
        let output = formatter.format_sessions(&sessions, &totals, &tz);

        let json: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");
        assert_eq!(json["sessions"][0]["session_id"], "session-123");
        assert_eq!(json["sessions"][0]["duration_seconds"], 9000); // 2.5 hours
        assert_eq!(json["sessions"][0]["total_cost"], 7.5);
    }

    #[test]
    fn test_json_formatter_monthly() {
        let formatter = JsonFormatter;

        let monthly_data = vec![MonthlyUsage {
            month: "2024-01".to_string(),
            tokens: TokenCounts::new(100000, 50000, 0, 0),
            total_cost: 150.00,
            active_days: 15,
        }];

        let totals = Totals::from_monthly(&monthly_data);
        let output = formatter.format_monthly(&monthly_data, &totals);

        let json: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");
        assert_eq!(json["monthly"][0]["month"], "2024-01");
        assert_eq!(json["monthly"][0]["active_days"], 15);
        assert_eq!(json["totals"]["total_cost"], 150.0);
    }

    #[test]
    fn test_json_formatter_blocks() {
        let formatter = JsonFormatter;
        let tz = chrono_tz::UTC;

        // Use fixed time for deterministic testing
        let now = Utc.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();

        let session = SessionUsage {
            session_id: SessionId::new("session-1"),
            start_time: now - chrono::Duration::hours(2),
            end_time: now - chrono::Duration::hours(1),
            tokens: TokenCounts::new(3000, 1500, 0, 0),
            total_cost: 4.50,
            model: ModelName::new("claude-3-opus"),
        };

        let block = SessionBlock {
            start_time: now - chrono::Duration::hours(2),
            end_time: now + chrono::Duration::hours(3),
            is_active: true,
            sessions: vec![session],
            tokens: TokenCounts::new(3000, 1500, 0, 0),
            total_cost: 4.50,
            warning: None,
        };

        let blocks = vec![block];
        let output = formatter.format_blocks(&blocks, &tz, false);

        let json: serde_json::Value =
            serde_json::from_str(&output).expect("Failed to parse JSON output");
        assert_eq!(json["blocks"][0]["is_active"], true);
        assert_eq!(json["blocks"][0]["session_count"], 1);
        assert_eq!(json["blocks"][0]["total_cost"], 4.5);
    }

    #[test]
    fn test_datetime_formatting_with_timezone() {
        let utc_time = Utc.with_ymd_and_hms(2024, 3, 15, 15, 30, 0).unwrap();

        // Test with UTC
        let utc_formatted = TableFormatter::format_datetime_with_tz(&utc_time, &chrono_tz::UTC);
        assert!(utc_formatted.contains("2024-03-15 15:30"));
        assert!(utc_formatted.contains("UTC"));

        // Test with Eastern timezone
        let est_formatted =
            TableFormatter::format_datetime_with_tz(&utc_time, &chrono_tz::US::Eastern);
        // On 2024-03-15, US/Eastern is in EDT (UTC-4)
        assert!(est_formatted.contains("2024-03-15 11:30"));
        assert!(est_formatted.contains("EDT"));
    }

    #[test]
    fn test_edge_cases() {
        let formatter = TableFormatter::new(false);

        // Test with zero tokens
        let zero_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            tokens: TokenCounts::new(0, 0, 0, 0),
            total_cost: 0.0,
            models_used: vec![],
            entries: None,
        }];
        let zero_totals = Totals::from_daily(&zero_data);
        let zero_output = formatter.format_daily(&zero_data, &zero_totals);
        assert!(zero_output.contains("$0.00"));

        // Test with very large numbers
        let large_data = vec![DailyUsage {
            date: DailyDate::new(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            tokens: TokenCounts::new(999999999, 888888888, 777777777, 666666666),
            total_cost: 9999999.99,
            models_used: vec!["model".to_string()],
            entries: None,
        }];
        let large_totals = Totals::from_daily(&large_data);
        let large_output = formatter.format_daily(&large_data, &large_totals);
        assert!(large_output.contains("999,999,999"));
    }
}
