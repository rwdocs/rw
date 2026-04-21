use thiserror::Error;
use uuid::Uuid;

use crate::anchoring::QuoteResolutionError;
use crate::model::ParseCommentStatusError;

/// Errors returned by comment store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// No comment exists with the requested id.
    #[error("comment not found: {0}")]
    NotFound(Uuid),
    /// Parent id is missing, resolved, or belongs to a different document.
    #[error("invalid parent comment: {0}")]
    InvalidParent(String),
    /// Database operation failed.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// I/O error while preparing the store on disk.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON (de)serialization failed for a persisted column.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Stored UUID failed to parse.
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    /// Stored row contains an unknown comment status.
    #[error(transparent)]
    CorruptStatus(#[from] ParseCommentStatusError),
}

/// Errors returned by the high-level [`crate::create_comment`] flow.
#[derive(Debug, Error)]
pub enum CreateError {
    /// Underlying storage operation failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Resolving the supplied `quote` against the rendered document failed.
    #[error(transparent)]
    Quote(#[from] QuoteResolutionError),
    /// Caller supplied both `selectors` and `quote`; only one is allowed.
    #[error("quote and selectors are mutually exclusive")]
    BothQuoteAndSelectors,
}
