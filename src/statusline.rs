//! Statusline module for Claude Code integration
//!
//! This module provides functionality to generate a single-line status
//! for Claude Code's statusline feature. It reads JSON input from stdin
//! and outputs a formatted status line with current model, session cost,
//! daily cost percentage, and remaining time in the billing block.

use crate::aggregation::Aggregator;
use crate::cost_calculator::CostCalculator;
use crate::data_loader::DataLoader;
use crate::error::Result;
use crate::pricing_fetcher::PricingFetcher;
use crate::timezone::TimezoneConfig;
use crate::types::{CostMode, SessionId, UsageEntry};
use chrono::{Datelike, Local, Utc};
use colored::*;
use futures::stream::StreamExt;
use serde::Deserialize;
use std::io::{self, Read};
use std::process::Command;
use std::sync::Arc;

/// Input structure from Claude Code
#[derive(Debug, Deserialize)]
pub struct StatuslineInput {
    pub session_id: String,
    pub model: ModelInfo,
    #[serde(default)]
    pub workspace: Option<WorkspaceInfo>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Model information from Claude Code
#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
}

/// Workspace information from Claude Code
#[derive(Debug, Deserialize)]
pub struct WorkspaceInfo {
    pub current_dir: Option<String>,
    pub project_dir: Option<String>,
}

/// Color configuration for statusline elements
struct ColorConfig {
    model: Color,
    cost: Color,
    separator: Color,
    muted: Color,
    date: Color,
    branch: Color,
    percent_good: Color,
    percent_warn: Color,
    percent_over: Color,
    percent_high: Color,
    time_normal: Color,
    time_warning: Color,
    time_error: Color,
}

impl ColorConfig {
    fn new() -> Self {
        Self {
            model: Color::TrueColor {
                r: 175,
                g: 135,
                b: 255,
            }, // Soft purple (141)
            cost: Color::TrueColor {
                r: 255,
                g: 175,
                b: 95,
            }, // Soft orange (215)
            separator: Color::TrueColor {
                r: 88,
                g: 88,
                b: 88,
            }, // Dark gray (240)
            muted: Color::TrueColor {
                r: 138,
                g: 138,
                b: 138,
            }, // Medium gray (245)
            date: Color::TrueColor {
                r: 189,
                g: 189,
                b: 189,
            }, // Light gray (250)
            branch: Color::TrueColor {
                r: 135,
                g: 215,
                b: 135,
            }, // Soft green (114)
            percent_good: Color::TrueColor {
                r: 135,
                g: 215,
                b: 135,
            }, // Green (114)
            percent_warn: Color::TrueColor {
                r: 255,
                g: 215,
                b: 95,
            }, // Yellow (221)
            percent_over: Color::TrueColor {
                r: 255,
                g: 175,
                b: 95,
            }, // Orange (215)
            percent_high: Color::TrueColor {
                r: 255,
                g: 135,
                b: 135,
            }, // Red (210)
            time_normal: Color::TrueColor {
                r: 95,
                g: 215,
                b: 255,
            }, // Soft cyan (117)
            time_warning: Color::TrueColor {
                r: 255,
                g: 215,
                b: 95,
            }, // Yellow (221)
            time_error: Color::TrueColor {
                r: 255,
                g: 135,
                b: 135,
            }, // Red (210)
        }
    }
}

/// Handler for statusline generation
pub struct StatuslineHandler {
    data_loader: Arc<DataLoader>,
    cost_calculator: Arc<CostCalculator>,
    aggregator: Arc<Aggregator>,
    monthly_fee: f64,
    no_color: bool,
    show_date: bool,
    show_git: bool,
    colors: ColorConfig,
}

