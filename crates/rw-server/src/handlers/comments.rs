//! Comments API endpoints.
//!
//! CRUD handlers for inline comments on documentation pages.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rw_comments::{
    Comment, CommentFilter, CommentStatus, CreateError, NewComment, QuoteResolutionError,
    StoreError, UpdateComment,
};

use serde::{Deserialize, Serialize};
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
                StoreError::InvalidFilter(_) | StoreError::InvalidParent(_) => {
                    StatusCode::BAD_REQUEST
                }
                StoreError::Sqlx(_)
                | StoreError::Io(_)
                | StoreError::Json(_)
                | StoreError::Uuid(_)
                | StoreError::CorruptStatus(_)
                | StoreError::IncompatibleSchema { .. } => StatusCode::INTERNAL_SERVER_ERROR,
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

/// PATCH request body. Mirrors `rw_comments::UpdateComment`. `CommentStatus`
/// is now the narrowed `{Open, Resolved}` enum — serde rejects
/// `status: "deleted"` at the extractor (422); deletion goes through
/// `DELETE /_api/comments/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateCommentRequest {
    body: Option<String>,
    status: Option<CommentStatus>,
    selectors: Option<Vec<rw_comments::Selector>>,
}

impl From<UpdateCommentRequest> for UpdateComment {
    fn from(r: UpdateCommentRequest) -> Self {
        UpdateComment {
            body: r.body,
            status: r.status,
            selectors: r.selectors,
        }
    }
}

/// HTTP response shape for a comment — wraps `Comment` with server-only
/// permission/state flags. Snake-case Rust fields are projected to
/// `canDelete` / `canRestore` on the wire.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentResponse {
    #[serde(flatten)]
    comment: Comment,
    can_delete: bool,
    can_restore: bool,
}

impl CommentResponse {
    /// Project a `Comment` into the wire shape with derived `canDelete` /
    /// `canRestore` flags. In this model only replies are deletable
    /// (`parent_id IS NOT NULL`); top-level comments use Resolve. Restore is
    /// always allowed on any deleted row.
    fn project(comment: Comment) -> Self {
        let can_delete = comment.deleted_at.is_none() && comment.parent_id.is_some();
        let can_restore = comment.deleted_at.is_some();
        Self {
            comment,
            can_delete,
            can_restore,
        }
    }
}

/// Handle `GET /_api/comments?documentId=...&status=...`.
pub(crate) async fn list_comments(
    State(state): State<Arc<AppState>>,
    Query(filter): Query<CommentFilter>,
) -> Result<Json<Vec<CommentResponse>>, CommentApiError> {
    // `?status=deleted` is rejected by the Query extractor itself — `Deleted`
    // is no longer a `CommentStatus` variant, so axum returns 400 before this
    // handler runs.
    let rows = state.comment_store.list(filter).await?;
    Ok(Json(
        rows.into_iter().map(CommentResponse::project).collect(),
    ))
}

/// Handle `POST /_api/comments`.
pub(crate) async fn create_comment(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NewComment>,
) -> Result<(StatusCode, Json<CommentResponse>), CommentApiError> {
    let comment = rw_comments::create_comment(&state.comment_store, &state.site, input).await?;
    Ok((StatusCode::CREATED, Json(CommentResponse::project(comment))))
}

/// Handle `GET /_api/comments/{id}`.
pub(crate) async fn get_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommentResponse>, CommentApiError> {
    let c = state.comment_store.get(id).await?;
    Ok(Json(CommentResponse::project(c)))
}

/// Handle `PATCH /_api/comments/{id}`.
pub(crate) async fn update_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCommentRequest>,
) -> Result<Json<CommentResponse>, CommentApiError> {
    let updated = state.comment_store.update(id, input.into()).await?;
    Ok(Json(CommentResponse::project(updated)))
}

