//! CLI error types.

use rw_config::ConfigError;
use rw_confluence::{ConfluenceError, UpdateError};
use rw_server::ServerError;

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
    BundlePublish(#[from] rw_storage_s3::BundlePublishError),

    #[error("{0}")]
    Server(#[from] ServerError),

    #[error("{0}")]
    Validation(String),
}
