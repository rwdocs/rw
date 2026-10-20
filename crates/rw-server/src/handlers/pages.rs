//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use rw_site::api::PageResponse;
use rw_site::{RenderError, Site};

use crate::error::HandlerError;
use crate::handlers::with_site;
use crate::state::AppState;

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

    Ok(Json(PageResponse::from(result)))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use rw_storage::MockStorage;
    use serde_json::json;

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
        assert_eq!(json["meta"]["documentId"], "domain:default/billing#");
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
                "documentId",
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
}
