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
use rw_site::{BreadcrumbItem, Section};
use serde::Serialize;

use crate::error::HandlerError;
use crate::handlers::to_url_path;
use crate::state::AppState;

/// Response for GET /_api/pages/{path}.
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
    /// Page kind (from metadata, indicates a section).
    #[serde(skip_serializing_if = "Option::is_none", rename = "kind")]
    page_kind: Option<String>,
    /// Custom variables (from metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    vars: Option<serde_json::Value>,
    /// Section ref for this page's section.
    section_ref: String,
    /// Page path relative to its section root (stable across section moves).
    subpath: String,
}

/// Breadcrumb item for serialization.
#[derive(Serialize)]
struct BreadcrumbResponse {
    /// Display title.
    title: String,
    /// Link target path.
    path: String,
    /// Section identity if this breadcrumb's path matches a section.
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<Section>,
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
            section: item.section,
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

/// Handle GET /_api/pages/ (root page).
pub(crate) async fn get_root_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(String::new(), state, headers)
}

/// Handle GET /_api/pages/{path}.
pub(crate) async fn get_page(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(path, state, headers)
}

/// Shared implementation for page rendering.
#[allow(clippy::needless_pass_by_value)]
fn get_page_impl(
    path: String,
    state: Arc<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    // Render the page using unified Site API (path is already without leading slash)
    let result = state.site.render(&path).map_err(|e| match e {
        // A page known to the navigation tree but whose source file is missing
        // from storage (FileNotFound — e.g. deleted under a stale snapshot) is a
        // not-found, not a server error; map it to 404 like an unknown page.
        rw_site::RenderError::PageNotFound(p) | rw_site::RenderError::FileNotFound(p) => {
            HandlerError::PageNotFound(p)
        }
        rw_site::RenderError::Storage(se) => HandlerError::Storage(se),
        e @ rw_site::RenderError::Io(_) => HandlerError::Render(e),
    })?;

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
    let (description, page_kind, vars) = if let Some(ref meta) = result.metadata {
        (
            meta.description.clone(),
            meta.page_kind.clone(),
            if meta.vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&meta.vars).unwrap_or_default())
            },
        )
    } else {
        (None, None, None)
    };

    let (section_ref, subpath) = state.site.section_location(&path)?;

    let response = PageResponse {
        meta: PageMeta {
            title: result.title,
            path: to_url_path(&path),
            source_file: if result.has_content {
                path.clone()
            } else {
                String::new()
            },
            last_modified: last_modified.to_rfc3339(),
            description,
            page_kind,
            vars,
            section_ref,
            subpath,
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
            (header::CACHE_CONTROL, "no-cache".to_owned()),
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

    use axum::http::StatusCode;
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    #[tokio::test]
    async fn test_page_in_tree_with_missing_source_returns_404() {
        // Regression: a page present in the site structure (returned by
        // `scan()`) but whose source file is missing from storage yields
        // `RenderError::FileNotFound`, which must map to 404 — not 500.
        // `with_document` registers the page in the tree but sets no mtime, so
        // `storage.mtime()` returns NotFound during render.
        let storage = MockStorage::new().with_document("ghost", "Ghost");
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/ghost").await;

        assert_eq!(resp.status, StatusCode::NOT_FOUND, "body: {}", resp.text());
    }

    #[tokio::test]
    async fn test_unknown_page_returns_404() {
        // A path absent from the tree raises PageNotFound (distinct from the
        // FileNotFound case above), which must also be 404.
        let server = TestServer::with_storage(MockStorage::new()).await;

        let resp = server.get("/_api/pages/does-not-exist").await;

        assert_eq!(resp.status, StatusCode::NOT_FOUND, "body: {}", resp.text());
    }

    #[tokio::test]
    async fn test_normal_page_returns_200() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
    }

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
            title: Some("Guide".to_owned()),
            path: "/guide".to_owned(),
            source_file: "/docs/guide.md".to_owned(),
            last_modified: "2025-01-01T00:00:00Z".to_owned(),
            description: None,
            page_kind: None,
            vars: None,
            section_ref: "section:default/root".to_owned(),
            subpath: "guide".to_owned(),
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "/guide");
        assert_eq!(json["sourceFile"], "/docs/guide.md");
        assert_eq!(json["lastModified"], "2025-01-01T00:00:00Z");
        assert_eq!(json["sectionRef"], "section:default/root");
        assert_eq!(json["subpath"], "guide");
        // description, kind, and vars should be omitted when None
        assert!(json.get("description").is_none());
        assert!(json.get("kind").is_none());
        assert!(json.get("vars").is_none());
    }

    #[test]
    fn test_page_meta_serialization_with_metadata() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("owner".to_owned(), serde_json::json!("team-a"));

        let meta = PageMeta {
            title: Some("Domain Guide".to_owned()),
            path: "/domain".to_owned(),
            source_file: "/docs/domain/index.md".to_owned(),
            last_modified: "2025-01-01T00:00:00Z".to_owned(),
            description: Some("Domain overview".to_owned()),
            page_kind: Some("domain".to_owned()),
            vars: Some(serde_json::to_value(vars).unwrap()),
            section_ref: "domain:default/domain".to_owned(),
            // This page IS the section root (path == section scope), so its
            // section-relative subpath is the empty string.
            subpath: String::new(),
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Domain Guide");
        assert_eq!(json["description"], "Domain overview");
        assert_eq!(json["kind"], "domain");
        assert_eq!(json["vars"]["owner"], "team-a");
        assert_eq!(json["sectionRef"], "domain:default/domain");
        assert_eq!(json["subpath"], "");
    }
}
