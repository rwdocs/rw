//! Confluence-specific diagram tag generation.
//!
//! This module provides [`ConfluenceTagGenerator`] for generating Confluence
//! image macros from rendered diagrams.

use rw_diagrams::{DiagramTagGenerator, RenderedDiagramInfo};

/// Confluence image macro tag generator.
///
/// Generates: `<ac:image ac:width="{w}"><ri:attachment ri:filename="{f}" /></ac:image>`
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfluenceTagGenerator;

impl DiagramTagGenerator for ConfluenceTagGenerator {
    fn generate_tag(&self, info: &RenderedDiagramInfo, dpi: u32) -> String {
        format!(
            r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
            info.display_width(dpi),
            info.filename
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confluence_tag_generator_192_dpi() {
        let generator = ConfluenceTagGenerator;
        let info = RenderedDiagramInfo {
            filename: "diagram_abc123.png".to_owned(),
            width: 400,
            height: 200,
        };
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        let tag = generator.generate_tag(&info, 192);
        assert_eq!(
            tag,
            r#"<ac:image ac:width="200"><ri:attachment ri:filename="diagram_abc123.png" /></ac:image>"#
        );
    }

    #[test]
    fn test_confluence_tag_generator_96_dpi() {
        let generator = ConfluenceTagGenerator;
        let info = RenderedDiagramInfo {
            filename: "test.png".to_owned(),
            width: 300,
            height: 150,
        };
        // At 96 DPI, width unchanged
        let tag = generator.generate_tag(&info, 96);
        assert_eq!(
            tag,
            r#"<ac:image ac:width="300"><ri:attachment ri:filename="test.png" /></ac:image>"#
        );
    }
}
