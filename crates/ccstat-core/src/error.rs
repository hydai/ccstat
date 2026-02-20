//! Error types for ccstat
//!
//! This module defines the error types used throughout the ccstat library.
//! All errors are derived from `thiserror` for convenient error handling
//! and automatic `From` implementations.
//!
//! # Example
//!
//! ```
//! use ccstat_core::error::{CcstatError, Result};
//!
//! fn example_function() -> Result<()> {
//!     // This will automatically convert io::Error to CcstatError
//!     let _file = std::fs::read_to_string("nonexistent.txt")?;
//!     Ok(())
//! }
//! ```

use std::path::PathBuf;
use thiserror::Error;

use crate::types::ModelName;

/// Main error type for ccstat operations
///
/// This enum encompasses all possible errors that can occur during
/// ccstat operations, from IO errors to parsing failures and network issues.
#[derive(Error, Debug)]
pub enum CcstatError {
    /// IO error occurred
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    /// No Claude data directories found
    #[error("No Claude data directories found")]
    NoClaudeDirectory,

    /// Unknown model encountered
    #[error("Unknown model: {0}")]
    UnknownModel(ModelName),

    /// Invalid date format
    #[error("Invalid date format: {0}")]
    InvalidDate(String),

    /// Invalid timezone
    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),

    /// Invalid token limit
    #[error("Invalid token limit: {0}")]
    InvalidTokenLimit(String),

    /// Parse error with file context
    #[error("Parse error in {file}: {error}")]
    Parse {
        /// The file that caused the error
        file: PathBuf,
        /// The error message
        error: String,
    },

    /// Network error
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid argument
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Duplicate entry found
    #[error("Duplicate entry")]
    DuplicateEntry,
}

/// Convenience type alias for Results in ccstat
///
/// This type alias makes it easier to work with Results throughout
/// the codebase by providing a default error type.
///
/// # Example
///
/// ```
/// use ccstat_core::Result;
///
/// fn process_data() -> Result<String> {
///     Ok("Processed successfully".to_string())
/// }
/// ```
pub type Result<T> = std::result::Result<T, CcstatError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = CcstatError::NoClaudeDirectory;
        assert_eq!(error.to_string(), "No Claude data directories found");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ccstat_error: CcstatError = io_error.into();
        assert!(matches!(ccstat_error, CcstatError::Io(_)));
    }
}
