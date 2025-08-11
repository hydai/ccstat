//! Statusline module for Claude Code integration
//!
//! This module provides functionality to generate a single-line status
//! for Claude Code's statusline feature. It reads JSON input from stdin
//! and outputs a formatted status line with current model, session cost,
//! daily cost percentage, and remaining time in the billing block.

use crate::cost_calculator::CostCalculator;
use crate::data_loader::DataLoader;
use crate::error::Result;
use crate::pricing_fetcher::PricingFetcher;
use crate::types::{CostMode, SessionId};
use chrono::{Datelike, Duration, Local, TimeZone, Timelike, Utc};
use colored::*;
use futures::stream::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{self, AsyncReadExt};
use tokio::process::Command;
use tokio::time::timeout;

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

/// Remaining time information
#[derive(Debug, Clone)]
enum RemainingTime {
    NoActiveBlock,
    Expired,
    TimeLeft(Duration),
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
        let pricing_fetcher = Arc::new(PricingFetcher::new(true).await); // offline mode
        let cost_calculator = Arc::new(CostCalculator::new(pricing_fetcher));

        Ok(Self {
            data_loader,
            cost_calculator,
            monthly_fee,
            no_color,
            show_date,
            show_git,
            colors: ColorConfig::new(),
        })
    }

    /// Read and parse JSON input from stdin
    pub async fn read_input() -> Result<StatuslineInput> {
        // Check if stdin is a terminal (TTY)
        if is_terminal::is_terminal(std::io::stdin()) {
            return Err(crate::error::CcstatError::InvalidArgument(
                "The statusline command expects JSON input from stdin.\n\
                 It is designed to be called by Claude Code, not run interactively.\n\
                 \n\
                 Example usage:\n\
                 echo '{\"session_id\": \"test\", \"model\": {\"id\": \"claude-3-opus\", \"display_name\": \"Claude 3 Opus\"}}' | ccstat statusline"
                    .to_string(),
            ));
        }

        // Read with timeout to prevent indefinite hanging
        const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

        let mut buffer = String::new();
        timeout(READ_TIMEOUT, io::stdin().read_to_string(&mut buffer))
            .await
            .map_err(|_| {
                crate::error::CcstatError::InvalidArgument(
                    "Timeout waiting for input. The statusline command expects JSON input from stdin."
                        .to_string(),
                )
            })??;

        let input: StatuslineInput = serde_json::from_str(&buffer)?;
        Ok(input)
    }

    /// Generate the statusline output
    pub async fn generate(&self, input: StatuslineInput) -> Result<String> {
        // Extract session ID
        let session_id = SessionId::new(input.session_id.clone());

        // Get model display name (simplified)
        let model_display = self.format_model(&input.model.display_name);

        // PERFORMANCE OPTIMIZATION: Only load data from today for statusline
        // This significantly improves performance for users with large history
        // by only loading JSONL files that have been modified today or later.

        let today = Local::now().date_naive();
        let mut session_cost = 0.0;
        let mut daily_cost = 0.0;
        let mut session_start_time: Option<chrono::DateTime<Utc>> = None;

        // Only load entries from files modified since start of today (in UTC)
        // This filters out old files at the filesystem level before parsing
        let today_start_local = today.and_hms_opt(0, 0, 0).ok_or_else(|| {
            crate::error::CcstatError::InvalidDate("Invalid time components".to_string())
        })?;

        let today_start_utc = Local
            .from_local_datetime(&today_start_local)
            .single()
            .ok_or_else(|| {
                crate::error::CcstatError::InvalidDate(
                    "Could not determine local start of day (DST ambiguity)".to_string(),
                )
            })?
            .with_timezone(&Utc);

        // Process stream of recent entries only
        let entries_stream = self.data_loader.load_recent_usage_entries(today_start_utc);
        tokio::pin!(entries_stream);

        while let Some(result) = entries_stream.next().await {
            if let Ok(entry) = result {
                let entry_date = entry.timestamp.inner().with_timezone(&Local).date_naive();

                // Calculate cost for this entry
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
                        entry.total_cost.unwrap_or(0.0)
                    }
                    Err(e) => return Err(e),
                };

                // Accumulate session cost and track start time
                if entry.session_id == session_id {
                    if session_start_time.is_none()
                        || entry.timestamp.inner() < &session_start_time.unwrap()
                    {
                        session_start_time = Some(*entry.timestamp.inner());
                    }
                    session_cost += cost;
                }

                // Accumulate daily cost
                if entry_date == today {
                    daily_cost += cost;
                }
            }
        }

        // Calculate daily percentage
        let now = Local::now();
        let days_in_month = Self::days_in_month(now.year(), now.month());
        let daily_budget = self.monthly_fee / days_in_month as f64;
        let daily_percentage = if daily_budget > 0.0 {
            (daily_cost / daily_budget) * 100.0
        } else {
            0.0
        };

        // Calculate remaining time in billing block (optimized)
        let remaining_time = if let Some(start_time) = session_start_time {
            self.calculate_remaining_time_optimized(start_time, &session_id)
                .await?
        } else {
            RemainingTime::NoActiveBlock
        };

        // Build status line components
        let mut components = Vec::new();

        // Add date/time if requested
        if self.show_date {
            let datetime = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            components.push(self.apply_color(&datetime, self.colors.date));
        }

        // Add git branch if requested
        if self.show_git {
            let branch = self.get_git_branch(&input).await;
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
    async fn get_git_branch(&self, input: &StatuslineInput) -> String {
        // Fix: Check both workspace.current_dir and input.cwd
        let cwd = input
            .workspace
            .as_ref()
            .and_then(|w| w.current_dir.as_ref())
            .or(input.cwd.as_ref())
            .map(|s| s.as_str())
            .unwrap_or(".");

        let output = Command::new("git")
            .args(["symbolic-ref", "--short", "HEAD"])
            .current_dir(cwd)
            .output()
            .await;

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
        let lower_name = display_name.to_lowercase();
        let name = if lower_name.contains("opus") {
            "Opus"
        } else if lower_name.contains("sonnet") {
            "Sonnet"
        } else if lower_name.contains("haiku") {
            "Haiku"
        } else {
            // Take the last significant part after "-" or "/" or use full name
            display_name
                .split(&['-', '/'][..])
                .next_back()
                .unwrap_or(display_name)
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
    fn format_remaining_time(&self, remaining: &RemainingTime) -> String {
        // Use structured data instead of parsing strings
        let (text, color) = match remaining {
            RemainingTime::NoActiveBlock => ("no block".to_string(), self.colors.muted),
            RemainingTime::Expired => ("expired".to_string(), self.colors.time_error),
            RemainingTime::TimeLeft(duration) => {
                let hours = duration.num_hours();
                let minutes = (duration.num_minutes() % 60).abs();

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
            }
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

    /// Truncate a timestamp to the hour boundary (XX:00:00)
    fn truncate_to_hour(timestamp: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
        timestamp
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .expect("truncating to hour should always be valid")
    }

    /// Calculate remaining time in the current billing block (optimized)
    async fn calculate_remaining_time_optimized(
        &self,
        session_start_time: chrono::DateTime<Utc>,
        _session_id: &SessionId,
    ) -> Result<RemainingTime> {
        // Determine the billing block for this session
        // Billing blocks are 5-hour periods as per Claude's billing model
        let now = Utc::now();
        let block_duration = Duration::hours(5);

        // Align block start to hour boundary (XX:00) similar to aggregation.rs
        let block_start = Self::truncate_to_hour(session_start_time);
        let block_end = block_start + block_duration;

        // Check if we're still in the active block
        if now >= block_end {
            return Ok(RemainingTime::Expired);
        }

        if now < block_start {
            // Shouldn't happen, but handle gracefully
            return Ok(RemainingTime::NoActiveBlock);
        }

        // Calculate remaining time
        let remaining = block_end - now;
        if remaining.num_seconds() > 0 {
            Ok(RemainingTime::TimeLeft(remaining))
        } else {
            Ok(RemainingTime::Expired)
        }
    }

    /// Get number of days in a month
    fn days_in_month(year: i32, month: u32) -> u32 {
        use chrono::{Datelike, NaiveDate};

        // To get the number of days in a month, we find the first day of the next month
        // and then get the day of the previous day
        let (next_year, next_month) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };

        NaiveDate::from_ymd_opt(next_year, next_month, 1)
            .unwrap()
            .pred_opt()
            .unwrap()
            .day()
    }
}

/// Run the statusline handler
pub async fn run(monthly_fee: f64, no_color: bool, show_date: bool, show_git: bool) -> Result<()> {
    // Disable colors if requested
    if no_color {
        colored::control::set_override(false);
    }

    // Read input from stdin
    let input = StatuslineHandler::read_input().await?;

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
