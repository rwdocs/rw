//! High-performance markdown renderer for Docstage.
//!
//! This crate provides a `CommonMark` parser with multiple output formats:
//! - **Confluence XHTML**: Storage format for Confluence REST API
//! - **Semantic HTML5**: Clean HTML with table of contents and heading anchors
//!
//! # Quick Start
//!
//! ```ignore
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new().extract_title(true);
//! let result = converter.convert_html("# Hello\n\nWorld!");
//! assert_eq!(result.title, Some("Hello".to_string()));
//! ```
//!
//! # Diagram Support
//!
//! Multiple diagram languages are supported via Kroki: `PlantUML`, Mermaid, `GraphViz`, etc.
//!
//! When converting to Confluence format, diagram code blocks are automatically
//! rendered via the Kroki service and replaced with image macros:
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new();
//! let result = converter.convert(
//!     "```plantuml\n@startuml\nA -> B\n@enduml\n```",
//!     "https://kroki.io",
//!     Path::new("/tmp/diagrams"),
//! )?;
//! ```
//!
//! For HTML output with rendered diagrams:
//!
//! ```ignore
//! let result = converter.convert_html_with_diagrams(
//!     "```mermaid\ngraph TD\n  A --> B\n```",
//!     "https://kroki.io",
//! )?;
//! // Result contains inline SVG diagrams
//! ```

mod confluence_tags;
mod converter;
mod page_cache;
mod page_renderer;

pub use converter::{ConvertResult, HtmlConvertResult, MarkdownConverter};
pub use page_cache::{CacheEntry, CachedMetadata, FilePageCache, NullPageCache, PageCache};
pub use page_renderer::{PageRenderResult, PageRenderer, PageRendererConfig, RenderError};
