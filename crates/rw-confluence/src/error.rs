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

/// Error from `rw-confluence` operations.
///
/// Covers bundle-write I/O (page.xhtml, diagram PNGs) and
/// comment-preservation parse failures. This crate does not perform HTTP
/// or OAuth — see crate root for the scope boundary.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfluenceError {
    /// I/O error (creating the output directory, writing `page.xhtml` or
    /// PNGs).
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// Comment preservation failed (parse error during marker transfer).
    #[error("comment preservation error")]
    CommentPreservation(#[from] CommentPreservationError),
}
