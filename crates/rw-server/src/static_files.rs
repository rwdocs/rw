//! Static file serving.
//!
//! Provides static file serving for frontend assets and SPA fallback.
//! Uses `rw-assets` for asset retrieval in both embedded and filesystem modes.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use percent_encoding::percent_decode_str;
use rw_site::Site;

use crate::error::HandlerError;
use crate::handlers::with_site;
use crate::state::AppState;

/// Serve a static asset, or the shell for a page route: the embedded preview
/// under `/_embedded/`, `index.html` elsewhere.
pub(crate) async fn fallback(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    if let Some(page_path) = embedded_page_path(path) {
        if is_spa_route(&state.site, page_path).await {
            return rw_embedded_preview::preview_page();
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(response) = asset_response(path) {
        return response;
    }

    if is_spa_route(&state.site, path).await
        && let Some(index) = rw_assets::get("index.html")
    {
        return ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], index).into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

fn embedded_page_path(path: &str) -> Option<&str> {
    match path.strip_prefix("_embedded")? {
        "" => Some(""),
        rest => rest.strip_prefix('/'),
    }
}

/// Every dotless path gets the shell so the viewer can show its own not-found
/// page, but a dotted path only when it names a page: otherwise a missing
/// `/foo.js` would answer 200 with HTML.
async fn is_spa_route(site: &Arc<Site>, path: &str) -> bool {
    if is_reserved_api(path) {
        return false;
    }
    if !path.contains('.') {
        return true;
    }
    let Ok(path) = percent_decode_str(path).decode_utf8() else {
        return false;
    };
    let path = path.trim_end_matches('/').to_owned();
    let has_page = with_site(site, move |site| {
        site.has_page_under(&path).map_err(HandlerError::from)
    });
    matches!(has_page.await, Ok(true))
}

fn asset_response(path: &str) -> Option<Response> {
    let content = rw_assets::get(path)?;
    Some(([(header::CONTENT_TYPE, rw_assets::mime_for(path))], content).into_response())
}

/// A bad API path must 404 rather than get an HTML shell, and that includes
/// the bare `_api` with no trailing slash.
fn is_reserved_api(path: &str) -> bool {
    path == "_api" || path.starts_with("_api/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_cache::NullCache;
    use rw_site::PageRendererConfig;
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    #[tokio::test]
    async fn test_serve_asset_returns_not_found_for_missing() {
        let server = TestServer::new().await;
        for path in ["/missing-asset.js", "/_api/does-not-exist", "/_api"] {
            let response = server.get(path).await;
            assert_eq!(response.status, StatusCode::NOT_FOUND, "for {path}");
            assert!(response.bytes().is_empty(), "for {path}");
        }
    }

    #[tokio::test]
    async fn spa_route_admits_dotted_page_paths_only() {
        let storage = MockStorage::new()
            .with_document("release-1.2", "Release")
            .with_document("v1.0/intro", "Intro")
            .with_document("notes/Q1 2.0", "Q1");
        let site = Arc::new(Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        ));

        for path in [
            "",
            "guide",
            "missing-page",
            "release-1.2",
            "release-1.2/",
            "v1.0",
            "v1.0/intro",
            "notes/Q1%202.0",
        ] {
            assert!(
                is_spa_route(&site, path).await,
                "{path} should get the shell"
            );
        }
        for path in [
            "missing-asset.js",
            "release-1.3",
            "v1.0/missing",
            "release-1.2.js",
            "_api",
            "_api/pages/release-1.2",
        ] {
            assert!(!is_spa_route(&site, path).await, "{path} should 404");
        }
    }

    mod embedded_preview {
        use super::*;

        async fn server() -> TestServer {
            TestServer::with_storage(MockStorage::new().with_document("release-1.2", "Release"))
                .await
        }

        #[tokio::test]
        async fn page_routes_under_the_prefix_get_the_preview_shell() {
            let server = server().await;
            for path in [
                "/_embedded",
                "/_embedded/",
                "/_embedded/some/doc/path",
                "/_embedded/release-1.2",
            ] {
                let response = server.get(path).await;
                assert_eq!(response.status, StatusCode::OK, "for {path}");
                assert_eq!(
                    response.header("content-type").as_deref(),
                    Some("text/html; charset=utf-8"),
                    "for {path}"
                );
                assert!(
                    response.text().contains("/_embedded/preview.js"),
                    "for {path}"
                );
            }
        }

        #[tokio::test]
        async fn missing_assets_and_api_paths_under_the_prefix_are_not_found() {
            let server = server().await;
            for path in [
                "/_embedded/lib/missing.js",
                "/_embedded/missing-asset.js.map",
                "/_embedded/_api/does-not-exist",
                "/_embedded/_api",
                "/_embedded.js",
            ] {
                let response = server.get(path).await;
                assert_eq!(response.status, StatusCode::NOT_FOUND, "for {path}");
            }
        }

        #[tokio::test]
        async fn serves_the_preview_script_and_style() {
            let server = server().await;
            for (path, content_type) in [
                ("/_embedded/preview.js", "text/javascript; charset=utf-8"),
                ("/_embedded/preview.css", "text/css; charset=utf-8"),
            ] {
                let response = server.get(path).await;
                assert_eq!(response.status, StatusCode::OK, "for {path}");
                assert_eq!(
                    response.header("content-type").as_deref(),
                    Some(content_type),
                    "for {path}"
                );
            }
        }
    }
}
