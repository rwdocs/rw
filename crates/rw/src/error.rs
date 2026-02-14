//! CLI error types.

use rw_config::ConfigError;
use rw_confluence::{ConfluenceError, UpdateError};
use rw_techdocs::{BuildError, PublishError};

/// CLI error type.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error("{0}")]
    Config(#[from] ConfigError),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Confluence(#[from] ConfluenceError),

    #[error("{0}")]
    Update(#[from] UpdateError),

    #[error("{0}")]
    Build(#[from] BuildError),

    #[error("{0}")]
    Publish(#[from] PublishError),

    #[error("{0}")]
    Server(String),

    #[error("{0}")]
    Validation(String),
}
