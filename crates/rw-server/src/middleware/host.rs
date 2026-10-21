//! `Host` header check against DNS rebinding.

use std::net::{Ipv4Addr, Ipv6Addr};

use axum::extract::Request;
use axum::http::uri::Authority;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Refuse requests addressed to a hostname other than `localhost`.
///
/// A site that rebinds its own name to 127.0.0.1 reaches the server with
/// that name in `Host`; an IP literal or `localhost` can't be rebound.
/// HTTP/2 carries the host in the URI instead of the header, so both count.
/// A request with neither can't come from a browser and passes.
pub(crate) async fn check_host(request: Request, next: Next) -> Response {
    let header = request
        .headers()
        .get(header::HOST)
        .map(|value| value.to_str().unwrap_or_default());
    let mut hosts = header.into_iter().chain(request.uri().host());
    if hosts.all(is_allowed) {
        next.run(request).await
    } else {
        (
            StatusCode::FORBIDDEN,
            "Host not allowed: open rw serve via localhost or an IP address\n",
        )
            .into_response()
    }
}

fn is_allowed(authority: &str) -> bool {
    let Ok(authority) = authority.parse::<Authority>() else {
        return false;
    };
    let host = authority.host();
    if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return v6.parse::<Ipv6Addr>().is_ok();
    }
    let host = host.to_ascii_lowercase();
    host.parse::<Ipv4Addr>().is_ok() || host == "localhost" || host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    async fn status_for_host(host: &str) -> StatusCode {
        TestServer::new()
            .await
            .request(Method::GET, "/_api/config", None, &[("host", host)])
            .await
            .status
    }

    #[tokio::test]
    async fn local_and_ip_hosts_are_served() {
        for host in [
            "localhost:8080",
            "localhost",
            "LocalHost:8080",
            "docs.localhost:8080",
            "127.0.0.1:8080",
            "192.168.1.20",
            "[::1]:8080",
            "[::1]",
        ] {
            assert_eq!(status_for_host(host).await, StatusCode::OK, "{host}");
        }
    }

    #[tokio::test]
    async fn other_hosts_are_refused() {
        for host in [
            "evil.example",
            "evil.example:8080",
            "localhost.evil.example",
            "evillocalhost",
            "::1",
            "[evil.example]",
            "",
        ] {
            assert_eq!(status_for_host(host).await, StatusCode::FORBIDDEN, "{host}");
        }
    }

    #[tokio::test]
    async fn refusal_is_plain_text() {
        let response = TestServer::new()
            .await
            .request(Method::GET, "/", None, &[("host", "evil.example")])
            .await;

        assert_eq!(response.status, StatusCode::FORBIDDEN);
        assert_eq!(
            response.text(),
            "Host not allowed: open rw serve via localhost or an IP address\n"
        );
    }

    #[tokio::test]
    async fn comment_writes_are_refused() {
        let doc = "section:default/root#a";
        let storage = MockStorage::new()
            .with_file("a", "A", "# A\n")
            .with_mtime("a", 0.0);
        let server = TestServer::with_storage(storage).await;
        for (host, status) in [
            ("evil.example:8080", StatusCode::FORBIDDEN),
            ("localhost:8080", StatusCode::CREATED),
        ] {
            let response = server
                .request(
                    Method::POST,
                    "/_api/comments",
                    Some(serde_json::json!({ "documentId": doc, "body": "hi" })),
                    &[("host", host)],
                )
                .await;
            assert_eq!(response.status, status, "{host}");
        }
        let list = server
            .get("/_api/comments?documentId=section:default/root%23a")
            .await;
        assert_eq!(list.json().as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn live_reload_upgrade_is_refused() {
        let response = TestServer::with_live_reload(MockStorage::new())
            .await
            .request(
                Method::GET,
                "/ws/live-reload",
                None,
                &[
                    ("host", "evil.example:8080"),
                    ("connection", "upgrade"),
                    ("upgrade", "websocket"),
                    ("sec-websocket-version", "13"),
                    ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
                ],
            )
            .await;

        assert_eq!(response.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn uri_authority_is_checked_too() {
        let response = TestServer::new()
            .await
            .request(
                Method::GET,
                "http://evil.example/_api/config",
                None,
                &[("host", "localhost")],
            )
            .await;

        assert_eq!(response.status, StatusCode::FORBIDDEN);
    }
}
