//! CLI error types.

use rw_comments::{CreateError, QuoteResolutionError, StoreError};
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

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    QuoteResolution(#[from] QuoteResolutionError),

    #[error("publish completed with {count} diagram warning(s); --strict was set")]
    DiagramWarningsInStrictMode { count: usize },
}

impl From<CreateError> for CliError {
    fn from(err: CreateError) -> Self {
        match err {
            CreateError::Store(e) => CliError::Store(e),
            CreateError::Quote(e) => CliError::QuoteResolution(e),
            e @ CreateError::BothQuoteAndSelectors => CliError::Validation(e.to_string()),
        }
    }
}

impl CliError {
    /// Exit code category:
    /// - `3` — validation / caller error (bad flags, ambiguous quote, etc.)
    /// - `2` — referenced entity does not exist
    /// - `1` — anything else
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            CliError::Validation(_)
            | CliError::Store(StoreError::InvalidParent(_))
            | CliError::QuoteResolution(
                QuoteResolutionError::NotFound { .. } | QuoteResolutionError::Ambiguous { .. },
            ) => 3,
            CliError::Store(StoreError::NotFound(_))
            | CliError::QuoteResolution(QuoteResolutionError::DocumentNotFound { .. }) => 2,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagram_warnings_in_strict_mode_exits_1() {
        // Pin the exit code so future changes to the catch-all surface here.
        let err = CliError::DiagramWarningsInStrictMode { count: 2 };
        assert_eq!(err.exit_code(), 1);
    }
}
