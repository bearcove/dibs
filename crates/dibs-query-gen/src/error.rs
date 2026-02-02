//! Query generation errors.

use std::fmt;

/// Error type for query generation.
#[derive(Debug, Clone)]
pub enum QueryGenError {
    /// Missing or invalid filter arguments.
    InvalidFilterArgs { filter: String, reason: String },
}

impl fmt::Display for QueryGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryGenError::InvalidFilterArgs { filter, reason } => {
                write!(f, "Invalid arguments for @{}: {}", filter, reason)
            }
        }
    }
}

impl std::error::Error for QueryGenError {}
