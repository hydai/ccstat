//! Error types for ccusage

use std::path::PathBuf;
use thiserror::Error;

use crate::types::ModelName;

/// Main error type for ccusage operations
#[derive(Error, Debug)]
pub enum CcusageError {
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
}

/// Convenience type alias for Results in ccusage
pub type Result<T> = std::result::Result<T, CcusageError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = CcusageError::NoClaudeDirectory;
        assert_eq!(error.to_string(), "No Claude data directories found");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ccusage_error: CcusageError = io_error.into();
        assert!(matches!(ccusage_error, CcusageError::Io(_)));
    }
}
