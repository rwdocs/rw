//! Kroki diagram rendering with parallel HTTP requests.
//!
//! This module handles parallel diagram rendering via the Kroki service:
//! - Renders diagrams to PNG or SVG via HTTP POST
//! - Uses rayon thread pool for parallel requests
//! - Extracts PNG dimensions for display width calculation
//! - Computes a content-based digest via SHA256 hashing
//!
//! # Output Formats
//!
//! - [`render_all`]: PNG bytes for Confluence
//! - [`render_all_svg`]: SVG output for HTML (returns SVG strings directly)

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use rayon::prelude::*;
use std::time::Duration;
use ureq::Agent;

use crate::cache::DiagramKey;
use crate::consts::PNG_DATA_URI_PREFIX;
use crate::language::{DiagramFormat, DiagramLanguage};

/// Result of rendering a single diagram to PNG.
#[derive(Debug)]
pub struct RenderedDiagram {
    pub index: usize,
    pub data: Vec<u8>,
    /// Raw pixel width, uncorrected for DPI.
    pub width: u32,
    /// Language this was rendered from. Only PlantUML-family output is
    /// oversized, so the caller needs it to decide whether to scale.
    pub language: DiagramLanguage,
    /// Full `DiagramKey` hash, hex-encoded.
    pub digest: String,
}

/// Result of rendering a single diagram to SVG.
#[derive(Debug)]
pub struct RenderedSvg {
    /// Index matching the original diagram request.
    pub index: usize,
    /// SVG content as a string.
    pub svg: String,
    /// Language this was rendered from — see [`RenderedDiagram::language`].
    pub language: DiagramLanguage,
}

/// Result of rendering a single diagram to PNG (as base64 data URI).
#[derive(Debug)]
pub struct RenderedPngDataUri {
    /// Index matching the original diagram request.
    pub index: usize,
    /// PNG data as base64-encoded data URI.
    pub data_uri: String,
    /// Language this was rendered from — see [`RenderedDiagram::language`].
    pub language: DiagramLanguage,
}

/// Diagram info for rendering.
#[derive(Debug)]
pub struct DiagramRequest {
    pub index: usize,
    pub source: String,
    /// Diagram language (defaults to `PlantUML` for backwards compatibility).
    pub language: DiagramLanguage,
}

impl DiagramRequest {
    /// Create a new diagram request.
    pub fn new(index: usize, source: String, language: DiagramLanguage) -> Self {
        Self {
            index,
            source,
            language,
        }
    }

    /// Create an error for this diagram request.
    fn error(&self, kind: DiagramErrorKind) -> DiagramError {
        DiagramError {
            index: self.index,
            kind,
        }
    }
}

/// Single diagram rendering error.
#[derive(Debug, thiserror::Error)]
#[error("diagram {index}: {kind}")]
pub struct DiagramError {
    pub index: usize,
    pub kind: DiagramErrorKind,
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
    format: &str,
) -> Result<Vec<u8>, DiagramError> {
    let endpoint = diagram.language.kroki_endpoint();
    let url = format!("{server_url}/{endpoint}/{format}");

    let response = agent
        .post(&url)
        .header("Content-Type", "text/plain")
        .send(diagram.source.as_bytes())
        .map_err(|e| diagram.error(DiagramErrorKind::HttpRequest(e)))?;

    let status = response.status().as_u16();
    let mut body = response.into_body();

    if status >= 400 {
        let error_body = body
            .read_to_string()
            .unwrap_or_else(|_| String::from("(unable to read error body)"));
        return Err(diagram.error(DiagramErrorKind::HttpResponse {
            status,
            body: error_body,
        }));
    }

    body.read_to_vec()
        .map_err(|e| diagram.error(DiagramErrorKind::HttpRequest(e)))
}

/// Render a single diagram to PNG via Kroki.
///
/// Returns the bytes and their content key; writing them somewhere is the
/// caller's business.
fn render_one_png(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
) -> Result<RenderedDiagram, DiagramError> {
    let data = send_diagram_request(agent, diagram, server_url, "png")?;

    // Height is unused: consumers size diagrams by width and let aspect ratio
    // follow, but a malformed PNG header must still fail the render here.
    let (width, _) =
        get_png_dimensions(&data).ok_or_else(|| diagram.error(DiagramErrorKind::InvalidPng))?;

    let key = DiagramKey::for_render(&diagram.source, diagram.language, DiagramFormat::Png);

    Ok(RenderedDiagram {
        index: diagram.index,
        data,
        width,
        language: diagram.language,
        digest: key.compute_hash(),
    })
}

