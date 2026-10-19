//! CLI error types.

use std::path::PathBuf;

use axoupdater::AxoupdateError;
use rw_comments::{CreateError, QuoteResolutionError, StoreError};
use rw_config::ConfigError;
use rw_server::ServerError;
use rw_storage::StorageError;

/// CLI error type.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("cannot read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    BundlePublish(#[from] rw_storage_s3::BundlePublishError),

    #[error(transparent)]
    Server(#[from] ServerError),

    #[error("{0}")]
    Validation(String),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    QuoteResolution(#[from] QuoteResolutionError),

    #[error("completed with {count} warning(s); --strict was set")]
    StrictWarnings { count: usize },

    #[error("--out - cannot stream {count} attachment(s); pass --out <dir> instead")]
    OutStdoutHasAttachments { count: usize },

    #[error(transparent)]
    Update(Box<AxoupdateError>),

    #[error(
        "rw wasn't installed with the rw installer, so it can't update itself — run `brew upgrade rw` if you used Homebrew, otherwise re-run the install script: https://github.com/rwdocs/rw#quickstart"
    )]
    CantSelfUpdate,
}

impl From<AxoupdateError> for CliError {
    fn from(err: AxoupdateError) -> Self {
        // Boxed to keep `CliError` (and every `Result<_, CliError>`) small —
        // `AxoupdateError` is large enough to trip clippy's `result_large_err`.
        CliError::Update(Box::new(err))
    }
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
            | CliError::Store(StoreError::InvalidParent(_) | StoreError::ReplyNotResolvable(_))
            | CliError::QuoteResolution(
                QuoteResolutionError::EmptyQuote
                | QuoteResolutionError::NotFound { .. }
                | QuoteResolutionError::Ambiguous { .. },
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

    use rw_error::error_chain;
    use rw_storage::StorageErrorKind;

    #[test]
    fn strict_warnings_exits_1() {
        let err = CliError::StrictWarnings { count: 2 };
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn out_stdout_has_attachments_exits_3() {
        let err = CliError::OutStdoutHasAttachments { count: 2 };
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("attachment(s)"));
        assert!(err.to_string().contains("--out -"));
    }

    #[test]
    fn cant_self_update_exits_1() {
        // Falls through to the `_ => 1` arm (not a validation/not-found bucket);
        // guard against a future exit_code arm reclassifying it.
        let err = CliError::CantSelfUpdate;
        assert_eq!(err.exit_code(), 1);
        let msg = err.to_string();
        assert!(msg.contains("brew upgrade"));
        assert!(msg.contains("https://github.com/rwdocs/rw#quickstart"));
    }

    #[derive(Debug, thiserror::Error)]
    #[error("dispatch failure")]
    struct DispatchFailure(#[source] std::io::Error);

    #[test]
    fn storage_error_message_names_every_cause() {
        let err = CliError::Storage(
            StorageError::new(StorageErrorKind::Unavailable)
                .with_backend("S3")
                .with_source(DispatchFailure(std::io::Error::other("connection refused"))),
        );

        assert_eq!(
            error_chain(&err),
            "[S3] Unavailable: dispatch failure: connection refused"
        );
    }

    #[test]
    fn incompatible_schema_exits_1() {
        // Raised when SqliteCommentStore::open() refuses a DB written by a newer
        // binary. Ensure a future exit_code arm doesn't change it to something
        // other than 1 (unexpected error).
        let err = CliError::Store(StoreError::IncompatibleSchema { db: 2, binary: 1 });
        assert_eq!(err.exit_code(), 1);
    }
}
