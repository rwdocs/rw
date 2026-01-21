//! Error types for comment preservation.

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
