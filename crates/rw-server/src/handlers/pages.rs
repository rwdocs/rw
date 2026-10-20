//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use rw_renderer::TocEntry;
use rw_site::{BreadcrumbItem, RenderError, SectionAnchor, Site, to_url_path};
use serde::Serialize;

use crate::error::HandlerError;
use crate::handlers::with_site;
use crate::state::AppState;

/// Response for GET /_api/pages/{path}.
#[derive(Serialize)]
struct PageResponse {
    /// Page metadata.
    meta: PageMeta,
    /// Breadcrumb navigation items.
    breadcrumbs: Vec<BreadcrumbResponse>,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Rendered HTML content.
    content: String,
    /// Ancestry chains for the sections this page is connected to (including the
    /// page's own section), keyed by section ref; each chain starts with the
    /// section itself (empty subpath), then its ancestors, root last. Omitted
    /// when empty.
    #[serde(rename = "sectionAncestry", skip_serializing_if = "BTreeMap::is_empty")]
    section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
}

/// Page metadata.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageMeta {
    /// The page's resolved title — the same string navigation and breadcrumbs
    /// report for this page.
    title: String,
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

/// Handle GET /_api/pages/ (root page).
pub(crate) async fn get_root_page(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, HandlerError> {
    with_site(&state.site, |site| page_response(site, "")).await
}

/// Handle GET /_api/pages/{path}.
pub(crate) async fn get_page(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, HandlerError> {
    with_site(&state.site, move |site| page_response(site, &path)).await
}

fn page_response(site: &Site, path: &str) -> Result<Json<PageResponse>, HandlerError> {
    let result = site.render(path).map_err(|e| match e {
        // A page known to the navigation tree but whose source file is missing
        // from storage (FileNotFound — e.g. deleted under a stale snapshot) is a
        // not-found, not a server error; map it to 404 like an unknown page.
        RenderError::PageNotFound(p) | RenderError::FileNotFound(p) => {
            HandlerError::PageNotFound(p)
        }
        RenderError::Storage(se) => HandlerError::Storage(se),
    })?;

    for warning in &result.warnings {
        tracing::warn!(path = %path, warning = %warning, "Page render warning");
    }

    let fields = result.meta_fields;

    let response = PageResponse {
        meta: PageMeta {
            title: result.meta.title.clone(),
            path: fields.path,
            source_file: fields.source_file,
            last_modified: fields.last_modified,
            description: result.meta.description.clone(),
            page_kind: result.meta.kind.clone(),
            section_ref: fields.section_ref,
            subpath: fields.subpath,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(BreadcrumbResponse::from)
            .collect(),
        toc: result.toc,
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
    use serde_json::json;

    use crate::testing::TestServer;

    fn anchor(section_ref: &str, subpath: &str) -> SectionAnchor {
        SectionAnchor {
            section_ref: section_ref.to_owned(),
            subpath: subpath.to_owned(),
        }
    }

    /// A `PageResponse` with fixed `content` (identical HTML), so tests can vary
    /// only `section_ancestry` and observe its effect on serialization.
    fn page_response_with(section_ancestry: BTreeMap<String, Vec<SectionAnchor>>) -> PageResponse {
        PageResponse {
            meta: PageMeta {
                title: "Same".to_owned(),
                path: "/guide".to_owned(),
                source_file: "guide".to_owned(),
                last_modified: "2025-01-01T00:00:00Z".to_owned(),
                description: None,
                page_kind: None,
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
    async fn page_with_unusable_mtime_still_renders() {
        // A manifest written by another tool can record a wrong-unit epoch
        // timestamp; 1.75e18 is nanoseconds in a seconds field. Converting it
        // must not panic and take down the request for the page.
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1.75e18);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        assert_eq!(
            resp.json()["meta"]["lastModified"],
            "1970-01-01T00:00:00+00:00",
            "an unusable mtime is reported as unknown, not as a crash"
        );
    }

    #[tokio::test]
    async fn test_page_response_is_uncached_json() {
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
        // Guards against stale content when `rw serve` is pointed at another
        // project on the same port.
        assert_eq!(resp.header("cache-control").as_deref(), Some("no-cache"));
        assert_eq!(resp.json()["meta"]["title"], "Guide");
    }

    #[tokio::test]
    async fn repeated_page_requests_return_identical_bodies() {
        let storage = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
            .into_iter()
            .fold(MockStorage::new(), |storage, name| {
                storage
                    .with_document_meta(name, "kind: domain")
                    .with_content(name, "# Domain")
            })
            .with_file(
                "guide",
                "Guide",
                "[a](/alpha) [b](/bravo) [c](/charlie) [d](/delta) [e](/echo) [f](/foxtrot)",
            )
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let first = server.get("/_api/pages/guide").await.text();
        for _ in 0..20 {
            assert_eq!(server.get("/_api/pages/guide").await.text(), first);
        }
    }

    #[tokio::test]
    async fn test_section_root_page_includes_own_section_in_ancestry() {
        // A page that IS a section root gets its own section via neither
        // breadcrumbs (which exclude the current page) nor content links, yet
        // `sectionAncestry[meta.sectionRef]` must still resolve — a host maps
        // such landing pages 1:1 to catalog entities. `render()` inserts the
        // page's own enclosing section for exactly this reason.
        let storage = MockStorage::new()
            .with_document_meta("billing", "title: Billing\nkind: domain")
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

    #[tokio::test]
    async fn test_page_response_forwards_resolved_meta_from_render_result() {
        let storage = MockStorage::new()
            .with_document_meta(
                "billing",
                "title: Resolved Billing\nkind: domain\ndescription: Money stuff",
            )
            .with_content("billing", "# Body H1\n\nOverview.\n\n## Setup Steps")
            .with_mtime("billing", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/billing").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        let json = resp.json();
        assert_eq!(
            json["toc"],
            json!([{"level": 2, "title": "Setup Steps", "id": "setup-steps"}])
        );
        let mut response_keys: Vec<_> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        response_keys.sort_unstable();
        assert_eq!(
            response_keys,
            ["breadcrumbs", "content", "meta", "sectionAncestry", "toc"]
        );
        assert_eq!(json["meta"]["title"], "Resolved Billing");
        assert_eq!(json["meta"]["path"], "/billing");
        assert_eq!(json["meta"]["sourceFile"], "billing");
        assert_eq!(json["meta"]["lastModified"], "1970-01-01T00:16:40+00:00");
        assert_eq!(json["meta"]["description"], "Money stuff");
        assert_eq!(json["meta"]["kind"], "domain");
        assert_eq!(json["meta"]["sectionRef"], "domain:default/billing");
        assert_eq!(json["meta"]["subpath"], "");
        let mut keys: Vec<_> = json["meta"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "description",
                "kind",
                "lastModified",
                "path",
                "sectionRef",
                "sourceFile",
                "subpath",
                "title",
            ]
        );
    }

    #[test]
    fn test_page_meta_serialization() {
        let meta = PageMeta {
            title: "Guide".to_owned(),
            path: "/guide".to_owned(),
            source_file: "/docs/guide.md".to_owned(),
            last_modified: "2025-01-01T00:00:00Z".to_owned(),
            description: None,
            page_kind: None,
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
        // description and kind should be omitted when None
        assert!(json.get("description").is_none());
        assert!(json.get("kind").is_none());
    }

    #[test]
    fn test_page_response_serializes_section_ancestry() {
        let resp = page_response_with(BTreeMap::from([(
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
        let json = serde_json::to_value(page_response_with(BTreeMap::new())).unwrap();

        assert!(json.get("sectionAncestry").is_none());
    }
}
