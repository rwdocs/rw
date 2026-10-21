//! `Host` header check against DNS rebinding.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::uri::Authority;
use axum::http::{StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};

/// Refuse requests addressed to a host name other than `localhost` or one in
/// `allowed`, unless `allowed` holds `all`.
///
/// A site that rebinds its own name to 127.0.0.1 reaches the server with
/// that name in `Host`; an IP literal or `localhost` can't be rebound.
pub(crate) fn check_hosts<S>(router: Router<S>, allowed: &[String]) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    if allowed.iter().any(|name| name.eq_ignore_ascii_case("all")) {
        return router;
    }
    let allowed: Arc<[String]> = allowed.iter().map(|n| n.to_ascii_lowercase()).collect();
    router.layer(from_fn_with_state(allowed, check_host))
}

/// HTTP/2 carries the host in the URI instead of the header, so both count.
/// A request with neither can't come from a browser and passes.
async fn check_host(
    State(allowed): State<Arc<[String]>>,
    request: Request,
    next: Next,
) -> Response {
    let header = request
        .headers()
        .get(header::HOST)
        .map(|value| value.to_str().unwrap_or_default());
    let refused = header
        .into_iter()
        .chain(request.uri().host())
        .find_map(|authority| refused_host(authority, &allowed));
    match refused {
        None => next.run(request).await,
        Some(host) => (
            StatusCode::FORBIDDEN,
            format!(
                "Host \"{host}\" not allowed. Add it with --allowed-host {host} \
                 or [server].allowed_hosts in rw.toml.\n"
            ),
        )
            .into_response(),
    }
}

/// The lowercased host of `authority` when it is not allowed.
fn refused_host(authority: &str, allowed: &[String]) -> Option<String> {
    let Ok(parsed) = authority.parse::<Authority>() else {
        return Some(authority.to_ascii_lowercase());
    };
    let host = parsed.host();
    if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return v6.parse::<Ipv6Addr>().is_err().then(|| host.to_owned());
    }
    let host = host.to_ascii_lowercase();
    let is_allowed = host.parse::<Ipv4Addr>().is_ok()
        || host == "localhost"
        || host.ends_with(".localhost")
        || allowed.iter().any(|name| match name.strip_prefix('.') {
            Some(domain) => host == domain || host.ends_with(name.as_str()),
            None => host == *name,
        });
    (!is_allowed).then_some(host)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    async fn status_for_host(host: &str) -> StatusCode {
        status_with_allowed(&[], host).await
    }

    async fn status_with_allowed(allowed: &[&str], host: &str) -> StatusCode {
        TestServer::new()
            .await
            .allowing(allowed)
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
    async fn configured_names_are_served() {
        let allowed = ["Docs.Example", ".ts.net"];
        for host in [
            "docs.example:8080",
            "DOCS.example",
            "ts.net",
            "laptop.ts.net:8080",
            "a.laptop.ts.net",
        ] {
            assert_eq!(
                status_with_allowed(&allowed, host).await,
                StatusCode::OK,
                "{host}"
            );
        }
        for host in [
            "evil.example",
            "a.docs.example",
            "evilts.net",
            "ts.net.evil.example",
        ] {
            assert_eq!(
                status_with_allowed(&allowed, host).await,
                StatusCode::FORBIDDEN,
                "{host}"
            );
        }
    }

    #[tokio::test]
    async fn all_turns_the_check_off() {
        assert_eq!(
            status_with_allowed(&["all"], "evil.example").await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn refusal_names_the_host_and_the_fix() {
        let response = TestServer::new()
            .await
            .request(Method::GET, "/", None, &[("host", "Foo.Example:8080")])
            .await;

        assert_eq!(response.status, StatusCode::FORBIDDEN);
        assert_eq!(
            response.header("content-type").as_deref(),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(
            response.text(),
            "Host \"foo.example\" not allowed. Add it with --allowed-host foo.example \
             or [server].allowed_hosts in rw.toml.\n"
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
