//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use docstage_renderer::TocEntry;
use docstage_site::BreadcrumbItem;
use md5::{Digest, Md5};
use serde::Serialize;

use crate::error::ServerError;
use crate::state::AppState;

/// Response for GET /api/pages/{path}.
#[derive(Serialize)]
pub struct PageResponse {
    /// Page metadata.
    pub meta: PageMeta,
    /// Breadcrumb navigation items.
    pub breadcrumbs: Vec<BreadcrumbResponse>,
    /// Table of contents entries.
    pub toc: Vec<TocResponse>,
    /// Rendered HTML content.
    pub content: String,
}

/// Page metadata.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    /// Page title (from H1 heading).
    pub title: Option<String>,
    /// URL path.
    pub path: String,
    /// Source file path.
    pub source_file: String,
    /// Last modification time (ISO 8601).
    pub last_modified: String,
}

/// Breadcrumb item for serialization.
#[derive(Serialize)]
pub struct BreadcrumbResponse {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: item.path,
        }
    }
}

/// Table of contents entry for serialization.
#[derive(Serialize)]
pub struct TocResponse {
    /// Heading level (2-6).
    pub level: u8,
    /// Heading text.
    pub title: String,
    /// Anchor ID.
    pub id: String,
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
pub async fn get_root_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ServerError> {
    get_page_impl(String::new(), state, headers)
}

/// Handle GET /api/pages/{path}.
pub async fn get_page(
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
    // Load site and resolve source path
    let site = state.site_loader.write().unwrap().load(true).clone();
    let source_path = site
        .resolve_source_path(&path)
        .ok_or_else(|| ServerError::PageNotFound(path.clone()))?;

    // Render the page
    let result = state.renderer.render(&source_path, &path)?;

    // Log warnings in verbose mode
    if state.verbose && !result.warnings.is_empty() {
        for warning in &result.warnings {
            tracing::warn!("{}: {}", path, warning);
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

    // Get last modified time
    let source_mtime = source_path
        .metadata()
        .map_err(ServerError::Io)?
        .modified()
        .map_err(ServerError::Io)?;
    let last_modified: DateTime<Utc> = source_mtime.into();

    // Build response
    let breadcrumbs = site
        .get_breadcrumbs(&path)
        .into_iter()
        .map(BreadcrumbResponse::from)
        .collect();

    let response = PageResponse {
        meta: PageMeta {
            title: result.title,
            path: if path.is_empty() {
                "/".to_string()
            } else {
                format!("/{path}")
            },
            source_file: source_path.display().to_string(),
            last_modified: last_modified.to_rfc3339(),
        },
        breadcrumbs,
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
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "/guide");
        assert_eq!(json["sourceFile"], "/docs/guide.md");
        assert_eq!(json["lastModified"], "2025-01-01T00:00:00Z");
    }
}
