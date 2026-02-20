//! Enhanced live monitoring display for billing blocks
//!
//! This module provides a modern, informative terminal UI for monitoring
//! active billing blocks with progress bars, burn rate calculations,
//! and usage projections.

use ccstat_core::aggregation_types::SessionBlock;
use ccstat_core::model_formatter::format_model_name;
use chrono::{DateTime, Duration, Utc};
use colored::*;
use std::fmt;

/// Box drawing characters for UI (ASCII)
const BOX_TOP_LEFT: &str = "+";
const BOX_TOP_RIGHT: &str = "+";
const BOX_BOTTOM_LEFT: &str = "+";
const BOX_BOTTOM_RIGHT: &str = "+";
const BOX_HORIZONTAL: &str = "-";
const BOX_VERTICAL: &str = "|";
const BOX_T_LEFT: &str = "+";
const BOX_T_RIGHT: &str = "+";

/// Progress bar characters (ASCII)
const PROGRESS_FULL: &str = "#";
const PROGRESS_EMPTY: &str = ".";

/// Default maximum historical cost for blocks monitoring (in USD)
pub const DEFAULT_MAX_COST: f64 = 50.0;

/// Threshold for exceeding projection limit (percentage)
const PROJECTION_EXCEED_THRESHOLD: f64 = 100.0;

/// Threshold for approaching projection limit (percentage)
const PROJECTION_APPROACHING_THRESHOLD: f64 = 80.0;

/// Enhanced display for billing blocks
pub struct BlocksMonitor {
    width: usize,
    timezone: chrono_tz::Tz,
    max_historical_cost: f64,
    /// Whether to use colored output (respects NO_COLOR environment variable)
    colored_output: bool,
}

impl BlocksMonitor {
    /// Create a new blocks monitor
    pub fn new(timezone: chrono_tz::Tz, max_historical_cost: Option<f64>) -> Self {
        // Get terminal width or use default
        // Use minimum width of 60 to support smaller terminals, but allow smaller if needed
        let raw_width = terminal_width().unwrap_or(100);
        let width = if raw_width < 60 {
            raw_width
        } else {
            raw_width.clamp(60, 120)
        };
        // Use provided max cost or default to a reasonable value
        let max_historical_cost = max_historical_cost.unwrap_or(DEFAULT_MAX_COST);
        // Check NO_COLOR environment variable for accessibility
        let colored_output = std::env::var("NO_COLOR").is_err();
        Self {
            width,
            timezone,
            max_historical_cost,
            colored_output,
        }
    }

    /// Render the active block with enhanced UI
    pub fn render_active_block(&self, block: &SessionBlock, now: DateTime<Utc>) -> String {
        let mut output = String::new();

        // Calculate metrics
        let elapsed = now - block.start_time;
        let remaining = block.end_time - now;
        let block_duration = block.end_time - block.start_time;
        let block_progress = if block_duration.num_seconds() > 0 {
            (elapsed.num_seconds() as f64 / block_duration.num_seconds() as f64) * 100.0
        } else {
            0.0
        };

        // Calculate burn rate and projection based on cost
        // Note: For blocks with less than one minute elapsed, burn rate may be overestimated.
        // To avoid this, use seconds for very short elapsed times.
        let current_cost = block.total_cost;
        let elapsed_minutes = elapsed.num_minutes();
        let burn_rate = if elapsed_minutes < 1 && elapsed.num_seconds() > 0 {
            // Use seconds for more accurate burn rate when elapsed time is less than a minute
            current_cost / (elapsed.num_seconds() as f64 / 60.0)
        } else {
            current_cost / (elapsed_minutes.max(1) as f64)
        };
        let remaining_minutes = remaining.num_minutes().max(0) as f64;
        let projected_cost = current_cost + (burn_rate * remaining_minutes);

        // Usage percentage based on cost
        let usage_percentage = if self.max_historical_cost > 0.0 {
            (current_cost / self.max_historical_cost) * 100.0
        } else {
            0.0
        };
        // Fixed projection formula: (Current Cost + Burn Rate * Remaining time) / Maximum block Cost * 100%
        let projection_percentage = if self.max_historical_cost > 0.0 {
            (projected_cost / self.max_historical_cost) * 100.0
        } else {
            0.0
        };

        // Determine status color
        let status_color = if projection_percentage > PROJECTION_EXCEED_THRESHOLD {
            "red"
        } else if projection_percentage > PROJECTION_APPROACHING_THRESHOLD {
            "yellow"
        } else {
            "green"
        };

        // Build the UI
        output.push_str(&self.draw_box_top());
        output.push_str(&self.draw_title());
        output.push_str(&self.draw_separator());
        output.push('\n');

        // Block progress section
        output.push_str(&self.draw_block_progress(
            block_progress,
            block.start_time,
            elapsed,
            remaining,
            block.end_time,
        ));
        output.push('\n');

        // Usage section
        output.push_str(&self.draw_usage_section(
            usage_percentage,
            current_cost,
            burn_rate,
            block.tokens.total() as f64,
            status_color,
        ));
        output.push('\n');

        // Projection section
        output.push_str(&self.draw_projection_section(
            projection_percentage,
            projected_cost,
            status_color,
        ));
        output.push('\n');

        // Info section
        output.push_str(&self.draw_info_section(block));

        output.push_str(&self.draw_separator());
        output.push_str(&self.draw_footer());
        output.push_str(&self.draw_box_bottom());

        output
    }