impl StatuslineHandler {
    /// Create a new statusline handler
    pub async fn new(
        monthly_fee: f64,
        no_color: bool,
        show_date: bool,
        show_git: bool,
    ) -> Result<Self> {
        // Disable progress and quiet mode for statusline
        let data_loader = Arc::new(DataLoader::new().await?.with_progress(false));
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await); // quiet mode
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));
        let aggregator = Arc::new(
            Aggregator::new(cost_calculator.clone(), TimezoneConfig::default())
                .with_progress(false),
        );

        Ok(Self {
            data_loader,
            cost_calculator,
            aggregator,
            monthly_fee,
            no_color,
            show_date,
            show_git,
            colors: ColorConfig::new(),
        })
    }

    /// Read and parse JSON input from stdin
    pub fn read_input() -> Result<StatuslineInput> {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        let input: StatuslineInput = serde_json::from_str(&buffer)?;
        Ok(input)
    }

    /// Generate the statusline output
    pub async fn generate(&self, input: StatuslineInput) -> Result<String> {
        // Extract session ID
        let session_id = SessionId::new(input.session_id);

        // Get model display name (simplified)
        let model_display = self.format_model(&input.model.display_name);

        // Load all usage entries
        let entries_stream = self.data_loader.load_usage_entries();

        // Collect entries for processing
        let mut all_entries = Vec::new();
        let mut session_entries = Vec::new();
        let mut today_entries = Vec::new();

        let today = Local::now().date_naive();

        tokio::pin!(entries_stream);
        while let Some(result) = entries_stream.next().await {
            if let Ok(entry) = result {
                // Check if this is for our session
                if entry.session_id == session_id {
                    session_entries.push(entry.clone());
                }

                // Check if this is from today (in local timezone)
                let entry_date = entry.timestamp.inner().with_timezone(&Local).date_naive();
                if entry_date == today {
                    today_entries.push(entry.clone());
                }

                all_entries.push(entry);
            }
        }

        // Calculate session cost (handle case where session has no entries)
        let session_cost = if session_entries.is_empty() {
            0.0
        } else {
            self.calculate_session_cost(&session_entries).await?
        };

        // Calculate daily cost and percentage
        let (_daily_cost, daily_percentage) =
            self.calculate_daily_percentage(&today_entries).await?;

        // Calculate remaining time in billing block
        let remaining_time = self
            .calculate_remaining_time(&all_entries, &session_id)
            .await?;

        // Build status line components
        let mut components = Vec::new();

        // Add date/time if requested
        if self.show_date {
            let datetime = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            components.push(self.apply_color(&datetime, self.colors.date));
        }

        // Add git branch if requested
        if self.show_git {
            let branch = self.get_git_branch(&input.workspace);
            components.push(branch);
        }

        // Add model
        components.push(model_display);

        // Add session cost
        components.push(self.format_cost(session_cost));

        // Add daily percentage
        components.push(self.format_percentage(daily_percentage));

        // Add remaining time
        components.push(self.format_remaining_time(&remaining_time));

        // Join with separator
        let separator = self.apply_color(" | ", self.colors.separator);
        Ok(components.join(&separator))
    }

    /// Get git branch for current directory
    fn get_git_branch(&self, workspace: &Option<WorkspaceInfo>) -> String {
        let cwd = workspace
            .as_ref()
            .and_then(|w| w.current_dir.as_ref())
            .map(|s| s.as_str())
            .unwrap_or(".");

        let output = Command::new("git")
            .args(["symbolic-ref", "--short", "HEAD"])
            .current_dir(cwd)
            .output();

        let branch = match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => "no git".to_string(),
        };

        if branch == "no git" {
            self.apply_color(&branch, self.colors.muted)
        } else {
            self.apply_color(&branch, self.colors.branch)
        }
    }

    /// Format model name with color
    fn format_model(&self, display_name: &str) -> String {
        let name = Self::extract_model_name(display_name);
        self.apply_color(&name, self.colors.model)
    }

    /// Extract key part of model name (without emoji)
    fn extract_model_name(display_name: &str) -> String {
        // Extract key part of model name
        let name = if display_name.to_lowercase().contains("opus") {
            "Opus"
        } else if display_name.to_lowercase().contains("sonnet") {
            "Sonnet"
        } else if display_name.to_lowercase().contains("haiku") {
            "Haiku"
        } else {
            // Take the last significant part after "-" or use full name
            display_name.split('-').next_back().unwrap_or(display_name)
        };

        name.to_string()
    }

    /// Format cost with color
    fn format_cost(&self, cost: f64) -> String {
        let text = if cost > 0.0 {
            format!("${:.2}", cost)
        } else {
            "no session".to_string()
        };

        let color = if cost > 0.0 {
            self.colors.cost
        } else {
            self.colors.muted
        };

        self.apply_color(&text, color)
    }

    /// Format percentage with color based on thresholds
    fn format_percentage(&self, percentage: f64) -> String {
        let text = if percentage >= 0.0 {
            format!("{:.1}%", percentage)
        } else {
            "N/A".to_string()
        };

        let color = if percentage < 0.0 {
            self.colors.muted
        } else if percentage < 80.0 {
            self.colors.percent_good
        } else if percentage < 120.0 {
            self.colors.percent_warn
        } else if percentage < 150.0 {
            self.colors.percent_over
        } else {
            self.colors.percent_high
        };

        self.apply_color(&text, color)
    }

    /// Format remaining time with color
    fn format_remaining_time(&self, time_str: &str) -> String {
        // Parse the time string to determine color
        let (text, color) = if time_str == "No active block" {
            ("no block".to_string(), self.colors.muted)
        } else if time_str == "expired" {
            ("expired".to_string(), self.colors.time_error)
        } else if time_str.contains(':') {
            // Parse HH:MM:SS format and convert to human-readable
            let parts: Vec<&str> = time_str.split(':').collect();
            if parts.len() == 3 {
                let hours: i32 = parts[0].parse().unwrap_or(0);
                let minutes: i32 = parts[1].parse().unwrap_or(0);

                let formatted = if hours == 0 {
                    format!("{}m left", minutes)
                } else {
                    format!("{}h {}m left", hours, minutes)
                };

                let color = if hours > 0 {
                    self.colors.time_normal
                } else {
                    self.colors.time_warning
                };

                (formatted, color)
            } else {
                (time_str.to_string(), self.colors.muted)
            }
        } else {
            (time_str.to_string(), self.colors.muted)
        };

        self.apply_color(&text, color)
    }

    /// Apply color to text if colors are enabled
    fn apply_color(&self, text: &str, color: Color) -> String {
        if self.no_color {
            text.to_string()
        } else {
            text.color(color).to_string()
        }
    }

    /// Calculate cost for the current session
    async fn calculate_session_cost(&self, entries: &[UsageEntry]) -> Result<f64> {
        let mut total_cost = 0.0;

        for entry in entries {
            // Use calculate_with_mode to handle unknown models gracefully
            // Skip entries with unknown models when in Auto mode without pre-calculated cost
            let cost = match self
                .cost_calculator
                .calculate_with_mode(
                    &entry.tokens,
                    &entry.model,
                    entry.total_cost,
                    CostMode::Auto,
                )
                .await
            {
                Ok(c) => c,
                Err(crate::error::CcstatError::UnknownModel(_)) => {
                    // Skip unknown models, use pre-calculated cost if available
                    entry.total_cost.unwrap_or(0.0)
                }
                Err(e) => return Err(e),
            };
            total_cost += cost;
        }

        Ok(total_cost)
    }

    /// Calculate daily cost and percentage of monthly fee
    async fn calculate_daily_percentage(&self, entries: &[UsageEntry]) -> Result<(f64, f64)> {
        let mut daily_cost = 0.0;

        for entry in entries {
            // Use calculate_with_mode to handle unknown models gracefully
            // Skip entries with unknown models when in Auto mode without pre-calculated cost
            let cost = match self
                .cost_calculator
                .calculate_with_mode(
                    &entry.tokens,
                    &entry.model,
                    entry.total_cost,
                    CostMode::Auto,
                )
                .await
            {
                Ok(c) => c,
                Err(crate::error::CcstatError::UnknownModel(_)) => {
                    // Skip unknown models, use pre-calculated cost if available
                    entry.total_cost.unwrap_or(0.0)
                }
                Err(e) => return Err(e),
            };
            daily_cost += cost;
        }

        // Calculate days in current month
        let now = Local::now();
        let days_in_month = Self::days_in_month(now.year(), now.month());

        // Calculate daily budget (monthly fee / days in month)
        let daily_budget = self.monthly_fee / days_in_month as f64;

        // Calculate percentage
        let percentage = if daily_budget > 0.0 {
            (daily_cost / daily_budget) * 100.0
        } else {
            0.0
        };

        Ok((daily_cost, percentage))
    }

    /// Calculate remaining time in the current billing block
    async fn calculate_remaining_time(
        &self,
        entries: &[UsageEntry],
        session_id: &SessionId,
    ) -> Result<String> {
        // If no entries, return no active block
        if entries.is_empty() {
            return Ok("No active block".to_string());
        }

        // Convert entries to stream for aggregation
        let entries_stream = futures::stream::iter(entries.iter().cloned().map(Ok));

        // Aggregate sessions - this might fail for unknown models, so handle gracefully
        let sessions = match self
            .aggregator
            .aggregate_sessions(entries_stream, CostMode::Auto)
            .await
        {
            Ok(s) => s,
            Err(crate::error::CcstatError::UnknownModel(_)) => {
                // If aggregation fails due to unknown models, return no active block
                return Ok("No active block".to_string());
            }
            Err(e) => return Err(e),
        };

        // Create billing blocks
        let blocks = Aggregator::create_billing_blocks(&sessions);

        // Find active block containing our session
        let now = Utc::now();

        for block in blocks {
            if block.is_active {
                // Check if our session is in this block
                let has_session = block.sessions.iter().any(|s| s.session_id == *session_id);

                if has_session || (block.start_time <= now && now < block.end_time) {
                    // Calculate remaining time
                    let remaining = block.end_time - now;

                    if remaining.num_seconds() > 0 {
                        let hours = remaining.num_hours();
                        let minutes = (remaining.num_minutes() % 60).abs();
                        let seconds = (remaining.num_seconds() % 60).abs();

                        return Ok(format!("{:02}:{:02}:{:02}", hours, minutes, seconds));
                    } else {
                        return Ok("expired".to_string());
                    }
                }
            }
        }

        // No active block found
        Ok("No active block".to_string())
    }

    /// Get number of days in a month
    fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 30, // Default fallback
        }
    }
}

