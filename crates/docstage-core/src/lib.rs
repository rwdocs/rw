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
//! # `PlantUML` Diagram Support
//!
//! When converting to Confluence format, `PlantUML` code blocks are automatically
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
//! # Architecture
//!
//! - [`MarkdownConverter`]: Main entry point with builder pattern
//! - [`HtmlRenderer`]: Event-based HTML5 renderer
//! - [`ConfluenceRenderer`]: Event-based Confluence XHTML renderer
//! - [`PlantUmlFilter`]: Iterator adapter for diagram extraction
//! - [`render_all`]: Parallel diagram rendering via Kroki

mod confluence;
mod converter;
mod html;
mod kroki;
mod plantuml;
mod plantuml_filter;

pub use confluence::{ConfluenceRenderer, RenderResult};
pub use converter::{
    ConvertResult, DiagramInfo, HtmlConvertResult, MarkdownConverter, create_image_tag,
};
pub use html::{HtmlRenderResult, HtmlRenderer, TocEntry};
pub use kroki::{
    DiagramError, DiagramErrorKind, DiagramRequest, RenderError, RenderedDiagram, render_all,
};
pub use plantuml::{DEFAULT_DPI, load_config_file, prepare_diagram_source};
pub use plantuml_filter::{ExtractedDiagram, PlantUmlFilter};
