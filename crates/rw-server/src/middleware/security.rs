//! Security headers middleware.
//!
//! Adds security headers to all responses:
//! - Content-Security-Policy
//! - X-Content-Type-Options
//! - X-Frame-Options

use axum::http::HeaderValue;
use axum::http::header::HeaderName;
use tower_http::set_header::SetResponseHeaderLayer;

/// Content-Security-Policy header value.
pub(crate) const CSP: &str = "default-src 'self'; \
                   script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   font-src 'self' data:; \
                   img-src 'self' data: https:; \
                   connect-src 'self' ws: wss:; \
                   frame-ancestors 'none'";

/// Create layer that adds Content-Security-Policy header.
///
/// Appended rather than overriding, so a policy a handler sets stays in force
/// too: a browser enforces every policy it gets, so a handler can tighten
/// this one but never loosen it.
pub(crate) fn csp_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::appending(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(CSP),
    )
}

/// Create layer that adds X-Content-Type-Options header.
pub(crate) fn content_type_options_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    )
}

/// Create layer that adds X-Frame-Options header.
pub(crate) fn frame_options_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    )
}

/// Create layer that adds Cache-Control: no-cache header.
///
/// Forces browsers to revalidate every request with the server, preventing
/// stale content when switching between projects or restarting the server.
pub(crate) fn cache_control_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestServer;

    #[tokio::test]
    async fn responses_carry_the_security_headers() {
        let response = TestServer::new().await.get("/missing-asset.js").await;

        assert_eq!(response.header_all("content-security-policy"), [CSP]);
        assert_eq!(
            response.header("x-content-type-options").as_deref(),
            Some("nosniff")
        );
        assert_eq!(response.header("x-frame-options").as_deref(), Some("DENY"));
        assert_eq!(
            response.header("cache-control").as_deref(),
            Some("no-cache")
        );
    }

    #[test]
    fn images_load_from_https_but_not_http() {
        let img_src = CSP
            .split(';')
            .map(str::trim)
            .find(|directive| directive.starts_with("img-src"));

        assert_eq!(img_src, Some("img-src 'self' data: https:"));
    }
}
