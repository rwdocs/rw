//! Trait-based markdown renderer with pluggable backends.
//!
//! This crate provides a generic [`MarkdownRenderer`] that can produce
//! HTML output using the [`RenderBackend`] trait.
//!
//! # Architecture
//!
//! The renderer uses a trait-based abstraction to handle format-specific differences:
//! - [`HtmlBackend`]: Produces semantic HTML5 with relative link resolution
//!
//! For Confluence XHTML storage format, use the `rw-confluence` crate.
//!
//! Shared functionality (tables, lists, inline formatting) is handled by the
//! generic renderer, while format-specific elements (code blocks, blockquotes,
//! images) are delegated to the backend.
//!
//! # Example
//!
//! ```
//! use pulldown_cmark::Parser;
//! use rw_renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "# Hello\n\n**Bold** text";
//! let parser = Parser::new(markdown);
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_title_extraction()
//!     .render(parser);
//! ```

mod backend;
mod code_block;
pub mod directive;
mod html;
mod renderer;
mod state;
pub(crate) mod tabs;
mod util;

pub use backend::{AlertKind, RenderBackend};
pub use code_block::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
pub use html::HtmlBackend;
pub use renderer::{MarkdownRenderer, RenderResult};
pub use state::{TocEntry, escape_html};
pub use tabs::{TabMetadata, TabsDirective, TabsGroup, TabsPreprocessor, TabsProcessor};
pub use util::relative_path;
