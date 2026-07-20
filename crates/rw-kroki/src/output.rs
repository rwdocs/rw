//! Output mode abstraction for diagram rendering.
//!
//! This module provides [`DiagramOutput`] for controlling how diagrams are rendered:
//! - [`Inline`](DiagramOutput::Inline): Embed diagrams directly in HTML (default)
//! - [`Files`](DiagramOutput::Files): Save diagrams to files with custom tag generation

use std::path::PathBuf;
use std::sync::Arc;

/// Information about a rendered diagram for tag generation.
#[derive(Debug)]
pub struct RenderedDiagramInfo {
    filename: String,
    display_width: u32,
}

impl RenderedDiagramInfo {
    /// Create a new rendered diagram info.
    ///
    /// `display_width` is the width the diagram should occupy on the page, not
    /// the pixel width of the file. High-DPI diagrams are rendered larger than
    /// they are shown, and the correction is applied here rather than passed on
    /// to the tag generator: a generator that forgot it, or applied it with the
    /// configured DPI where the language never received one, emitted diagrams
    /// at the wrong size.
    #[must_use]
    pub fn new(filename: String, display_width: u32) -> Self {
        Self {
            filename,
            display_width,
        }
    }

    /// Filename of the diagram (e.g., "`diagram_abc123.png`").
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Width the diagram should be displayed at, in CSS pixels.
    #[must_use]
    pub fn display_width(&self) -> u32 {
        self.display_width
    }
}

/// Callback that generates an HTML tag for a rendered diagram.
///
/// Arguments: `(info)` → HTML string to fill the diagram's reserved hole. The
/// info carries display dimensions, so a generator never sees a render DPI and
/// cannot scale by the wrong one.
pub type TagGenerator = Arc<dyn Fn(&RenderedDiagramInfo) -> String + Send + Sync>;

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
    use std::assert_matches;

    fn img_tag_generator(prefix: &str) -> TagGenerator {
        let prefix = prefix.to_owned();
        Arc::new(move |info: &RenderedDiagramInfo| {
            format!(
                r#"<img src="{}{}" width="{}" alt="diagram">"#,
                prefix,
                info.filename(),
                info.display_width()
            )
        })
    }

    /// The info carries a display width, so a generator just formats it — the
    /// DPI correction happened before it was constructed (see `scale`).
    #[test]
    fn test_img_tag_generator() {
        let generator = img_tag_generator("/diagrams/");
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 200);
        assert_eq!(
            generator(&info),
            r#"<img src="/diagrams/test.png" width="200" alt="diagram">"#
        );
    }

    #[test]
    fn test_figure_tag_generator() {
        let prefix = "/diagrams/".to_owned();
        let generator: TagGenerator = Arc::new(move |info: &RenderedDiagramInfo| {
            format!(
                r#"<figure class="diagram"><img src="{}{}" width="{}" alt="diagram"></figure>"#,
                prefix,
                info.filename(),
                info.display_width()
            )
        });
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 200);
        assert_eq!(
            generator(&info),
            r#"<figure class="diagram"><img src="/diagrams/test.png" width="200" alt="diagram"></figure>"#
        );
    }

    #[test]
    fn test_diagram_output_default() {
        let output = DiagramOutput::default();
        assert_matches!(output, DiagramOutput::Inline);
    }

    #[test]
    fn info_reports_what_it_was_built_with() {
        let info = RenderedDiagramInfo::new("d.png".to_owned(), 300);
        assert_eq!(info.filename(), "d.png");
        assert_eq!(info.display_width(), 300);
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
