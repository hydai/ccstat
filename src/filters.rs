//! Filtering module for usage entries
//!
//! This module provides flexible filtering capabilities for usage data,
//! supporting date ranges, project names, and month-based filtering.
//!
//! # Examples
//!
//! ```
//! use ccstat::filters::UsageFilter;
//! use chrono::NaiveDate;
//!
//! // Create a filter for January 2024
//! let filter = UsageFilter::new()
//!     .with_since(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
//!     .with_until(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap())
//!     .with_project("my-project".to_string());
//! ```

use crate::types::UsageEntry;
use chrono::{Datelike, NaiveDate};

/// Filter configuration for usage entries
///
/// Supports filtering by date range and project name. All filters are optional
/// and can be combined for more specific queries.
#[derive(Debug, Default, Clone)]
pub struct UsageFilter {
    /// Start date filter (inclusive)
    pub since_date: Option<NaiveDate>,
    /// End date filter (inclusive)
    pub until_date: Option<NaiveDate>,
    /// Project name filter
    pub project: Option<String>,
}

impl UsageFilter {
    /// Create a new filter with no restrictions
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the start date filter
    pub fn with_since(mut self, date: NaiveDate) -> Self {
        self.since_date = Some(date);
        self
    }

    /// Set the end date filter
    pub fn with_until(mut self, date: NaiveDate) -> Self {
        self.until_date = Some(date);
        self
    }

    /// Set the project filter
    pub fn with_project(mut self, project: String) -> Self {
        self.project = Some(project);
        self
    }

    /// Check if an entry passes the filter
    pub fn matches(&self, entry: &UsageEntry) -> bool {
        // Check date filters
        let daily_date = entry.timestamp.to_daily_date();
        let entry_date = daily_date.inner();

        if let Some(since) = &self.since_date {
            if entry_date < since {
                return false;
            }
        }

        if let Some(until) = &self.until_date {
            if entry_date > until {
                return false;
            }
        }

        // Check project filter
        if let Some(project_filter) = &self.project {
            if let Some(entry_project) = &entry.project {
                if entry_project != project_filter {
                    return false;
                }
            } else {
                // If filter specifies a project but entry has no project, don't match
                return false;
            }
        }

        true
    }

    /// Filter a stream of entries
    ///
    /// Applies the configured filters to a stream of usage entries,
    /// returning only those that match all criteria.
    ///
    /// # Arguments
    ///
    /// * `stream` - An async stream of usage entries to filter
    ///
    /// # Returns
    ///
    /// A filtered stream containing only matching entries
    pub async fn filter_stream<S>(
        self,
        stream: S,
    ) -> impl futures::Stream<Item = crate::error::Result<UsageEntry>>
    where
        S: futures::Stream<Item = crate::error::Result<UsageEntry>>,
    {
        use futures::StreamExt;

        stream.filter_map(move |result| {
            let filter = self.clone();
            async move {
                match result {
                    Ok(entry) => {
                        if filter.matches(&entry) {
                            Some(Ok(entry))
                        } else {
                            None
                        }
                    }
                    Err(e) => Some(Err(e)),
                }
            }
        })
    }
}

/// Month filter for monthly aggregation
///
/// Provides filtering by year and month for monthly usage reports.
/// Useful for generating reports for specific months or month ranges.
///
/// # Example
///
/// ```
/// use ccstat::filters::MonthFilter;
///
/// // Filter for Q1 2024
/// let filter = MonthFilter::new()
///     .with_since(2024, 1)
///     .with_until(2024, 3);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MonthFilter {
    /// Start month (year and month)
    pub since: Option<(i32, u32)>,
    /// End month (year and month)
    pub until: Option<(i32, u32)>,
}

impl MonthFilter {
    /// Create a new month filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the start month
    pub fn with_since(mut self, year: i32, month: u32) -> Self {
        self.since = Some((year, month));
        self
    }

    /// Set the end month
    pub fn with_until(mut self, year: i32, month: u32) -> Self {
        self.until = Some((year, month));
        self
    }

    /// Check if a date falls within the month filter
    pub fn matches_date(&self, date: &NaiveDate) -> bool {
        let year = date.year();
        let month = date.month();

        if let Some((since_year, since_month)) = self.since {
            if year < since_year || (year == since_year && month < since_month) {
                return false;
            }
        }

        if let Some((until_year, until_month)) = self.until {
            if year > until_year || (year == until_year && month > until_month) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelName, SessionId, TokenCounts};
    use crate::ISOTimestamp;
    use chrono::{DateTime, Utc};

    #[test]
    fn test_date_filter() {
        let filter = UsageFilter::new()
            .with_since(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
            .with_until(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());

        // Create test entries
        let entry_before = UsageEntry {
            session_id: SessionId::new("test1"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2023-12-31T23:59:59Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        let entry_within = UsageEntry {
            session_id: SessionId::new("test2"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-01-15T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        let entry_after = UsageEntry {
            session_id: SessionId::new("test3"),
            timestamp: ISOTimestamp::new(
                DateTime::parse_from_rfc3339("2024-02-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        assert!(!filter.matches(&entry_before));
        assert!(filter.matches(&entry_within));
        assert!(!filter.matches(&entry_after));
    }

    #[test]
    fn test_month_filter() {
        let filter = MonthFilter::new().with_since(2024, 1).with_until(2024, 3);

        assert!(!filter.matches_date(&NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()));
        assert!(filter.matches_date(&NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()));
        assert!(filter.matches_date(&NaiveDate::from_ymd_opt(2024, 2, 15).unwrap()));
        assert!(filter.matches_date(&NaiveDate::from_ymd_opt(2024, 3, 31).unwrap()));
        assert!(!filter.matches_date(&NaiveDate::from_ymd_opt(2024, 4, 1).unwrap()));
    }

    #[test]
    fn test_project_filter() {
        let filter = UsageFilter::new().with_project("my-project".to_string());

        let entry_with_project = UsageEntry {
            session_id: SessionId::new("test1"),
            timestamp: ISOTimestamp::new(DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&Utc)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: Some("my-project".to_string()),
            instance_id: None,
        };

        let entry_different_project = UsageEntry {
            session_id: SessionId::new("test2"),
            timestamp: ISOTimestamp::new(DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&Utc)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: Some("other-project".to_string()),
            instance_id: None,
        };

        let entry_no_project = UsageEntry {
            session_id: SessionId::new("test3"),
            timestamp: ISOTimestamp::new(DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&Utc)),
            model: ModelName::new("claude-3-opus"),
            tokens: TokenCounts::default(),
            total_cost: None,
            project: None,
            instance_id: None,
        };

        assert!(filter.matches(&entry_with_project));
        assert!(!filter.matches(&entry_different_project));
        assert!(!filter.matches(&entry_no_project));
    }
}
