//! Error types for ccstat
//!
//! This module defines the error types used throughout the ccstat library.
//! All errors are derived from `thiserror` for convenient error handling
//! and automatic `From` implementations.
//!
//! # Example
//!
//! ```
//! use ccstat::error::{CcstatError, Result};
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

    /// MCP server error
    #[error("MCP server error: {0}")]
    McpServer(String),

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
/// use ccstat::Result;
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

    #[test]
    fn test_json_error_conversion() {
        let json_str = "{invalid json}";
        let json_error = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let ccstat_error: CcstatError = json_error.into();

        assert!(matches!(ccstat_error, CcstatError::Json(_)));
        assert!(ccstat_error.to_string().contains("key must be a string"));
    }

    #[test]
    fn test_result_type_alias() {
        fn test_function() -> Result<i32> {
            Ok(42)
        }

        let result = test_function();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        fn test_error_function() -> Result<i32> {
            Err(CcstatError::NoClaudeDirectory)
        }

        let error_result = test_error_function();
        assert!(error_result.is_err());
    }

    #[test]
    fn test_unknown_model_error() {
        let model = ModelName::new("unknown-model");
        let error = CcstatError::UnknownModel(model);
        assert_eq!(error.to_string(), "Unknown model: unknown-model");
    }

    #[test]
    fn test_invalid_date_error() {
        let error = CcstatError::InvalidDate("bad date format".to_string());
        assert_eq!(error.to_string(), "Invalid date format: bad date format");
    }

    #[test]
    fn test_parse_error() {
        let error = CcstatError::Parse {
            file: PathBuf::from("/path/to/file.jsonl"),
            error: "invalid JSON".to_string(),
        };
        assert!(error.to_string().contains("/path/to/file.jsonl"));
        assert!(error.to_string().contains("invalid JSON"));
    }

    #[test]
    fn test_config_error() {
        let error = CcstatError::Config("missing required field".to_string());
        assert_eq!(error.to_string(), "Configuration error: missing required field");
    }

    #[test]
    fn test_invalid_argument_error() {
        let error = CcstatError::InvalidArgument("value must be positive".to_string());
        assert_eq!(error.to_string(), "Invalid argument: value must be positive");
    }

    #[test]
    fn test_mcp_server_error() {
        let error = CcstatError::McpServer("connection failed".to_string());
        assert_eq!(error.to_string(), "MCP server error: connection failed");
    }

    #[test]
    fn test_duplicate_entry_error() {
        let error = CcstatError::DuplicateEntry;
        assert_eq!(error.to_string(), "Duplicate entry");
    }
}
