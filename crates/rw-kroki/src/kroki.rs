//! Kroki diagram rendering with parallel HTTP requests.
//!
//! [`render_all`] posts every diagram to Kroki on the rayon thread pool and
//! hands back, in request order, SVG text or a PNG base64 data URI — content,
//! never files: writing a diagram out is the caller's business.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use rayon::prelude::*;
use std::time::Duration;
use ureq::Agent;

use crate::consts::PNG_DATA_URI_PREFIX;
use crate::language::{DiagramFormat, DiagramLanguage};

/// One diagram to render.
#[derive(Debug)]
pub struct DiagramRequest {
    pub source: String,
    pub language: DiagramLanguage,
    pub format: DiagramFormat,
}

/// Kind of diagram rendering error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiagramErrorKind {
    /// HTTP request failed (network error, timeout, etc).
    #[error("HTTP request failed")]
    HttpRequest(#[source] ureq::Error),
    /// HTTP response error (server returned error status).
    #[error("HTTP {status}: {body}")]
    HttpResponse {
        /// HTTP status code.
        status: u16,
        /// Response body (may contain error details).
        body: String,
    },
    /// Invalid UTF-8 in response.
    #[error("invalid UTF-8")]
    InvalidUtf8(#[source] std::string::FromUtf8Error),
    /// Invalid PNG data (missing or malformed header).
    #[error("invalid PNG data")]
    InvalidPng,
}

impl DiagramErrorKind {
    /// Whether this error is transient — i.e. a retry with the same diagram
    /// source could succeed once the underlying condition clears.
    ///
    /// Network failures and Kroki 5xx server errors are transient (Kroki was
    /// unreachable or briefly failing), as are the retryable 4xx statuses a
    /// server or reverse proxy raises under transient conditions — 408 Request
    /// Timeout, 425 Too Early, and 429 Too Many Requests. Every other 4xx
    /// response (e.g. Kroki returns 400 for malformed diagram source) is
    /// deterministic: the same source will fail identically on every retry.
    /// Malformed-response kinds are treated as deterministic to avoid an
    /// infinite re-render loop on a persistently bad response.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::HttpRequest(_) => true,
            Self::HttpResponse { status, .. } => {
                *status >= 500 || matches!(status, 408 | 425 | 429)
            }
            Self::InvalidUtf8(_) | Self::InvalidPng => false,
        }
    }
}

/// Create HTTP agent with the specified timeout.
///
/// Use this to create a reusable agent for connection pooling when making
/// multiple render calls.
pub fn create_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Extract width and height from PNG image data.
///
/// PNG format: 8-byte signature, then IHDR chunk with width/height at bytes 16-24.
fn get_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 {
        return None;
    }

    // PNG signature check
    if &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    // IHDR chunk: width at bytes 16-20, height at bytes 20-24 (big-endian)
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((width, height))
}

/// Extract width and height from a `data:image/png;base64,...` URI.
///
/// Only the IHDR chunk is needed, so this decodes just the leading bytes rather
/// than the whole image — inline diagrams are cached as data URIs, so this runs
/// on every cache hit as well as every fresh render.
pub(crate) fn png_data_uri_dimensions(data_uri: &str) -> Option<(u32, u32)> {
    const PNG_HEADER_LEN: usize = 24;
    // 4 base64 chars encode 3 bytes; round up so the slice covers the header.
    const B64_PREFIX_LEN: usize = PNG_HEADER_LEN.div_ceil(3) * 4;

    let b64 = data_uri.strip_prefix(PNG_DATA_URI_PREFIX)?;
    let prefix = b64.get(..B64_PREFIX_LEN)?;
    let bytes = BASE64_STANDARD.decode(prefix).ok()?;
    get_png_dimensions(&bytes)
}

/// Send a diagram to Kroki and return the response body as bytes.
///
/// Handles HTTP errors by reading the response body for error details.
fn send_diagram_request(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
) -> Result<Vec<u8>, DiagramErrorKind> {
    let endpoint = diagram.language.kroki_endpoint();
    let format = diagram.format.as_str();
    let url = format!("{server_url}/{endpoint}/{format}");

    let response = agent
        .post(&url)
        .header("Content-Type", "text/plain")
        .send(diagram.source.as_bytes())
        .map_err(DiagramErrorKind::HttpRequest)?;

    let status = response.status().as_u16();
    let mut body = response.into_body();

    if status >= 400 {
        let error_body = body
            .read_to_string()
            .unwrap_or_else(|_| String::from("(unable to read error body)"));
        return Err(DiagramErrorKind::HttpResponse {
            status,
            body: error_body,
        });
    }

    body.read_to_vec().map_err(DiagramErrorKind::HttpRequest)
}

