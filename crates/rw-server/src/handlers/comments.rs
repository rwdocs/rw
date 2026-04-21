//! Comments API endpoints.
//!
//! CRUD handlers for inline comments on documentation pages.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rw_comments::{
    Comment, CommentFilter, CreateError, NewComment, QuoteResolutionError, StoreError,
    UpdateComment,
};
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

/// HTTP error wrapper for the comments API.
///
/// Needed because axum's [`IntoResponse`] can't be implemented directly on the
/// upstream error types (orphan rule).
#[derive(Debug, thiserror::Error)]
pub(crate) enum CommentApiError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Create(#[from] CreateError),
}

impl CommentApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Store(err) | Self::Create(CreateError::Store(err)) => match err {
                StoreError::NotFound(_) => StatusCode::NOT_FOUND,
                StoreError::InvalidParent(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Create(CreateError::Quote(quote)) => match quote {
                QuoteResolutionError::DocumentNotFound { .. } => StatusCode::NOT_FOUND,
                QuoteResolutionError::NotFound { .. } | QuoteResolutionError::Ambiguous { .. } => {
                    StatusCode::BAD_REQUEST
                }
                QuoteResolutionError::RenderFailed { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Create(CreateError::BothQuoteAndSelectors) => StatusCode::BAD_REQUEST,
        }
    }
}

impl IntoResponse for CommentApiError {
    fn into_response(self) -> Response {
        (self.status_code(), Json(json!({"error": self.to_string()}))).into_response()
    }
}

/// Handle `GET /api/comments?documentId=...&status=...`.
pub(crate) async fn list_comments(
    State(state): State<Arc<AppState>>,
    Query(filter): Query<CommentFilter>,
) -> Result<Json<Vec<Comment>>, CommentApiError> {
    Ok(Json(state.comment_store.list(filter).await?))
}

/// Handle `POST /api/comments`.
pub(crate) async fn create_comment(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NewComment>,
) -> Result<(StatusCode, Json<Comment>), CommentApiError> {
    let comment = rw_comments::create_comment(&state.comment_store, &state.site, input).await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

/// Handle `GET /api/comments/{id}`.
pub(crate) async fn get_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Comment>, CommentApiError> {
    Ok(Json(state.comment_store.get(id).await?))
}

/// Handle `PATCH /api/comments/{id}`.
pub(crate) async fn update_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateComment>,
) -> Result<Json<Comment>, CommentApiError> {
    Ok(Json(state.comment_store.update(id, input).await?))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn status_code_mapping_covers_all_variants() {
        use rw_comments::QuoteResolutionError as Quote;

        let from_store = |e: StoreError| CommentApiError::Store(e).status_code();
        let from_create = |e: CreateError| CommentApiError::Create(e).status_code();
        let missing_doc = || "doc".to_owned();

        assert_eq!(
            from_store(StoreError::NotFound(Uuid::nil())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            from_store(StoreError::InvalidParent("bad parent".to_owned())),
            StatusCode::BAD_REQUEST,
        );
        assert_eq!(
            from_store(StoreError::CorruptStatus(
                "bogus".parse::<rw_comments::CommentStatus>().unwrap_err(),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        assert_eq!(
            from_create(CreateError::Store(StoreError::NotFound(Uuid::nil()))),
            StatusCode::NOT_FOUND,
        );
        assert_eq!(
            from_create(CreateError::Quote(Quote::DocumentNotFound {
                document_id: missing_doc(),
            })),
            StatusCode::NOT_FOUND,
        );
        assert_eq!(
            from_create(CreateError::Quote(Quote::NotFound {
                document_id: missing_doc(),
            })),
            StatusCode::BAD_REQUEST,
        );
        assert_eq!(
            from_create(CreateError::Quote(Quote::Ambiguous {
                document_id: missing_doc(),
                count: 3,
            })),
            StatusCode::BAD_REQUEST,
        );
        assert_eq!(
            from_create(CreateError::Quote(Quote::RenderFailed {
                document_id: missing_doc(),
                reason: "boom".to_owned(),
            })),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        assert_eq!(
            from_create(CreateError::BothQuoteAndSelectors),
            StatusCode::BAD_REQUEST,
        );
    }
}
