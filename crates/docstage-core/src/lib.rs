//! Confluence integration for Docstage.
//!
//! This crate provides:
//! - **Confluence conversion**: [`MarkdownConverter`] for Confluence XHTML storage format
//! - **Page updating**: [`PageUpdater`] for Confluence page updates
//!
//! # Confluence Conversion
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new()
//!     .prepend_toc(true)
//!     .extract_title(true);
//!
//! let result = converter.convert(
//!     "# Hello\n\n```plantuml\nA -> B\n```",
//!     Some("https://kroki.io"),
//!     Some(Path::new("/tmp/diagrams")),
//! );
//! ```

mod confluence_tags;
mod converter;
pub mod updater;

#[allow(deprecated)]
pub use converter::{ConvertResult, MarkdownConverter};
pub use docstage_renderer::RenderResult;
