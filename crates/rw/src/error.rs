//! CLI error types.

use rw_config::ConfigError;
use rw_confluence::{ConfluenceError, UpdateError};
use rw_server::ServerError;

/// CLI error type.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Confluence(#[from] ConfluenceError),

    #[error(transparent)]
    Update(#[from] UpdateError),

    #[error(transparent)]
    BundlePublish(#[from] rw_storage_s3::BundlePublishError),

    #[error(transparent)]
    Server(#[from] ServerError),

    #[error("{0}")]
    Validation(String),
}
