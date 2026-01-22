//! Error types for Confluence integration.

use std::str::Utf8Error;

/// Error during comment preservation.
#[derive(Debug, thiserror::Error)]
pub enum CommentPreservationError {
    /// XML parsing error.
    #[error("XML parse error: {0}")]
    XmlParse(#[from] quick_xml::Error),

    /// UTF-8 decoding error.
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] Utf8Error),

    /// XML attribute error.
    #[error("XML attribute error: {0}")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),
}

/// Error from Confluence API operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfluenceError {
    /// HTTP request error.
    #[error("HTTP error: {status} - {body}")]
    Http { status: u16, body: String },

    /// RSA key loading/parsing error.
    #[error("RSA key error: {0}")]
    RsaKey(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(String),

    /// Comment preservation error.
    #[error("Comment preservation error: {0}")]
    CommentPreservation(#[from] CommentPreservationError),

    /// OAuth token generation error.
    #[error("OAuth error: {0}")]
    OAuth(String),
}

impl From<serde_json::Error> for ConfluenceError {
    fn from(e: serde_json::Error) -> Self {
        ConfluenceError::Json(e.to_string())
    }
}

impl From<ureq::Error> for ConfluenceError {
    fn from(e: ureq::Error) -> Self {
        ConfluenceError::Http {
            status: 0,
            body: e.to_string(),
        }
    }
}
