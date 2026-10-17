//! Error types for Confluence integration.

use std::str::Utf8Error;

/// Error during comment preservation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CommentPreservationError {
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
