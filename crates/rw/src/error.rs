//! CLI error types.

use rw_comments::{CreateError, QuoteResolutionError, StoreError};
use rw_config::ConfigError;
use rw_confluence::ConfluenceError;
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
    BundlePublish(#[from] rw_storage_s3::BundlePublishError),

    #[error("{0}")]
    Server(#[from] ServerError),

    #[error("{0}")]
    Validation(String),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    QuoteResolution(#[from] QuoteResolutionError),

    #[error("completed with {count} warning(s); --strict was set")]
    DiagramWarningsInStrictMode { count: usize },

    #[error("--out - cannot stream {count} attachment(s); pass --out <dir> instead")]
    OutStdoutHasAttachments { count: usize },
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
            | CliError::OutStdoutHasAttachments { .. }
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
        let err = CliError::DiagramWarningsInStrictMode { count: 2 };
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn out_stdout_has_attachments_exits_3() {
        let err = CliError::OutStdoutHasAttachments { count: 2 };
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("attachment(s)"));
        assert!(err.to_string().contains("--out -"));
    }
}
