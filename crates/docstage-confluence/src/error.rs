//! Error types for Confluence integration.

use std::str::Utf8Error;

/// Error during comment preservation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommentPreservationError {
    /// XML parsing error.
    #[error("XML parse error")]
    XmlParse(#[from] quick_xml::Error),

    /// UTF-8 decoding error.
    #[error("UTF-8 error")]
    Utf8(#[from] Utf8Error),

    /// XML attribute error.
    #[error("XML attribute error")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),

    /// Encoding error during XML parsing.
    #[error("encoding error")]
    Encoding(#[from] quick_xml::encoding::EncodingError),
}

/// Error from Confluence API operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfluenceError {
    /// HTTP request failed (network error, timeout, etc).
    #[error("HTTP request failed")]
    HttpRequest(#[from] ureq::Error),

    /// HTTP response error (server returned error status).
    #[error("HTTP error: {status} - {body}")]
    HttpResponse {
        /// HTTP status code.
        status: u16,
        /// Response body (may contain error details).
        body: String,
    },

    /// RSA key loading/parsing error.
    #[error("RSA key error")]
    RsaKey(#[from] RsaKeyError),

    /// I/O error.
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error")]
    Json(#[from] serde_json::Error),

    /// Comment preservation error.
    #[error("comment preservation error")]
    CommentPreservation(#[from] CommentPreservationError),

    /// OAuth token generation error.
    #[error("OAuth error: {0}")]
    OAuth(String),
}

/// RSA key loading/parsing error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RsaKeyError {
    /// Invalid UTF-8 in key file.
    #[error("invalid UTF-8 in key")]
    InvalidUtf8(#[from] Utf8Error),

    /// PKCS#1 key parsing error.
    #[error("PKCS#1 key error")]
    Pkcs1(#[from] rsa::pkcs1::Error),

    /// PKCS#8 key parsing error (returned when both formats fail).
    #[error("PKCS#8 key error")]
    Pkcs8(#[from] rsa::pkcs8::Error),
}