    /// Draw the top of the box
    fn draw_box_top(&self) -> String {
        format!(
            "{}{}{}",
            BOX_TOP_LEFT,
            BOX_HORIZONTAL.repeat(self.width - 2),
            BOX_TOP_RIGHT
        )
    }

    /// Draw the bottom of the box
    fn draw_box_bottom(&self) -> String {
        format!(
            "\n{}{}{}",
            BOX_BOTTOM_LEFT,
            BOX_HORIZONTAL.repeat(self.width - 2),
            BOX_BOTTOM_RIGHT
        )
    }

    /// Draw a separator line
    fn draw_separator(&self) -> String {
        format!(
            "\n{}{}{}",
            BOX_T_LEFT,
            BOX_HORIZONTAL.repeat(self.width - 2),
            BOX_T_RIGHT
        )
    }

    /// Draw the title
    fn draw_title(&self) -> String {
        let title = "CCSTAT - LIVE BILLING BLOCK MONITOR";
        self.draw_centered_line(title)
    }

    /// Draw the footer
    fn draw_footer(&self) -> String {
        let footer = "Refreshing every 5s - Press Ctrl+C to stop";
        self.draw_centered_line(footer)
    }

    /// Draw a centered line within the box
    fn draw_centered_line(&self, text: &str) -> String {
        let text_width = console::measure_text_width(text);
        let available_width = self.width.saturating_sub(2);
        if text_width >= available_width {
            // Text is too long, just use it as-is
            return format!("\n{} {} {}", BOX_VERTICAL, text, BOX_VERTICAL);
        }
        let padding = (available_width - text_width) / 2;
        let left_pad = " ".repeat(padding);
        let right_pad = " ".repeat(available_width - padding - text_width);
        format!(
            "\n{}{}{}{}{}",
            BOX_VERTICAL, left_pad, text, right_pad, BOX_VERTICAL
        )
    }

    /// Draw a left-aligned line with padding
    fn draw_line(&self, content: &str) -> String {
        let available_width = self.width.saturating_sub(4); // Account for "| " and " |"
        let truncated_content = console::truncate_str(content, available_width, "...");
        let final_width = console::measure_text_width(&truncated_content);

        let padding = available_width.saturating_sub(final_width);
        format!(
            "\n{} {}{} {}",
            BOX_VERTICAL,
            truncated_content,
            " ".repeat(padding),
            BOX_VERTICAL
        )
    }

    /// Draw block progress section
    fn draw_block_progress(
        &self,
        progress: f64,
        start_time: DateTime<Utc>,
        elapsed: Duration,
        remaining: Duration,
        end_time: DateTime<Utc>,
    ) -> String {
        let mut output = String::new();

        // Progress bar
        let bar = self.create_progress_bar(progress, 40);
        let progress_line = format!("TIME         {}  {:5.1}%", bar, progress.min(999.9));
        output.push_str(&self.draw_line(&progress_line));

        // Time details
        let start_str = start_time.with_timezone(&self.timezone).format("%H:%M:%S");
        let end_str = end_time.with_timezone(&self.timezone).format("%H:%M:%S");
        let elapsed_str = self.format_duration(elapsed);
        let remaining_str = if remaining.num_seconds() > 0 {
            self.format_duration(remaining)
        } else {
            "Expired".to_string()
        };

        let time_line = format!(
            "   Started: {}  Elapsed: {}  Remaining: {} ({})",
            start_str, elapsed_str, remaining_str, end_str
        );
        output.push_str(&self.draw_line(&time_line));

        output
    }

    /// Draw usage section
    fn draw_usage_section(
        &self,
        usage_percentage: f64,
        current_cost: f64,
        burn_rate: f64,
        total_tokens: f64,
        status_color: &str,
    ) -> String {
        let mut output = String::new();

        // Usage progress bar based on cost
        let bar = self.create_colored_progress_bar(usage_percentage, 40, status_color);
        // Use fixed width for cost values to prevent layout issues
        let usage_line = format!(
            "USAGE        {}  {:5.1}% (${:8.2}/${:8.2})",
            bar,
            usage_percentage.min(999.9),
            current_cost.min(99999.99),
            self.max_historical_cost.min(99999.99)
        );
        output.push_str(&self.draw_line(&usage_line));

        // Burn rate and tokens
        let burn_status = self.get_burn_status(burn_rate);
        let detail_line = format!(
            "   Cost: ${:8.2}  (Burn: ${:.3}/min {})  Tokens: {}",
            current_cost.min(99999.99),
            burn_rate.min(999.999),
            burn_status,
            self.format_number(total_tokens as u64)
        );
        output.push_str(&self.draw_line(&detail_line));

        output
    }

