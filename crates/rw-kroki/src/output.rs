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
    filename: String,
    width: u32,
    height: u32,
}

impl RenderedDiagramInfo {
    /// Create a new rendered diagram info.
    #[must_use]
    pub fn new(filename: String, width: u32, height: u32) -> Self {
        Self {
            filename,
            width,
            height,
        }
    }

    /// Filename of the diagram (e.g., "`diagram_abc123.png`").
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Physical width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Physical height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

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

/// Callback that generates an HTML tag for a rendered diagram.
///
/// Arguments: `(info, dpi)` → HTML string to replace the diagram placeholder.
pub type TagGenerator = Arc<dyn Fn(&RenderedDiagramInfo, u32) -> String + Send + Sync>;

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
        /// Callback to generate HTML tag from rendered diagram info.
        tag_generator: TagGenerator,
    },
}

impl std::fmt::Debug for DiagramOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inline => write!(f, "DiagramOutput::Inline"),
            Self::Files { output_dir, .. } => f
                .debug_struct("DiagramOutput::Files")
                .field("output_dir", output_dir)
                .field("tag_generator", &"<TagGenerator>")
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img_tag_generator(prefix: &str) -> TagGenerator {
        let prefix = prefix.to_owned();
        Arc::new(move |info: &RenderedDiagramInfo, dpi: u32| {
            format!(
                r#"<img src="{}{}" width="{}" alt="diagram">"#,
                prefix,
                info.filename(),
                info.display_width(dpi)
            )
        })
    }

    #[test]
    fn test_img_tag_generator() {
        let generator = img_tag_generator("/diagrams/");
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 400, 200);
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        let tag = generator(&info, 192);
        assert_eq!(
            tag,
            r#"<img src="/diagrams/test.png" width="200" alt="diagram">"#
        );
    }

    #[test]
    fn test_img_tag_generator_96_dpi() {
        let generator = img_tag_generator("/assets/");
        let info = RenderedDiagramInfo::new("diagram.png".to_owned(), 300, 150);
        // At 96 DPI, width unchanged: 300 * 96 / 96 = 300
        let tag = generator(&info, 96);
        assert_eq!(
            tag,
            r#"<img src="/assets/diagram.png" width="300" alt="diagram">"#
        );
    }

    #[test]
    fn test_figure_tag_generator() {
        let prefix = "/diagrams/".to_owned();
        let generator: TagGenerator = Arc::new(move |info: &RenderedDiagramInfo, dpi: u32| {
            format!(
                r#"<figure class="diagram"><img src="{}{}" width="{}" alt="diagram"></figure>"#,
                prefix,
                info.filename(),
                info.display_width(dpi)
            )
        });
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 400, 200);
        let tag = generator(&info, 192);
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
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 400, 200);
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        assert_eq!(info.display_width(192), 200);
    }

    #[test]
    fn test_display_width_96_dpi() {
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 300, 150);
        // At 96 DPI, width unchanged: 300 * 96 / 96 = 300
        assert_eq!(info.display_width(96), 300);
    }

    #[test]
    fn test_display_height_192_dpi() {
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 400, 200);
        // At 192 DPI (2x), height should be halved: 200 * 96 / 192 = 100
        assert_eq!(info.display_height(192), 100);
    }

    #[test]
    fn test_diagram_output_debug_inline() {
        let output = DiagramOutput::Inline;
        assert_eq!(format!("{output:?}"), "DiagramOutput::Inline");
    }

    #[test]
    fn test_diagram_output_debug_files() {
        let output = DiagramOutput::Files {
            output_dir: PathBuf::from("/tmp/diagrams"),
            tag_generator: img_tag_generator("/assets/"),
        };
        let debug = format!("{output:?}");
        assert!(debug.contains("DiagramOutput::Files"));
        assert!(debug.contains("/tmp/diagrams"));
        assert!(debug.contains("<TagGenerator>"));
    }
}