/// Run the statusline handler
pub async fn run(monthly_fee: f64, no_color: bool, show_date: bool, show_git: bool) -> Result<()> {
    // Disable colors if requested
    if no_color {
        colored::control::set_override(false);
    }

    // Read input from stdin
    let input = StatuslineHandler::read_input()?;

    // Create handler
    let handler = StatuslineHandler::new(monthly_fee, no_color, show_date, show_git).await?;

    // Generate and print statusline
    let output = handler.generate(input).await?;
    println!("{}", output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_name() {
        assert_eq!(
            StatuslineHandler::extract_model_name("Claude 3 Opus"),
            "Opus"
        );
        assert_eq!(
            StatuslineHandler::extract_model_name("Claude 3 Sonnet"),
            "Sonnet"
        );
        assert_eq!(
            StatuslineHandler::extract_model_name("claude-3-haiku"),
            "Haiku"
        );
        assert_eq!(
            StatuslineHandler::extract_model_name("some-other-model"),
            "model"
        );
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(StatuslineHandler::days_in_month(2024, 1), 31);
        assert_eq!(StatuslineHandler::days_in_month(2024, 2), 29); // Leap year
        assert_eq!(StatuslineHandler::days_in_month(2023, 2), 28); // Non-leap year
        assert_eq!(StatuslineHandler::days_in_month(2024, 4), 30);
        assert_eq!(StatuslineHandler::days_in_month(2024, 12), 31);
    }
}
