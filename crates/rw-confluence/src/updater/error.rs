//! Error types for page update operations.

use crate::error::ConfluenceError;

/// Error during page update operation.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Missing required configuration.
    #[error("{0}")]
    Config(String),

    /// Confluence API error.
    #[error("Confluence API error")]
    Confluence(#[from] ConfluenceError),

    /// IO error (file operations, temp directory).
    #[error("I/O error")]
    Io(#[from] std::io::Error),
}
