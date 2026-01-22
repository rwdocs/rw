//! CLI error types.

use docstage_config::ConfigError;
use docstage_confluence::ConfluenceError;
use docstage_confluence::updater::UpdateError;

/// CLI error type.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Config(#[from] ConfigError),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Confluence(#[from] ConfluenceError),

    #[error("{0}")]
    Update(#[from] UpdateError),

    #[error("{0}")]
    Server(String),

    #[error("{0}")]
    Validation(String),
}
