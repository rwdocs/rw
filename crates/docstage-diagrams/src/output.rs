//! Output mode abstraction for diagram rendering.
//!
//! This module provides [`DiagramOutput`] for controlling how diagrams are rendered:
//! - [`Inline`](DiagramOutput::Inline): Embed diagrams directly in HTML (default)
//! - [`Files`](DiagramOutput::Files): Save diagrams to files with custom tag generation

use std::path::PathBuf;
use std::sync::Arc;

use crate::consts::STANDARD_DPI;

/// Information about a rendered diagram for tag generation.
#[derive(Debug)]
pub struct RenderedDiagramInfo {
    /// Filename of the diagram (e.g., "diagram_abc123.png").
    pub filename: String,
    /// Physical width in pixels.
    pub width: u32,
    /// Physical height in pixels.
    pub height: u32,
}

impl RenderedDiagramInfo {
    /// Calculate display width for a given rendering DPI.
    ///
    /// High-DPI diagrams (e.g., 192 DPI) should be displayed at half their
    /// physical size to appear crisp on retina displays.
    #[must_use]
    pub fn display_width(&self, dpi: u32) -> u32 {
        self.width * STANDARD_DPI / dpi
    }

    /// Calculate display height for a given rendering DPI.
    #[must_use]
    pub fn display_height(&self, dpi: u32) -> u32 {
        self.height * STANDARD_DPI / dpi
    }
}

/// Trait for generating HTML tags from rendered diagram info.
///
/// Implement this trait to customize how diagrams are embedded in HTML.
/// The generated tag replaces the diagram placeholder in the output.
pub trait DiagramTagGenerator: Send + Sync {
    /// Generate an HTML tag for a rendered diagram.
    ///
    /// # Arguments
    ///
    /// - `info` - Information about the rendered diagram
    /// - `dpi` - DPI used for rendering (for display width calculation)
    ///
    /// # Returns
    ///
    /// HTML string to replace the diagram placeholder.
    fn generate_tag(&self, info: &RenderedDiagramInfo, dpi: u32) -> String;
}

/// Output mode for diagram rendering.
#[derive(Default)]
pub enum DiagramOutput {
    /// Embed diagrams inline in HTML (default).
    ///
    /// - SVG: Inline SVG content with dimension scaling
    /// - PNG: Base64 data URI in `<img>` tag
    ///
    /// Both wrapped in `<figure class="diagram">`.
    #[default]
    Inline,

    /// Save diagrams to files and use custom tag generator.
    Files {
        /// Directory to save rendered diagram files.
        output_dir: PathBuf,
        /// Function to generate HTML tag from rendered diagram info.
        tag_generator: Arc<dyn DiagramTagGenerator>,
    },
}

/// Simple `<img>` tag generator for static sites.
///
/// Generates: `<img src="{prefix}{filename}" width="{display_width}" alt="diagram">`
#[derive(Debug)]
pub struct ImgTagGenerator {
    /// Path prefix (e.g., "/assets/diagrams/").
    pub path_prefix: String,
}

impl ImgTagGenerator {
    /// Create a new img tag generator with the given path prefix.
    #[must_use]
    pub fn new(path_prefix: impl Into<String>) -> Self {
        Self {
            path_prefix: path_prefix.into(),
        }
    }
}

impl DiagramTagGenerator for ImgTagGenerator {
    fn generate_tag(&self, info: &RenderedDiagramInfo, dpi: u32) -> String {
        format!(
            r#"<img src="{}{}" width="{}" alt="diagram">"#,
            self.path_prefix,
            info.filename,
            info.display_width(dpi)
        )
    }
}

/// Figure-wrapped tag generator.
///
/// Generates: `<figure class="diagram"><img src="..." width="..." alt="diagram"></figure>`
#[derive(Debug)]
pub struct FigureTagGenerator {
    /// Path prefix (e.g., "/assets/diagrams/").
    pub path_prefix: String,
}

impl FigureTagGenerator {
    /// Create a new figure tag generator with the given path prefix.
    #[must_use]
    pub fn new(path_prefix: impl Into<String>) -> Self {
        Self {
            path_prefix: path_prefix.into(),
        }
    }
}

impl DiagramTagGenerator for FigureTagGenerator {
    fn generate_tag(&self, info: &RenderedDiagramInfo, dpi: u32) -> String {
        format!(
            r#"<figure class="diagram"><img src="{}{}" width="{}" alt="diagram"></figure>"#,
            self.path_prefix,
            info.filename,
            info.display_width(dpi)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_img_tag_generator() {
        let generator = ImgTagGenerator::new("/diagrams/");
        let info = RenderedDiagramInfo {
            filename: "test.png".to_string(),
            width: 400,
            height: 200,
        };
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        let tag = generator.generate_tag(&info, 192);
        assert_eq!(
            tag,
            r#"<img src="/diagrams/test.png" width="200" alt="diagram">"#
        );
    }

    #[test]
    fn test_img_tag_generator_96_dpi() {
        let generator = ImgTagGenerator::new("/assets/");
        let info = RenderedDiagramInfo {
            filename: "diagram.png".to_string(),
            width: 300,
            height: 150,
        };
        // At 96 DPI, width unchanged: 300 * 96 / 96 = 300
        let tag = generator.generate_tag(&info, 96);
        assert_eq!(
            tag,
            r#"<img src="/assets/diagram.png" width="300" alt="diagram">"#
        );
    }

    #[test]
    fn test_figure_tag_generator() {
        let generator = FigureTagGenerator::new("/diagrams/");
        let info = RenderedDiagramInfo {
            filename: "test.png".to_string(),
            width: 400,
            height: 200,
        };
        let tag = generator.generate_tag(&info, 192);
        assert_eq!(
            tag,
            r#"<figure class="diagram"><img src="/diagrams/test.png" width="200" alt="diagram"></figure>"#
        );
    }

    #[test]
    fn test_diagram_output_default() {
        let output = DiagramOutput::default();
        assert!(matches!(output, DiagramOutput::Inline));
    }

    #[test]
    fn test_display_width_192_dpi() {
        let info = RenderedDiagramInfo {
            filename: "test.png".to_string(),
            width: 400,
            height: 200,
        };
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        assert_eq!(info.display_width(192), 200);
    }

    #[test]
    fn test_display_width_96_dpi() {
        let info = RenderedDiagramInfo {
            filename: "test.png".to_string(),
            width: 300,
            height: 150,
        };
        // At 96 DPI, width unchanged: 300 * 96 / 96 = 300
        assert_eq!(info.display_width(96), 300);
    }

    #[test]
    fn test_display_height_192_dpi() {
        let info = RenderedDiagramInfo {
            filename: "test.png".to_string(),
            width: 400,
            height: 200,
        };
        // At 192 DPI (2x), height should be halved: 200 * 96 / 192 = 100
        assert_eq!(info.display_height(192), 100);
    }
}
