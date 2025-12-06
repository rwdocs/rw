//! High-performance markdown renderer for Docstage.
//!
//! This crate provides a markdown parser with multiple output formats:
//! - Confluence storage format (XHTML)
//! - Semantic HTML5 with syntax highlighting

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
pub use kroki::{DiagramRequest, RenderError, RenderedDiagram, render_all};
pub use plantuml::{load_config_file, prepare_diagram_source};
pub use plantuml_filter::{ExtractedDiagram, PlantUmlFilter};
