//! Diagram rendering via Kroki for docstage.
//!
//! This crate provides diagram extraction and rendering for markdown documents:
//! - `DiagramProcessor` implements `CodeBlockProcessor` for extracting diagrams
//! - Parallel rendering via Kroki service (PlantUML, Mermaid, GraphViz, etc.)
//! - PlantUML preprocessing with `!include` resolution and DPI configuration
//!
//! # Architecture
//!
//! The crate is organized into modules:
//! - [`language`]: Diagram type definitions (`DiagramLanguage`, `DiagramFormat`, `ExtractedDiagram`)
//! - [`processor`]: `DiagramProcessor` implementing `CodeBlockProcessor` trait
//! - [`kroki`]: Parallel HTTP rendering via Kroki service
//! - [`plantuml`]: PlantUML-specific preprocessing
//!
//! # Example
//!
//! ```ignore
//! use pulldown_cmark::Parser;
//! use docstage_diagrams::{DiagramProcessor, to_extracted_diagrams};
//! use docstage_renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
//! let parser = Parser::new(markdown);
//! let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_processor(DiagramProcessor::new());
//!
//! let result = renderer.render(parser);
//! let diagrams = to_extracted_diagrams(&renderer.extracted_code_blocks());
//! ```

mod kroki;
mod language;
mod plantuml;
mod processor;

pub use kroki::{
    DiagramError, DiagramErrorKind, DiagramRequest, PartialRenderResult, RenderError,
    RenderedDiagram, RenderedPngDataUri, RenderedSvg, render_all, render_all_png_data_uri,
    render_all_png_data_uri_partial, render_all_svg, render_all_svg_partial,
};
pub use language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};
pub use plantuml::{DEFAULT_DPI, PrepareResult, load_config_file, prepare_diagram_source};
pub use processor::{DiagramProcessor, to_extracted_diagram, to_extracted_diagrams};
