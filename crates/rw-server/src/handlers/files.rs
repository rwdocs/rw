//! Files API endpoint.
//!
//! Serves the project files that pages' local images and links name, by their
//! path in the project.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rw_error::error_chain;
use rw_storage::{StorageErrorKind, content_disposition, file_content_type};
use sha2::{Digest, Sha256};

use crate::error::HandlerError;
use crate::handlers::with_site;
use crate::state::AppState;

/// Handle GET /_api/files/{*path}.
///
/// A file it can't serve, for whatever reason, is a bare 404, so a request
/// cannot tell why.
pub(crate) async fn get_file(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, HandlerError> {
    let Some(content_type) = file_content_type(&path) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let read = with_site(&state.site, {
        let path = path.clone();
        move |site| {
            let bytes = site.read_file(&path)?;
            let etag = format!("\"{}\"", hex::encode(Sha256::digest(&bytes)));
            Ok::<_, HandlerError>((bytes, etag))
        }
    });
    let (bytes, etag) = match read.await {
        Ok(read) => read,
        Err(HandlerError::Storage(err)) => {
            if err.kind != StorageErrorKind::NotFound {
                tracing::warn!(path = %path, error = %error_chain(&err), "Failed to read file");
            }
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
        Err(err) => return Err(err),
    };

    // A browser replaces a cached response's headers with a 304's, so the
    // 304 repeats the sandbox.
    let revalidated = [
        (header::ETAG, etag.clone()),
        // An SVG opened on its own is a document that can run script.
        (header::CONTENT_SECURITY_POLICY, "sandbox".to_owned()),
    ];
    if headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .any(|value| matches_etag(value, &etag))
    {
        return Ok((StatusCode::NOT_MODIFIED, revalidated).into_response());
    }
    let disposition =
        content_disposition(&path).map(|value| [(header::CONTENT_DISPOSITION, value)]);
    Ok((
        [(header::CONTENT_TYPE, content_type)],
        disposition,
        revalidated,
        bytes,
    )
        .into_response())
}

/// Whether one `If-None-Match` field lists `etag`, comparing weakly as the
/// header requires.
fn matches_etag(value: &HeaderValue, etag: &str) -> bool {
    value.to_str().is_ok_and(|value| {
        value.split(',').map(str::trim).any(|candidate| {
            candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
        })
    })
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use rw_storage::{MockStorage, StorageErrorKind};

    use crate::middleware::security::CSP;
    use crate::testing::TestServer;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";

    async fn server() -> TestServer {
        TestServer::with_storage(
            MockStorage::new()
                .with_project_file("docs/img/flow.png", PNG)
                .with_project_file("docs/img/a b#1?.JPG", b"jpg".as_slice())
                .with_project_file("docs/гайд/logo.svg", SVG)
                .with_project_file(".github/assets/badge.webp", b"webp".as_slice())
                .with_project_file(".env", b"SECRET=1".as_slice())
                .with_project_file("docs/flow.puml", b"@startuml\n@enduml\n".as_slice())
                .with_unreadable_project_file("docs/locked.png", StorageErrorKind::Other),
        )
        .await
    }

    #[tokio::test]
    async fn serves_project_files_with_their_content_type() {
        let server = server().await;

        for (url, content_type, body) in [
            ("/_api/files/docs/img/flow.png", "image/png", PNG),
            (
                "/_api/files/.github/assets/badge.webp",
                "image/webp",
                b"webp".as_slice(),
            ),
            (
                "/_api/files/docs/img/a%20b%231%3F.JPG",
                "image/jpeg",
                b"jpg".as_slice(),
            ),
            (
                "/_api/files/docs/%D0%B3%D0%B0%D0%B9%D0%B4/logo.svg?v=2",
                "image/svg+xml",
                SVG,
            ),
        ] {
            let response = server.get(url).await;
            assert_eq!(response.status, StatusCode::OK, "{url}");
            assert_eq!(
                response.header("content-type").as_deref(),
                Some(content_type),
                "{url}"
            );
            assert_eq!(response.bytes(), body, "{url}");
            assert_eq!(
                response.header("x-content-type-options").as_deref(),
                Some("nosniff"),
                "{url}"
            );
            assert_eq!(
                response.header("cache-control").as_deref(),
                Some("no-cache"),
                "{url}"
            );
            assert_eq!(response.header("content-disposition"), None, "{url}");
        }
    }

    async fn downloads_server() -> TestServer {
        TestServer::with_storage(
            MockStorage::new()
                .with_file(
                    "guide",
                    "Guide",
                    "[a](/docs/spec.pdf) [b](/docs/plan%20v2.xlsx) [c](/docs/отчёт.csv) \
                     [d](/docs/a%22b.zip) [e](/docs/config.yaml) [f](/docs/page.html)\n",
                )
                .with_project_file("docs/spec.pdf", b"pdf".as_slice())
                .with_project_file("docs/plan v2.xlsx", b"xlsx".as_slice())
                .with_project_file("docs/отчёт.csv", b"csv".as_slice())
                .with_project_file("docs/a\"b.zip", b"zip".as_slice())
                .with_project_file("docs/customers.csv", b"csv".as_slice())
                .with_project_file("docs/config.yaml", b"key: secret".as_slice())
                .with_project_file("docs/page.html", b"<script></script>".as_slice()),
        )
        .await
    }

    #[tokio::test]
    async fn linked_downloads_are_attachments_named_by_their_file() {
        let server = downloads_server().await;

        for (url, content_type, disposition) in [
            (
                "/_api/files/docs/plan%20v2.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "attachment; filename=\"plan v2.xlsx\"; filename*=UTF-8''plan%20v2.xlsx",
            ),
            (
                "/_api/files/docs/%D0%BE%D1%82%D1%87%D1%91%D1%82.csv",
                "text/csv",
                "attachment; filename=\"_____.csv\"; \
                 filename*=UTF-8''%D0%BE%D1%82%D1%87%D1%91%D1%82.csv",
            ),
            (
                "/_api/files/docs/a%22b.zip",
                "application/zip",
                "attachment; filename=\"a_b.zip\"; filename*=UTF-8''a%22b.zip",
            ),
        ] {
            let response = server.get(url).await;
            assert_eq!(response.status, StatusCode::OK, "{url}");
            assert_eq!(
                response.header("content-type").as_deref(),
                Some(content_type),
                "{url}"
            );
            assert_eq!(
                response.header("content-disposition").as_deref(),
                Some(disposition),
                "{url}"
            );
            assert_eq!(
                response.header_all("content-security-policy"),
                ["sandbox", CSP],
                "{url}"
            );
            assert_eq!(
                response.header("x-content-type-options").as_deref(),
                Some("nosniff"),
                "{url}"
            );
            assert_eq!(
                response.header("cache-control").as_deref(),
                Some("no-cache"),
                "{url}"
            );
            assert!(response.header("etag").is_some(), "{url}");
        }
    }

    #[tokio::test]
    async fn a_linked_pdf_opens_in_the_browser() {
        let response = downloads_server()
            .await
            .get("/_api/files/docs/spec.pdf")
            .await;

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.header("content-type").as_deref(),
            Some("application/pdf")
        );
        assert_eq!(response.header("content-disposition"), None);
        assert_eq!(
            response.header_all("content-security-policy"),
            ["sandbox", CSP]
        );
        assert_eq!(response.bytes(), b"pdf");
    }

    #[tokio::test]
    async fn unlinked_downloads_and_other_types_are_not_found() {
        let server = downloads_server().await;

        for url in [
            "/_api/files/docs/customers.csv",
            "/_api/files/docs/config.yaml",
            "/_api/files/docs/page.html",
        ] {
            let response = server.get(url).await;
            assert_eq!(response.status, StatusCode::NOT_FOUND, "{url}");
            assert_eq!(response.bytes(), b"", "{url}");
        }
    }

    #[tokio::test]
    async fn files_are_sandboxed_on_top_of_the_page_policy() {
        let response = server().await.get("/_api/files/docs/гайд/logo.svg").await;

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.header_all("content-security-policy"),
            ["sandbox", CSP]
        );
    }

    #[tokio::test]
    async fn a_matching_etag_is_not_modified() {
        let server = server().await;
        let url = "/_api/files/docs/img/flow.png";
        let etag = server.get(url).await.header("etag").unwrap();
        assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");

        for if_none_match in [
            etag.clone(),
            format!("W/{etag}"),
            format!("\"other\", {etag}"),
            "*".to_owned(),
        ] {
            let response = server
                .request(Method::GET, url, None, &[("if-none-match", &if_none_match)])
                .await;
            assert_eq!(response.status, StatusCode::NOT_MODIFIED, "{if_none_match}");
            assert_eq!(response.header("etag").as_deref(), Some(etag.as_str()));
            assert_eq!(
                response.header_all("content-security-policy"),
                ["sandbox", CSP]
            );
            assert_eq!(
                response.header("cache-control").as_deref(),
                Some("no-cache")
            );
            assert_eq!(response.bytes(), b"");
        }

        let split = server
            .request(
                Method::GET,
                url,
                None,
                &[("if-none-match", "\"other\""), ("if-none-match", &etag)],
            )
            .await;
        assert_eq!(split.status, StatusCode::NOT_MODIFIED);

        let changed = server
            .request(Method::GET, url, None, &[("if-none-match", "\"other\"")])
            .await;
        assert_eq!(changed.status, StatusCode::OK);
        assert_eq!(changed.bytes(), PNG);
    }

    #[tokio::test]
    async fn missing_refused_escaping_and_unreadable_files_are_not_found() {
        let server = server().await;

        for url in [
            "/_api/files/docs/img/missing.png",
            "/_api/files/docs/locked.png",
            "/_api/files/.env",
            "/_api/files/docs/flow.puml",
            "/_api/files/docs/index.md",
            "/_api/files/../.env",
            "/_api/files/docs/%2E%2E/%2E%2E/secret.png",
            "/_api/files/%2Fetc%2Fhosts.png",
            "/_api/files/",
            "/_api/files",
        ] {
            let response = server.get(url).await;
            assert_eq!(response.status, StatusCode::NOT_FOUND, "{url}");
            assert_eq!(response.bytes(), b"", "{url}");
        }
    }
}
