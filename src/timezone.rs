//! Timezone utilities for date handling
//!
//! This module provides functionality for detecting the system's local timezone
//! and parsing timezone strings from user input.

use chrono_tz::Tz;
use std::str::FromStr;
use tracing::debug;

/// Configuration for timezone handling
#[derive(Debug, Clone)]
pub struct TimezoneConfig {
    /// The timezone to use for date operations
    pub tz: Tz,
    /// Whether the timezone is UTC
    pub is_utc: bool,
}

impl Default for TimezoneConfig {
    fn default() -> Self {
        let tz = get_local_timezone();
        Self {
            is_utc: tz == Tz::UTC,
            tz,
        }
    }
}

impl TimezoneConfig {
    /// Create a new timezone configuration from CLI arguments
    pub fn from_cli(timezone_str: Option<&str>, use_utc: bool) -> crate::error::Result<Self> {
        if use_utc {
            return Ok(Self {
                tz: Tz::UTC,
                is_utc: true,
            });
        }

        if let Some(tz_str) = timezone_str {
            let tz = Tz::from_str(tz_str).map_err(|_| {
                crate::error::CcstatError::InvalidTimezone(format!(
                    "'{}'. Use format like 'America/New_York', 'Asia/Tokyo', or 'UTC'",
                    tz_str
                ))
            })?;
            Ok(Self {
                tz,
                is_utc: tz == Tz::UTC,
            })
        } else {
            Ok(Self::default())
        }
    }

    /// Get the display name for the configured timezone
    pub fn display_name(&self) -> &str {
        if self.is_utc { "UTC" } else { self.tz.name() }
    }
}

/// Detect the system's local timezone
///
/// This function attempts to detect the local timezone from the system.
/// If detection fails, it falls back to UTC.
pub fn get_local_timezone() -> Tz {
    // Try to get the timezone from the TZ environment variable first
    // Note: We use nested if let instead of let_chains for stable Rust compatibility
    #[allow(clippy::collapsible_if)]
    if let Ok(tz_str) = std::env::var("TZ") {
        if let Ok(tz) = Tz::from_str(&tz_str) {
            debug!("Using timezone from TZ environment variable: {}", tz_str);
            return tz;
        }
    }

    // The `iana-time-zone` crate provides a robust cross-platform way to get the system timezone
    match iana_time_zone::get_timezone() {
        Ok(tz_str) => match Tz::from_str(&tz_str) {
            Ok(tz) => {
                debug!("Using system timezone from iana-time-zone: {}", tz_str);
                tz
            }
            Err(_) => {
                debug!(
                    "Could not parse timezone from iana-time-zone: '{}', falling back to UTC",
                    tz_str
                );
                Tz::UTC
            }
        },
        Err(e) => {
            debug!(
                "Could not detect local timezone via iana-time-zone: {:?}, falling back to UTC",
                e
            );
            Tz::UTC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_config_utc() {
        let config = TimezoneConfig::from_cli(None, true).unwrap();
        assert!(config.is_utc);
        assert_eq!(config.tz, Tz::UTC);
        assert_eq!(config.display_name(), "UTC");
    }

    #[test]
    fn test_timezone_config_explicit() {
        let config = TimezoneConfig::from_cli(Some("America/New_York"), false).unwrap();
        assert!(!config.is_utc);
        assert_eq!(config.tz.name(), "America/New_York");
    }

    #[test]
    fn test_timezone_config_invalid() {
        let result = TimezoneConfig::from_cli(Some("Invalid/Timezone"), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_timezone_config_utc_via_timezone_flag() {
        // Test that specifying UTC via --timezone=UTC correctly sets is_utc to true
        let config = TimezoneConfig::from_cli(Some("UTC"), false).unwrap();
        assert!(config.is_utc);
        assert_eq!(config.tz, Tz::UTC);
        assert_eq!(config.display_name(), "UTC");
    }
}
