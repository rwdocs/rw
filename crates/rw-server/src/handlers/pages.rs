//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use rw_renderer::TocEntry;
use rw_site::BreadcrumbItem;
use serde::Serialize;

use crate::error::ServerError;
use crate::handlers::to_url_path;
use crate::state::AppState;

/// Response for GET /api/pages/{path}.
#[derive(Serialize)]
struct PageResponse {
    /// Page metadata.
    meta: PageMeta,
    /// Breadcrumb navigation items.
    breadcrumbs: Vec<BreadcrumbResponse>,
    /// Table of contents entries.
    toc: Vec<TocResponse>,
    /// Rendered HTML content.
    content: String,
}

/// Page metadata.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageMeta {
    /// Page title (from H1 heading or metadata).
    title: Option<String>,
    /// URL path.
    path: String,
    /// Source file path.
    source_file: String,
    /// Last modification time (ISO 8601).
    last_modified: String,
    /// Page description (from metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Page type (from metadata, indicates a section).
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    page_type: Option<String>,
    /// Custom variables (from metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    vars: Option<serde_json::Value>,
}

/// Breadcrumb item for serialization.
#[derive(Serialize)]
struct BreadcrumbResponse {
    /// Display title.
    title: String,
    /// Link target path.
    path: String,
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
        }
    }
}

/// Table of contents entry for serialization.
#[derive(Serialize)]
struct TocResponse {
    /// Heading level (2-6).
    level: u8,
    /// Heading text.
    title: String,
    /// Anchor ID.
    id: String,
}

impl From<&TocEntry> for TocResponse {
    fn from(entry: &TocEntry) -> Self {
        Self {
            level: entry.level,
            title: entry.title.clone(),
            id: entry.id.clone(),
        }
    }
}

/// Handle GET /api/pages/ (root page).
pub(crate) async fn get_root_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ServerError> {
    get_page_impl(String::new(), state, headers)
}

/// Handle GET /api/pages/{path}.
pub(crate) async fn get_page(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ServerError> {
    get_page_impl(path, state, headers)
}

/// Shared implementation for page rendering.
#[allow(clippy::needless_pass_by_value)]
fn get_page_impl(
    path: String,
    state: Arc<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ServerError> {
    // Render the page using unified Site API (path is already without leading slash)
    let result = state
        .site
        .render(&path)
        .map_err(|_| ServerError::PageNotFound(path.clone()))?;

    // Log warnings in verbose mode
    if state.verbose && !result.warnings.is_empty() {
        for warning in &result.warnings {
            tracing::warn!(path = %path, warning = %warning, "Page render warning");
        }
    }

    // Compute ETag
    let etag = compute_etag(&state.version, &result.html);

    // Check If-None-Match header for conditional request
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && if_none_match.as_bytes() == etag.as_bytes()
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    // Get last modified time from render result
    let source_mtime = UNIX_EPOCH + Duration::from_secs_f64(result.source_mtime);
    let last_modified: DateTime<Utc> = source_mtime.into();

    // Build response using render result fields directly
    // Add leading slash to path for JSON response (frontend expects URLs with leading slash)
    let (description, page_type, vars) = if let Some(ref meta) = result.metadata {
        (
            meta.description.clone(),
            meta.page_type.clone(),
            if meta.vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&meta.vars).unwrap_or_default())
            },
        )
    } else {
        (None, None, None)
    };

    let response = PageResponse {
        meta: PageMeta {
            title: result.title,
            path: to_url_path(&path),
            source_file: result
                .source_path
                .as_ref()
                .map_or(String::new(), |p| p.display().to_string()),
            last_modified: last_modified.to_rfc3339(),
            description,
            page_type,
            vars,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(BreadcrumbResponse::from)
            .collect(),
        toc: result.toc.iter().map(TocResponse::from).collect(),
        content: result.html,
    };

    Ok((
        [
            (header::ETAG, etag),
            (
                header::LAST_MODIFIED,
                last_modified
                    .format("%a, %d %b %Y %H:%M:%S GMT")
                    .to_string(),
            ),
            (header::CACHE_CONTROL, "private, max-age=60".to_string()),
        ],
        Json(response),
    )
        .into_response())
}

/// Compute `ETag` from version and content.
///
/// Uses MD5 hash truncated to 64 bits (16 hex chars) - sufficient for
/// cache invalidation with negligible collision probability.
fn compute_etag(version: &str, content: &str) -> String {
    let hash = Md5::digest(format!("{version}:{content}").as_bytes());
    format!("\"{}\"", &hex::encode(hash)[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_etag_includes_version() {
        let etag1 = compute_etag("1.0.0", "content");
        let etag2 = compute_etag("1.0.1", "content");

        assert_ne!(etag1, etag2);
    }

    #[test]
    fn test_compute_etag_includes_content() {
        let etag1 = compute_etag("1.0.0", "content1");
        let etag2 = compute_etag("1.0.0", "content2");

        assert_ne!(etag1, etag2);
    }

    #[test]
    fn test_compute_etag_format() {
        let etag = compute_etag("1.0.0", "content");

        assert!(etag.starts_with('"'));
        assert!(etag.ends_with('"'));
        // 16 hex chars + 2 quotes = 18 total
        assert_eq!(etag.len(), 18);
    }

    #[test]
    fn test_page_meta_serialization() {
        let meta = PageMeta {
            title: Some("Guide".to_string()),
            path: "/guide".to_string(),
            source_file: "/docs/guide.md".to_string(),
            last_modified: "2025-01-01T00:00:00Z".to_string(),
            description: None,
            page_type: None,
            vars: None,
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "/guide");
        assert_eq!(json["sourceFile"], "/docs/guide.md");
        assert_eq!(json["lastModified"], "2025-01-01T00:00:00Z");
        // description, type, and vars should be omitted when None
        assert!(json.get("description").is_none());
        assert!(json.get("type").is_none());
        assert!(json.get("vars").is_none());
    }

    #[test]
    fn test_page_meta_serialization_with_metadata() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("owner".to_string(), serde_json::json!("team-a"));

        let meta = PageMeta {
            title: Some("Domain Guide".to_string()),
            path: "/domain".to_string(),
            source_file: "/docs/domain/index.md".to_string(),
            last_modified: "2025-01-01T00:00:00Z".to_string(),
            description: Some("Domain overview".to_string()),
            page_type: Some("domain".to_string()),
            vars: Some(serde_json::to_value(vars).unwrap()),
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Domain Guide");
        assert_eq!(json["description"], "Domain overview");
        assert_eq!(json["type"], "domain");
        assert_eq!(json["vars"]["owner"], "team-a");
    }
}
