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
//!
//! # Architecture
//!
//! - [`MarkdownConverter`]: Main entry point with builder pattern
//! - [`HtmlRenderer`]: Event-based HTML5 renderer
//! - [`ConfluenceRenderer`]: Event-based Confluence XHTML renderer
//! - [`DiagramFilter`]: Iterator adapter for diagram extraction
//! - [`render_all`]: Parallel PNG diagram rendering via Kroki
//! - [`render_all_svg`]: Parallel SVG diagram rendering via Kroki

mod confluence;
mod converter;
mod diagram_filter;
mod html;
mod kroki;
mod plantuml;
mod util;

pub use confluence::{ConfluenceRenderer, RenderResult};
pub use converter::{
    ConvertResult, DiagramInfo, ExtractResult, HtmlConvertResult, MarkdownConverter,
    PreparedDiagram, create_image_tag,
};
pub use diagram_filter::{DiagramFilter, DiagramFormat, DiagramLanguage, ExtractedDiagram};
pub use html::{HtmlRenderResult, HtmlRenderer, TocEntry, escape_html};
pub use kroki::{
    DiagramError, DiagramErrorKind, DiagramRequest, PartialRenderResult, RenderError,
    RenderedDiagram, RenderedPngDataUri, RenderedSvg, render_all, render_all_png_data_uri,
    render_all_png_data_uri_partial, render_all_svg, render_all_svg_partial,
};
pub use plantuml::{DEFAULT_DPI, load_config_file, prepare_diagram_source};
