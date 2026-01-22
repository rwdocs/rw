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
const CSP: &str = "default-src 'self'; \
                   script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   font-src 'self' data:; \
                   img-src 'self' data:; \
                   connect-src 'self' ws: wss:; \
                   frame-ancestors 'none'";

/// Create layer that adds Content-Security-Policy header.
pub(crate) fn csp_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csp_value() {
        assert!(CSP.contains("default-src 'self'"));
        assert!(CSP.contains("script-src 'self'"));
        assert!(CSP.contains("connect-src 'self' ws: wss:"));
        assert!(CSP.contains("frame-ancestors 'none'"));
    }
}
