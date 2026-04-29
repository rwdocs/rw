//! Confluence-specific diagram tag generation.
//!
//! This module provides [`confluence_tag_generator`] for generating Confluence
//! image macros from rendered diagrams.

use std::sync::Arc;

use rw_kroki::{RenderedDiagramInfo, TagGenerator};

/// Create a Confluence image macro tag generator.
///
/// Generates: `<ac:image ac:width="{w}"><ri:attachment ri:filename="{f}" /></ac:image>`
pub(crate) fn confluence_tag_generator() -> TagGenerator {
    Arc::new(|info: &RenderedDiagramInfo, dpi: u32| {
        format!(
            r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
            info.display_width(dpi),
            info.filename()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confluence_tag_generator_192_dpi() {
        let generator = confluence_tag_generator();
        let info = RenderedDiagramInfo::new("diagram_abc123.png".to_owned(), 400, 200);
        // At 192 DPI (2x), width should be halved: 400 * 96 / 192 = 200
        let tag = generator(&info, 192);
        assert_eq!(
            tag,
            r#"<ac:image ac:width="200"><ri:attachment ri:filename="diagram_abc123.png" /></ac:image>"#
        );
    }

    #[test]
    fn test_confluence_tag_generator_96_dpi() {
        let generator = confluence_tag_generator();
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 300, 150);
        // At 96 DPI, width unchanged
        let tag = generator(&info, 96);
        assert_eq!(
            tag,
            r#"<ac:image ac:width="300"><ri:attachment ri:filename="test.png" /></ac:image>"#
        );
    }
}
