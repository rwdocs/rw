//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use rw_renderer::TocEntry;
use rw_site::{BreadcrumbItem, SectionAnchor};
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
    /// Ancestry chains for the sections this page is connected to (including the
    /// page's own section), keyed by section ref; each chain starts with the
    /// section itself (empty subpath), then its ancestors, root last. Omitted
    /// when empty.
    #[serde(rename = "sectionAncestry", skip_serializing_if = "HashMap::is_empty")]
    section_ancestry: HashMap<String, Vec<SectionAnchor>>,
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
#[serde(rename_all = "camelCase")]
struct BreadcrumbResponse {
    /// Display title.
    title: String,
    /// Link target path.
    path: String,
    /// Section ref of the nearest enclosing section — the crumb's key into the
    /// page response's `sectionAncestry` map.
    section_ref: String,
    /// This crumb's path relative to `section_ref`'s scope root.
    subpath: String,
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
            section_ref: item.section_ref,
            subpath: item.subpath,
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
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(String::new(), state)
}

/// Handle GET /_api/pages/{path}.
pub(crate) async fn get_page(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(path, state)
}

/// Shared implementation for page rendering.
#[allow(clippy::needless_pass_by_value)]
fn get_page_impl(path: String, state: Arc<AppState>) -> Result<impl IntoResponse, HandlerError> {
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
        section_ancestry: result.section_ancestry,
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::http::StatusCode;
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    fn anchor(section_ref: &str, subpath: &str) -> SectionAnchor {
        SectionAnchor {
            section_ref: section_ref.to_owned(),
            subpath: subpath.to_owned(),
        }
    }

    /// A `PageResponse` with fixed `content` (identical HTML), so tests can vary
    /// only `section_ancestry` and observe its effect on serialization.
    fn page_response_with(section_ancestry: HashMap<String, Vec<SectionAnchor>>) -> PageResponse {
        PageResponse {
            meta: PageMeta {
                title: Some("Same".to_owned()),
                path: "/guide".to_owned(),
                source_file: "guide".to_owned(),
                last_modified: "2025-01-01T00:00:00Z".to_owned(),
                description: None,
                page_kind: None,
                vars: None,
                section_ref: "section:default/root".to_owned(),
                subpath: "guide".to_owned(),
            },
            breadcrumbs: Vec::new(),
            toc: Vec::new(),
            content: "<h1>Same</h1>".to_owned(),
            section_ancestry,
        }
    }

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

    #[tokio::test]
    async fn test_page_response_sets_json_content_type() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        assert_eq!(
            resp.header("content-type").as_deref(),
            Some("application/json")
        );
        assert_eq!(resp.json()["meta"]["title"], "Guide");
    }

    #[tokio::test]
    async fn test_page_response_carries_no_etag() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        // `rw serve` is a local dev server. An HTTP validator would only save a
        // loopback write of a string the handler has already rendered and
        // serialized, so page responses deliberately carry none.
        assert!(
            resp.header("etag").is_none(),
            "page responses carry no ETag"
        );
    }

    #[tokio::test]
    async fn test_page_response_sets_cache_control_no_cache() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        // Cache-Control comes from `middleware::security::cache_control_layer`,
        // not from this handler. It guards against stale content across server
        // restarts (e.g. pointing `rw serve` at another project on the same
        // localhost:7979), so pin it here — nothing else covers that layer's
        // reach over the pages route.
        assert_eq!(resp.header("cache-control").as_deref(), Some("no-cache"));
    }

    #[tokio::test]
    async fn test_section_root_page_includes_own_section_in_ancestry() {
        // A page that IS a section root gets its own section via neither
        // breadcrumbs (which exclude the current page) nor content links, yet
        // `sectionAncestry[meta.sectionRef]` must still resolve — a host maps
        // such landing pages 1:1 to catalog entities. `render()` inserts the
        // page's own enclosing section for exactly this reason.
        let storage = MockStorage::new()
            .with_document_and_kind("billing", "Billing", "domain")
            .with_content("billing", "# Billing\n\nOverview.")
            .with_mtime("billing", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/billing").await;
        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        let json = resp.json();

        let section_ref = json["meta"]["sectionRef"].as_str().unwrap();
        assert_eq!(section_ref, "domain:default/billing");
        let chain = &json["sectionAncestry"][section_ref];
        assert!(
            chain.is_array(),
            "section-root page must key its own section in sectionAncestry; got {}",
            json["sectionAncestry"]
        );
        // The chain starts with the section itself (empty subpath).
        assert_eq!(chain[0]["sectionRef"], "domain:default/billing");
        assert_eq!(chain[0]["subpath"], "");
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

    #[test]
    fn test_page_response_serializes_section_ancestry() {
        let resp = page_response_with(HashMap::from([(
            "domain:default/billing".to_owned(),
            vec![
                anchor("domain:default/billing", "overview"),
                anchor("section:default/root", ""),
            ],
        )]));

        let json = serde_json::to_value(&resp).unwrap();

        let chain = &json["sectionAncestry"]["domain:default/billing"];
        assert_eq!(chain[0]["sectionRef"], "domain:default/billing");
        assert_eq!(chain[0]["subpath"], "overview");
        assert_eq!(chain[1]["sectionRef"], "section:default/root");
        assert_eq!(chain[1]["subpath"], "");
    }

    #[test]
    fn test_page_response_omits_empty_section_ancestry() {
        let json = serde_json::to_value(page_response_with(HashMap::new())).unwrap();

        assert!(json.get("sectionAncestry").is_none());
    }
}
