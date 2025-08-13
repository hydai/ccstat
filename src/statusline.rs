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

/// Duration of a billing block in hours (as per Claude's billing model)
const BILLING_BLOCK_DURATION_HOURS: i64 = 5;

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
            self.calculate_remaining_time_optimized(start_time, &session_id, Utc::now())
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
        now: chrono::DateTime<Utc>,
    ) -> Result<RemainingTime> {
        // Determine the billing block for this session
        // Billing blocks are fixed-duration periods as per Claude's billing model
        let block_duration = Duration::hours(BILLING_BLOCK_DURATION_HOURS);

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
    use chrono::{TimeZone, Utc};

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
        assert_eq!(
            StatuslineHandler::extract_model_name("gpt-4-turbo"),
            "turbo"
        );
        assert_eq!(
            StatuslineHandler::extract_model_name("claude/opus-20240229"),
            "Opus" // Contains "opus" so returns "Opus"
        );
    }

    #[test]
    fn test_days_in_month() {
        // Test regular months
        assert_eq!(StatuslineHandler::days_in_month(2024, 1), 31);
        assert_eq!(StatuslineHandler::days_in_month(2024, 4), 30);
        assert_eq!(StatuslineHandler::days_in_month(2024, 12), 31);
        assert_eq!(StatuslineHandler::days_in_month(2024, 7), 31);
        assert_eq!(StatuslineHandler::days_in_month(2024, 9), 30);
        assert_eq!(StatuslineHandler::days_in_month(2024, 11), 30);

        // Test leap year calculation for February
        assert_eq!(StatuslineHandler::days_in_month(2024, 2), 29); // Leap year
        assert_eq!(StatuslineHandler::days_in_month(2023, 2), 28); // Non-leap year
        assert_eq!(StatuslineHandler::days_in_month(2000, 2), 29); // Divisible by 400
        assert_eq!(StatuslineHandler::days_in_month(1900, 2), 28); // Divisible by 100 but not 400
        assert_eq!(StatuslineHandler::days_in_month(2004, 2), 29); // Divisible by 4
        assert_eq!(StatuslineHandler::days_in_month(2100, 2), 28); // Divisible by 100 but not 400
    }

    #[test]
    fn test_truncate_to_hour() {
        let timestamp = Utc.with_ymd_and_hms(2024, 3, 15, 14, 37, 23).unwrap();
        let truncated = StatuslineHandler::truncate_to_hour(timestamp);

        assert_eq!(truncated.hour(), 14);
        assert_eq!(truncated.minute(), 0);
        assert_eq!(truncated.second(), 0);
        assert_eq!(truncated.nanosecond(), 0);

        // Test already truncated timestamp
        let already_truncated = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let result = StatuslineHandler::truncate_to_hour(already_truncated);
        assert_eq!(result, already_truncated);
    }

    /// Helper function to create StatuslineHandler for tests
    ///
    /// Creates a test handler with the specified configuration.
    /// Returns None if Claude directories are not found (for graceful test skipping).
    ///
    /// # Arguments
    /// * `monthly_fee` - Monthly subscription fee in USD
    /// * `no_color` - Whether to disable colored output
    /// * `show_date` - Whether to show date in statusline
    /// * `show_git` - Whether to show git branch in statusline
    async fn create_test_handler(
        monthly_fee: f64,
        no_color: bool,
        show_date: bool,
        show_git: bool,
    ) -> Option<StatuslineHandler> {
        match StatuslineHandler::new(monthly_fee, no_color, show_date, show_git).await {
            Ok(h) => Some(h),
            Err(_) => {
                println!("Skipping test: Unable to create handler");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_calculate_remaining_time_optimized() {
        // Create a handler for testing
        let handler = match create_test_handler(200.0, false, false, false).await {
            Some(h) => h,
            None => return,
        };

        let session_id = SessionId::new("test-session");
        // Use a fixed timestamp for deterministic testing
        let fixed_now = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();

        // Test active block (session started 2 hours ago)
        let session_start = fixed_now - chrono::Duration::hours(2);
        let remaining = handler
            .calculate_remaining_time_optimized(session_start, &session_id, fixed_now)
            .await
            .unwrap();

        match remaining {
            RemainingTime::TimeLeft(duration) => {
                // Session started at 12:30, block starts at 12:00 (truncated to hour)
                // Block ends at 17:00 (5 hours later)
                // Current time is 14:30, so 2.5 hours remain
                assert_eq!(duration.num_hours(), 2);
                assert_eq!(duration.num_minutes(), 150); // 2 hours 30 minutes
            }
            _ => panic!("Expected TimeLeft, got {:?}", remaining),
        }

        // Test expired block (session started 6 hours ago)
        let expired_start = fixed_now - chrono::Duration::hours(6);
        let expired_remaining = handler
            .calculate_remaining_time_optimized(expired_start, &session_id, fixed_now)
            .await
            .unwrap();

        match expired_remaining {
            RemainingTime::Expired => (),
            _ => panic!("Expected Expired, got {:?}", expired_remaining),
        }

        // Test edge case: session just started
        let just_started = fixed_now - chrono::Duration::minutes(5);
        let new_remaining = handler
            .calculate_remaining_time_optimized(just_started, &session_id, fixed_now)
            .await
            .unwrap();

        match new_remaining {
            RemainingTime::TimeLeft(duration) => {
                // Session started at 14:25, block starts at 14:00 (truncated to hour)
                // Block ends at 19:00 (5 hours later)
                // Current time is 14:30, so 4.5 hours remain
                assert_eq!(duration.num_hours(), 4);
                assert_eq!(duration.num_minutes(), 270); // 4 hours 30 minutes
            }
            _ => panic!("Expected TimeLeft for new session"),
        }
    }

    #[tokio::test]
    async fn test_statusline_handler_creation() {
        // Test with different configurations
        let configs = vec![
            (200.0, false, false, false),
            (100.0, true, false, false),
            (300.0, false, true, false),
            (150.0, false, false, true),
            (250.0, true, true, true),
        ];

        for (monthly_fee, no_color, show_date, show_git) in configs {
            match StatuslineHandler::new(monthly_fee, no_color, show_date, show_git).await {
                Ok(handler) => {
                    assert_eq!(handler.monthly_fee, monthly_fee);
                    assert_eq!(handler.no_color, no_color);
                    assert_eq!(handler.show_date, show_date);
                    assert_eq!(handler.show_git, show_git);
                }
                Err(_) => {
                    // This is expected in CI environments without Claude directories
                    println!("Handler creation failed (expected in CI)");
                }
            }
        }
    }

    #[test]
    fn test_format_model() {
        // We need to test the color application logic without actually creating the handler
        let _colors = ColorConfig::new();

        // Test model name extraction with various formats
        let test_cases = vec![
            ("Claude 3 Opus", "Opus"),
            ("Claude 3.5 Sonnet", "Sonnet"),
            ("claude-3-haiku-20240307", "Haiku"),
            ("gemini-pro", "pro"),
            ("gpt-4", "4"),
        ];

        for (input, expected) in test_cases {
            let result = StatuslineHandler::extract_model_name(input);
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_color_config() {
        let config = ColorConfig::new();

        // Verify color values are correctly set
        match config.model {
            Color::TrueColor { r, g, b } => {
                assert_eq!(r, 175);
                assert_eq!(g, 135);
                assert_eq!(b, 255);
            }
            _ => panic!("Expected TrueColor for model"),
        }

        match config.cost {
            Color::TrueColor { r, g, b } => {
                assert_eq!(r, 255);
                assert_eq!(g, 175);
                assert_eq!(b, 95);
            }
            _ => panic!("Expected TrueColor for cost"),
        }
    }

    #[test]
    fn test_remaining_time_enum() {
        // Test the RemainingTime enum variants
        let no_block = RemainingTime::NoActiveBlock;
        let expired = RemainingTime::Expired;
        let time_left = RemainingTime::TimeLeft(chrono::Duration::hours(3));

        // Test pattern matching
        match no_block {
            RemainingTime::NoActiveBlock => (),
            _ => panic!("Expected NoActiveBlock"),
        }

        match expired {
            RemainingTime::Expired => (),
            _ => panic!("Expected Expired"),
        }

        match time_left {
            RemainingTime::TimeLeft(d) => assert_eq!(d.num_hours(), 3),
            _ => panic!("Expected TimeLeft"),
        }
    }

    #[tokio::test]
    async fn test_format_cost() {
        let handler = match create_test_handler(200.0, true, false, false).await {
            Some(h) => h,
            None => return,
        };

        // Test with positive cost
        let cost_text = handler.format_cost(12.34);
        assert_eq!(cost_text, "$12.34");

        // Test with zero cost
        let zero_text = handler.format_cost(0.0);
        assert_eq!(zero_text, "no session");

        // Test with small cost
        let small_text = handler.format_cost(0.01);
        assert_eq!(small_text, "$0.01");

        // Test with large cost
        let large_text = handler.format_cost(999.99);
        assert_eq!(large_text, "$999.99");
    }

    #[tokio::test]
    async fn test_format_percentage() {
        let handler = match create_test_handler(200.0, true, false, false).await {
            Some(h) => h,
            None => return,
        };

        // Test various percentage thresholds
        assert_eq!(handler.format_percentage(50.0), "50.0%");
        assert_eq!(handler.format_percentage(79.9), "79.9%");
        assert_eq!(handler.format_percentage(80.0), "80.0%");
        assert_eq!(handler.format_percentage(119.9), "119.9%");
        assert_eq!(handler.format_percentage(120.0), "120.0%");
        assert_eq!(handler.format_percentage(149.9), "149.9%");
        assert_eq!(handler.format_percentage(150.0), "150.0%");
        assert_eq!(handler.format_percentage(200.0), "200.0%");
        assert_eq!(handler.format_percentage(-1.0), "N/A");
    }

    #[tokio::test]
    async fn test_format_remaining_time() {
        let handler = match create_test_handler(200.0, true, false, false).await {
            Some(h) => h,
            None => return,
        };

        // Test no active block
        let no_block = RemainingTime::NoActiveBlock;
        assert_eq!(handler.format_remaining_time(&no_block), "no block");

        // Test expired
        let expired = RemainingTime::Expired;
        assert_eq!(handler.format_remaining_time(&expired), "expired");

        // Test with hours and minutes
        let time_3h_30m = RemainingTime::TimeLeft(chrono::Duration::minutes(210));
        assert_eq!(handler.format_remaining_time(&time_3h_30m), "3h 30m left");

        // Test with only minutes
        let time_45m = RemainingTime::TimeLeft(chrono::Duration::minutes(45));
        assert_eq!(handler.format_remaining_time(&time_45m), "45m left");

        // Test with exactly 1 hour
        let time_1h = RemainingTime::TimeLeft(chrono::Duration::hours(1));
        assert_eq!(handler.format_remaining_time(&time_1h), "1h 0m left");
    }

    /// RAII guard for colored output override
    struct ColoredOverrideGuard {
        was_set: bool,
    }

    impl ColoredOverrideGuard {
        fn new(enable: bool) -> Self {
            colored::control::set_override(enable);
            Self { was_set: true }
        }
    }

    impl Drop for ColoredOverrideGuard {
        fn drop(&mut self) {
            if self.was_set {
                colored::control::unset_override();
            }
        }
    }

    #[tokio::test]
    async fn test_apply_color() {
        // Test with colors disabled
        let handler_no_color = match StatuslineHandler::new(200.0, true, false, false).await {
            Ok(h) => h,
            Err(_) => {
                println!("Skipping test: Unable to create handler");
                return;
            }
        };

        let text = "test text";
        let colored_text = handler_no_color.apply_color(text, Color::Red);
        assert_eq!(colored_text, text); // Should return plain text when no_color is true

        // Test with colors enabled
        // Force color output to ensure deterministic testing
        let _guard = ColoredOverrideGuard::new(true);

        let handler_color = match create_test_handler(200.0, false, false, false).await {
            Some(h) => h,
            None => {
                return;
            }
        };

        let colored_text_enabled = handler_color.apply_color(text, Color::Red);

        // Verify that ANSI color codes are present
        assert!(
            colored_text_enabled.contains('\x1b'),
            "The output string should contain ANSI color codes"
        );
        assert_ne!(
            colored_text_enabled, text,
            "The colored output should be different from the input text"
        );

        // Guard will automatically clean up when dropped
    }

    #[test]
    fn test_statusline_input_deserialization() {
        // Test basic input
        let json = r#"{
            "session_id": "test-123",
            "model": {
                "id": "claude-3-opus",
                "display_name": "Claude 3 Opus"
            }
        }"#;

        let input: StatuslineInput =
            serde_json::from_str(json).expect("Failed to deserialize StatuslineInput");
        assert_eq!(input.session_id, "test-123");
        assert_eq!(input.model.id, "claude-3-opus");
        assert_eq!(input.model.display_name, "Claude 3 Opus");
        assert!(input.workspace.is_none());
        assert!(input.transcript_path.is_none());
        assert!(input.cwd.is_none());

        // Test with workspace info
        let json_with_workspace = r#"{
            "session_id": "test-456",
            "model": {
                "id": "claude-3-sonnet",
                "display_name": "Claude 3 Sonnet"
            },
            "workspace": {
                "current_dir": "/home/user/project",
                "project_dir": "/home/user/project"
            },
            "cwd": "/home/user/project/src"
        }"#;

        let input_workspace: StatuslineInput = serde_json::from_str(json_with_workspace)
            .expect("Failed to deserialize StatuslineInput with workspace");
        assert_eq!(input_workspace.session_id, "test-456");
        assert!(input_workspace.workspace.is_some());
        assert_eq!(
            input_workspace
                .workspace
                .as_ref()
                .expect("Workspace should be present")
                .current_dir,
            Some("/home/user/project".to_string())
        );
        assert_eq!(
            input_workspace.cwd,
            Some("/home/user/project/src".to_string())
        );
    }

    #[test]
    fn test_workspace_info_deserialization() {
        let json = r#"{
            "current_dir": "/path/to/current",
            "project_dir": "/path/to/project"
        }"#;

        let workspace: WorkspaceInfo =
            serde_json::from_str(json).expect("Failed to deserialize WorkspaceInfo");
        assert_eq!(workspace.current_dir, Some("/path/to/current".to_string()));
        assert_eq!(workspace.project_dir, Some("/path/to/project".to_string()));

        // Test with null values
        let json_nulls = r#"{
            "current_dir": null,
            "project_dir": null
        }"#;

        let workspace_nulls: WorkspaceInfo = serde_json::from_str(json_nulls)
            .expect("Failed to deserialize WorkspaceInfo with nulls");
        assert!(workspace_nulls.current_dir.is_none());
        assert!(workspace_nulls.project_dir.is_none());
    }

    #[tokio::test]
    async fn test_read_input_invalid_json() {
        // This test would require mocking stdin, which is complex
        // Instead, we validate that the JSON parsing works correctly
        let invalid_json = "not json";
        let result: std::result::Result<StatuslineInput, serde_json::Error> =
            serde_json::from_str(invalid_json);
        assert!(result.is_err());

        let missing_fields = "{}";
        let result: std::result::Result<StatuslineInput, serde_json::Error> =
            serde_json::from_str(missing_fields);
        assert!(result.is_err());

        let partial_json = r#"{"session_id": "test"}"#;
        let result: std::result::Result<StatuslineInput, serde_json::Error> =
            serde_json::from_str(partial_json);
        assert!(result.is_err()); // Missing required model field
    }

    #[test]
    fn test_edge_cases_model_names() {
        // Test edge cases for model name extraction
        assert_eq!(StatuslineHandler::extract_model_name(""), "");
        assert_eq!(StatuslineHandler::extract_model_name("OPUS"), "Opus");
        assert_eq!(StatuslineHandler::extract_model_name("SoNnEt"), "Sonnet");
        assert_eq!(StatuslineHandler::extract_model_name("haiku"), "Haiku");
        assert_eq!(StatuslineHandler::extract_model_name("-"), "");
        assert_eq!(StatuslineHandler::extract_model_name("model-"), "");
        assert_eq!(StatuslineHandler::extract_model_name("/"), "");
        assert_eq!(StatuslineHandler::extract_model_name("a/b/c/d"), "d");
        assert_eq!(StatuslineHandler::extract_model_name("a-b-c-d"), "d");
    }
}