    /// Draw projection section
    fn draw_projection_section(
        &self,
        projection_percentage: f64,
        projected_cost: f64,
        status_color: &str,
    ) -> String {
        let mut output = String::new();

        // Projection bar based on cost
        let bar = self.create_colored_progress_bar(projection_percentage, 40, status_color);
        // Use fixed width for cost values to prevent layout issues
        let projection_line = format!(
            "PROJECTION   {}  {:5.1}% (${:8.2}/${:8.2})",
            bar,
            projection_percentage.min(999.9),
            projected_cost.min(99999.99),
            self.max_historical_cost.min(99999.99)
        );
        output.push_str(&self.draw_line(&projection_line));

        // Status
        let (status_text, color) = if projection_percentage > PROJECTION_EXCEED_THRESHOLD {
            ("WILL EXCEED LIMIT", "red")
        } else if projection_percentage > PROJECTION_APPROACHING_THRESHOLD {
            ("APPROACHING LIMIT", "yellow")
        } else {
            ("WITHIN LIMITS", "green")
        };

        let status = if self.colored_output {
            match color {
                "red" => status_text.red().to_string(),
                "yellow" => status_text.yellow().to_string(),
                _ => status_text.green().to_string(),
            }
        } else {
            status_text.to_string()
        };

        let status_line = format!(
            "   Status: {}  Projected Cost: ${:8.2}",
            status,
            projected_cost.min(99999.99)
        );
        output.push_str(&self.draw_line(&status_line));

        output
    }

    /// Draw info section
    fn draw_info_section(&self, block: &SessionBlock) -> String {
        let models = if block.models_used.is_empty() {
            "None".to_string()
        } else {
            // Format model names to short version
            block
                .models_used
                .iter()
                .map(|m| format_model_name(m, false))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let projects = if block.projects_used.is_empty() {
            "Default".to_string()
        } else {
            format!("{}", block.projects_used.len())
        };

        let info_line = format!(
            "Models: {}  Sessions: {}  Projects: {}",
            models,
            block.sessions.len(),
            projects
        );
        self.draw_line(&info_line)
    }

    /// Create a progress bar
    fn create_progress_bar(&self, percentage: f64, width: usize) -> String {
        let clamped_percentage = percentage.clamp(0.0, 100.0);
        let filled = ((clamped_percentage / 100.0) * width as f64) as usize;
        let filled = filled.min(width); // Ensure we don't exceed width
        let empty = width.saturating_sub(filled);
        format!(
            "[{}{}]",
            PROGRESS_FULL.repeat(filled),
            PROGRESS_EMPTY.repeat(empty)
        )
    }

    /// Create a colored progress bar
    fn create_colored_progress_bar(&self, percentage: f64, width: usize, color: &str) -> String {
        // Allow percentage to go over 100% for projections
        let display_percentage = if percentage > 100.0 {
            // Show filled bar for over 100%
            100.0
        } else {
            percentage
        };
        let bar = self.create_progress_bar(display_percentage, width);

        // Return plain bar if colors are disabled
        if !self.colored_output {
            return bar;
        }

        match color {
            "red" => bar.red().to_string(),
            "yellow" => bar.yellow().to_string(),
            _ => bar.green().to_string(),
        }
    }

    /// Get burn rate status (based on cost per minute)
    fn get_burn_status(&self, burn_rate: f64) -> String {
        const HIGH_BURN_RATE_THRESHOLD: f64 = 0.5;
        const ELEVATED_BURN_RATE_THRESHOLD: f64 = 0.2;

        let status_text = if burn_rate > HIGH_BURN_RATE_THRESHOLD {
            "HIGH"
        } else if burn_rate > ELEVATED_BURN_RATE_THRESHOLD {
            "ELEVATED"
        } else {
            "NORMAL"
        };

        // Return plain text if colors are disabled
        if !self.colored_output {
            return status_text.to_string();
        }

        // Apply colors based on status
        if burn_rate > HIGH_BURN_RATE_THRESHOLD {
            status_text.red().to_string()
        } else if burn_rate > ELEVATED_BURN_RATE_THRESHOLD {
            status_text.yellow().to_string()
        } else {
            status_text.green().to_string()
        }
    }

    /// Format a duration
    fn format_duration(&self, duration: Duration) -> String {
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;
        format!("{}h {}m", hours, minutes)
    }

    /// Format a number with thousands separator
    fn format_number(&self, num: u64) -> String {
        let s = num.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    }
}

/// Get terminal width using the cross-platform terminal_size crate
fn terminal_width() -> Option<usize> {
    terminal_size::terminal_size().map(|(width, _)| width.0 as usize)
}

impl fmt::Display for BlocksMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlocksMonitor(width: {})", self.width)
    }
}