/// Render a single diagram via Kroki: SVG text, or a PNG as a base64 data URI.
fn render_one(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
) -> Result<String, DiagramErrorKind> {
    let data = send_diagram_request(agent, diagram, server_url)?;
    match diagram.format {
        DiagramFormat::Svg => String::from_utf8(data).map_err(DiagramErrorKind::InvalidUtf8),
        DiagramFormat::Png => {
            if get_png_dimensions(&data).is_none() {
                return Err(DiagramErrorKind::InvalidPng);
            }
            let base64 = BASE64_STANDARD.encode(&data);
            Ok(format!("{PNG_DATA_URI_PREFIX}{base64}"))
        }
    }
}

/// Render all diagrams in parallel, one result per diagram, in order.
///
/// A failed diagram fails only its own result.
#[must_use]
pub fn render_all(
    diagrams: &[DiagramRequest],
    server_url: &str,
    agent: &Agent,
) -> Vec<Result<String, DiagramErrorKind>> {
    let server_url = server_url.trim_end_matches('/');
    diagrams
        .par_iter()
        .map(|d| render_one(agent, d, server_url))
        .collect()
}

#[cfg(test)]
mod http_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_transient_classifies_error_kinds() {
        // 5xx server errors and network failures are transient (retry may succeed).
        assert!(
            DiagramErrorKind::HttpResponse {
                status: 500,
                body: String::new(),
            }
            .is_transient()
        );
        assert!(
            DiagramErrorKind::HttpResponse {
                status: 503,
                body: String::new(),
            }
            .is_transient()
        );

        // Retryable 4xx statuses (rate-limit / timeout, often raised by a proxy
        // in front of Kroki) are transient.
        for status in [408, 425, 429] {
            assert!(
                DiagramErrorKind::HttpResponse {
                    status,
                    body: String::new(),
                }
                .is_transient(),
                "status {status} should be transient"
            );
        }

        // Other 4xx (e.g. malformed diagram source → Kroki 400) is deterministic.
        assert!(
            !DiagramErrorKind::HttpResponse {
                status: 400,
                body: String::new(),
            }
            .is_transient()
        );
        assert!(
            !DiagramErrorKind::HttpResponse {
                status: 404,
                body: String::new(),
            }
            .is_transient()
        );

        // Malformed-response kinds are deterministic: retrying yields the same
        // bad response, so caching avoids an infinite re-render loop.
        assert!(!DiagramErrorKind::InvalidPng.is_transient());
    }

    #[test]
    fn test_get_png_dimensions() {
        // Minimal valid PNG with 100x50 dimensions
        let mut png_data = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR length
            b'I', b'H', b'D', b'R', // IHDR type
            0x00, 0x00, 0x00, 0x64, // width = 100
            0x00, 0x00, 0x00, 0x32, // height = 50
        ];
        png_data.extend_from_slice(&[0; 5]); // bit depth, color type, etc.

        let dims = get_png_dimensions(&png_data);
        assert_eq!(dims, Some((100, 50)));
    }

    #[test]
    fn test_png_data_uri_dimensions() {
        // 200x100 PNG header, base64'd — only the IHDR chunk is needed.
        let uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAMgAAABkAAAAAAA=";
        assert_eq!(png_data_uri_dimensions(uri), Some((200, 100)));
    }

    #[test]
    fn test_png_data_uri_dimensions_rejects_non_png_uri() {
        // An SVG data URI reaches this only through a bug; it must not be
        // mistaken for a PNG whose first bytes happen to decode.
        assert_eq!(
            png_data_uri_dimensions("data:image/svg+xml;base64,PHN2Zy8+"),
            None
        );
        assert_eq!(png_data_uri_dimensions("data:image/png;base64,AAAA"), None);
    }

    #[test]
    fn test_get_png_dimensions_invalid() {
        let invalid_data = b"not a png";
        assert_eq!(get_png_dimensions(invalid_data), None);
    }
}