/// Handle `DELETE /_api/comments/{id}`.
pub(crate) async fn delete_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommentResponse>, CommentApiError> {
    let deleted = state.comment_store.delete_comment(id).await?;
    Ok(Json(CommentResponse::project(deleted)))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;
    use crate::testing::TestServer;

    /// Pull a UUID out of a freshly-created comment's JSON body.
    fn id_of(value: &serde_json::Value) -> Uuid {
        value["id"]
            .as_str()
            .unwrap_or_else(|| panic!("comment JSON missing `id`: {value}"))
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn delete_reply_returns_200_with_deleted_comment() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let reply = server.create_reply("a.md", id_of(&parent), "r").await;
        let resp = server
            .delete(&format!("/_api/comments/{}", id_of(&reply)))
            .await;
        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.json();
        assert!(
            body["deletedAt"].is_string(),
            "expected `deletedAt` to be a non-null ISO timestamp string, got: {body}",
        );
        assert_eq!(body["canDelete"], false);
        assert_eq!(body["canRestore"], true);
    }

    #[tokio::test]
    async fn delete_already_deleted_reply_is_idempotent_200() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let reply = server.create_reply("a.md", id_of(&parent), "r").await;
        let url = format!("/_api/comments/{}", id_of(&reply));
        let first = server.delete(&url).await;
        assert_eq!(first.status, StatusCode::OK);
        // Sleep so the wall clock would change if `updated_at` were re-bumped.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let second = server.delete(&url).await;
        assert_eq!(second.status, StatusCode::OK);
        assert_eq!(first.json()["updatedAt"], second.json()["updatedAt"]);
    }

    #[tokio::test]
    async fn delete_missing_is_404() {
        let server = TestServer::with_comments().await;
        let resp = server
            .delete(&format!("/_api/comments/{}", Uuid::new_v4()))
            .await;
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_top_level_is_404() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let resp = server
            .delete(&format!("/_api/comments/{}", id_of(&parent)))
            .await;
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn patch_status_open_on_deleted_reply_restores() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let reply = server.create_reply("a.md", id_of(&parent), "r").await;
        let id = id_of(&reply);
        let _ = server.delete(&format!("/_api/comments/{id}")).await;
        let resp = server
            .patch_json(
                &format!("/_api/comments/{id}"),
                serde_json::json!({"status": "open"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.json();
        assert_eq!(body["status"], "open");
        // Restore clears `deleted_at`; the field is `skip_serializing_if=None`,
        // so the wire shape drops it entirely on a live row.
        assert!(
            body.get("deletedAt").is_none() || body["deletedAt"].is_null(),
            "expected `deletedAt` to be absent or null after restore, got: {body}",
        );
        assert_eq!(body["canDelete"], true);
        assert_eq!(body["canRestore"], false);
    }

    #[tokio::test]
    async fn patch_status_deleted_is_rejected_by_serde() {
        let server = TestServer::with_comments().await;
        let created = server.create_comment("a.md", "body").await;
        let resp = server
            .patch_json(
                &format!("/_api/comments/{}", id_of(&created)),
                serde_json::json!({"status": "deleted"}),
            )
            .await;
        // `"deleted"` is not a valid `UpdateStatus` variant, so axum's `Json`
        // extractor fails. axum 0.8 maps semantic JSON validation failures
        // (valid syntax, rejected by serde) to 422 Unprocessable Entity;
        // syntax errors map to 400 Bad Request. Either is a 4xx client error
        // and never reaches the handler, which is what matters for the
        // contract: the wire never accepts `status: "deleted"` on PATCH.
        assert!(
            resp.status.is_client_error(),
            "expected 4xx, got {}: {}",
            resp.status,
            resp.text(),
        );
        assert_eq!(resp.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn patch_status_resolved_on_deleted_reply_is_404() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let reply = server.create_reply("a.md", id_of(&parent), "r").await;
        let id = id_of(&reply);
        let _ = server.delete(&format!("/_api/comments/{id}")).await;
        let resp = server
            .patch_json(
                &format!("/_api/comments/{id}"),
                serde_json::json!({"status": "resolved"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_deleted_reply_is_404() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "p").await;
        let reply = server.create_reply("a.md", id_of(&parent), "r").await;
        let id = id_of(&reply);
        let _ = server.delete(&format!("/_api/comments/{id}")).await;
        let resp = server.get(&format!("/_api/comments/{id}")).await;
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_status_deleted_is_rejected_by_serde() {
        let server = TestServer::with_comments().await;
        let resp = server.get("/_api/comments?status=deleted").await;
        // `"deleted"` is not a valid `CommentStatus` variant, so axum's `Query`
        // extractor rejects the request before the handler runs. axum 0.8 maps
        // these to 400 Bad Request (vs. 422 for JSON body validation). Either
        // way it must be a 4xx, and the wire contract — `status=deleted` is
        // not an addressable filter — is what matters.
        assert!(
            resp.status.is_client_error(),
            "expected 4xx, got {}: {}",
            resp.status,
            resp.text(),
        );
    }

    #[tokio::test]
    async fn list_returns_can_delete_and_can_restore_in_response() {
        let server = TestServer::with_comments().await;
        let parent = server.create_comment("a.md", "body").await;
        let _ = server.create_reply("a.md", id_of(&parent), "r").await;
        let resp = server.get("/_api/comments?documentId=a.md").await;
        assert_eq!(resp.status, StatusCode::OK);
        let body = resp.json();
        let arr = body.as_array().expect("response is a JSON array");
        assert_eq!(arr.len(), 2, "expected two comments, got: {body}");

        // Parent is top-level → not deletable in this model.
        let parent_row = arr
            .iter()
            .find(|r| r["body"] == "body")
            .expect("parent row");
        assert!(
            parent_row.get("canDelete").is_some(),
            "wire field canDelete missing: {parent_row}"
        );
        assert!(
            parent_row.get("canRestore").is_some(),
            "wire field canRestore missing: {parent_row}"
        );
        assert_eq!(parent_row["canDelete"], false);
        assert_eq!(parent_row["canRestore"], false);

        // Reply is deletable.
        let reply_row = arr.iter().find(|r| r["body"] == "r").expect("reply row");
        assert_eq!(reply_row["canDelete"], true);
        assert_eq!(reply_row["canRestore"], false);

        // camelCase projection must not leak snake_case versions.
        assert!(
            parent_row.get("can_delete").is_none(),
            "snake_case must not leak: {parent_row}"
        );
        assert!(
            parent_row.get("can_restore").is_none(),
            "snake_case must not leak: {parent_row}"
        );
    }

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
            from_store(StoreError::InvalidFilter("bad filter".to_owned())),
            StatusCode::BAD_REQUEST,
        );
        assert_eq!(
            from_store(StoreError::IncompatibleSchema { db: 99, binary: 1 }),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        assert_eq!(
            from_store(StoreError::CorruptStatus(
                "bogus".parse::<rw_comments::CommentStatus>().unwrap_err(),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        // Raised only at store-open time, before any handler runs; handlers
        // never see it in production. Ensure a future arm doesn't change the
        // status code if IncompatibleSchema is ever plumbed through a handler.
        assert_eq!(
            from_store(StoreError::IncompatibleSchema { db: 2, binary: 1 }),
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

    #[test]
    fn comment_response_predicates() {
        use rw_comments::{Author, CommentStatus, Selector};

        fn comment(
            status: CommentStatus,
            parent_id: Option<Uuid>,
            deleted_at: Option<String>,
        ) -> Comment {
            Comment {
                id: Uuid::nil(),
                document_id: "a.md".into(),
                parent_id,
                author: Author::local_human(),
                body: "x".into(),
                selectors: Vec::<Selector>::new(),
                status,
                created_at: "t".into(),
                updated_at: "t".into(),
                deleted_at,
            }
        }

        // Top-level open → not deletable (top-level uses Resolve), not restorable.
        let r = CommentResponse::project(comment(CommentStatus::Open, None, None));
        assert!(!r.can_delete);
        assert!(!r.can_restore);

        // Reply open → deletable, not restorable.
        let r = CommentResponse::project(comment(CommentStatus::Open, Some(Uuid::new_v4()), None));
        assert!(r.can_delete);
        assert!(!r.can_restore);

        // Deleted reply → not deletable (already deleted), restorable.
        // `status` is whatever it was when the row was deleted (typically Open).
        let r = CommentResponse::project(comment(
            CommentStatus::Open,
            Some(Uuid::new_v4()),
            Some("t".into()),
        ));
        assert!(!r.can_delete);
        assert!(r.can_restore);

        // Top-level resolved → not deletable, not restorable.
        let r = CommentResponse::project(comment(CommentStatus::Resolved, None, None));
        assert!(!r.can_delete);
        assert!(!r.can_restore);

        // Wire shape uses camelCase
        let r = CommentResponse::project(comment(CommentStatus::Open, Some(Uuid::new_v4()), None));
        let json = serde_json::to_value(&r).unwrap();
        assert!(
            json.get("canDelete").is_some(),
            "wire field must be camelCase: {json}"
        );
        assert!(
            json.get("canRestore").is_some(),
            "wire field must be camelCase: {json}"
        );
        assert!(
            json.get("can_delete").is_none(),
            "snake_case must not leak: {json}"
        );
        assert!(
            json.get("can_restore").is_none(),
            "snake_case can_restore must not leak"
        );
    }
}
