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
    Arc::new(|info: &RenderedDiagramInfo| {
        // Width only: Confluence scales an image proportionally from a single
        // dimension, so supplying a height too could only ever distort it.
        format!(
            r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
            info.display_width(),
            info.filename()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator formats the display width it is given; the DPI correction
    /// happens in rw-kroki before the info reaches here, so this can no longer
    /// scale by the wrong DPI (or forget to).
    #[test]
    fn emits_the_display_width_it_is_given() {
        let generator = confluence_tag_generator();
        let info = RenderedDiagramInfo::new("diagram_abc123.png".to_owned(), 200);
        assert_eq!(
            generator(&info),
            r#"<ac:image ac:width="200"><ri:attachment ri:filename="diagram_abc123.png" /></ac:image>"#
        );
    }

    /// Height is deliberately absent: Confluence scales proportionally from a
    /// single dimension, so emitting both could only ever distort the diagram.
    #[test]
    fn emits_width_but_never_height() {
        let generator = confluence_tag_generator();
        let info = RenderedDiagramInfo::new("test.png".to_owned(), 300);
        let tag = generator(&info);
        assert!(tag.contains(r#"ac:width="300""#), "{tag}");
        assert!(!tag.contains("height"), "{tag}");
    }
}