/// Render all diagrams to PNG in parallel using Kroki service.
///
/// Uses the global rayon thread pool for parallel rendering.
/// Returns partial results - successfully rendered diagrams even when some fail.
///
/// # Arguments
/// * `diagrams` - List of diagrams to render
/// * `server_url` - Kroki server URL (e.g., `<https://kroki.io>`)
/// * `agent` - HTTP agent for connection pooling
///
/// # Returns
/// Partial result containing both successful renders and errors.
#[must_use]
pub fn render_all(
    diagrams: &[DiagramRequest],
    server_url: &str,
    agent: &Agent,
) -> PartialRenderResult<RenderedDiagram> {
    if diagrams.is_empty() {
        return PartialRenderResult {
            rendered: Vec::new(),
            errors: Vec::new(),
        };
    }

    let server_url = server_url.trim_end_matches('/');

    let results: Vec<Result<RenderedDiagram, DiagramError>> = diagrams
        .par_iter()
        .map(|d| render_one_png(agent, d, server_url))
        .collect();

    partition_results(results)
}

/// Render a single diagram to SVG via Kroki.
fn render_one_svg(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
) -> Result<RenderedSvg, DiagramError> {
    let data = send_diagram_request(agent, diagram, server_url, "svg")?;
    let svg =
        String::from_utf8(data).map_err(|e| diagram.error(DiagramErrorKind::InvalidUtf8(e)))?;

    Ok(RenderedSvg {
        index: diagram.index,
        svg,
        language: diagram.language,
    })
}

/// Render a single diagram to PNG as base64 data URI via Kroki.
fn render_one_png_data_uri(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
) -> Result<RenderedPngDataUri, DiagramError> {
    let data = send_diagram_request(agent, diagram, server_url, "png")?;

    if get_png_dimensions(&data).is_none() {
        return Err(diagram.error(DiagramErrorKind::InvalidPng));
    }

    let base64 = BASE64_STANDARD.encode(&data);
    let data_uri = format!("{PNG_DATA_URI_PREFIX}{base64}");

    Ok(RenderedPngDataUri {
        index: diagram.index,
        data_uri,
        language: diagram.language,
    })
}

/// Result of rendering diagrams with partial failures.
#[derive(Debug)]
pub struct PartialRenderResult<T> {
    /// Successfully rendered diagrams.
    pub rendered: Vec<T>,
    /// Errors for diagrams that failed to render.
    pub errors: Vec<DiagramError>,
}

/// Generic parallel rendering with partial failure support.
///
/// Uses the global rayon thread pool for parallel rendering,
/// collecting both successes and failures.
fn render_all_partial<T: Send + std::fmt::Debug>(
    diagrams: &[DiagramRequest],
    server_url: &str,
    agent: &Agent,
    render_fn: fn(&Agent, &DiagramRequest, &str) -> Result<T, DiagramError>,
) -> PartialRenderResult<T> {
    if diagrams.is_empty() {
        return PartialRenderResult {
            rendered: Vec::new(),
            errors: Vec::new(),
        };
    }

    let server_url = server_url.trim_end_matches('/');

    let results: Vec<Result<T, DiagramError>> = diagrams
        .par_iter()
        .map(|d| render_fn(agent, d, server_url))
        .collect();

    partition_results(results)
}

/// Partition results into successes and failures.
fn partition_results<T: std::fmt::Debug>(
    results: Vec<Result<T, DiagramError>>,
) -> PartialRenderResult<T> {
    let mut rendered = Vec::with_capacity(results.len());
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(item) => rendered.push(item),
            Err(error) => errors.push(error),
        }
    }

    PartialRenderResult { rendered, errors }
}

/// Render all diagrams to SVG in parallel, returning partial results on failure.
///
/// Uses the global rayon thread pool for parallel rendering.
/// Returns successfully rendered diagrams even when some diagrams fail.
/// Use this when you want to show partial results rather than failing completely.
///
/// # Arguments
/// * `diagrams` - List of diagrams to render
/// * `server_url` - Kroki server URL (e.g., `<https://kroki.io>`)
/// * `agent` - HTTP agent for connection pooling
///
/// # Returns
/// Partial result containing both successful renders and errors.
#[must_use]
pub fn render_all_svg_partial(
    diagrams: &[DiagramRequest],
    server_url: &str,
    agent: &Agent,
) -> PartialRenderResult<RenderedSvg> {
    render_all_partial(diagrams, server_url, agent, render_one_svg)
}

/// Render all diagrams to PNG as base64 data URIs, returning partial results on failure.
///
/// Uses the global rayon thread pool for parallel rendering.
/// Returns successfully rendered diagrams even when some diagrams fail.
///
/// # Arguments
/// * `diagrams` - List of diagrams to render
/// * `server_url` - Kroki server URL
/// * `agent` - HTTP agent for connection pooling
///
/// # Returns
/// Partial result containing both successful renders and errors.
#[must_use]
pub fn render_all_png_data_uri_partial(
    diagrams: &[DiagramRequest],
    server_url: &str,
    agent: &Agent,
) -> PartialRenderResult<RenderedPngDataUri> {
    render_all_partial(diagrams, server_url, agent, render_one_png_data_uri)
}

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
