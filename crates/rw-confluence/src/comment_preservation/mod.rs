//! Comment preservation for Confluence page updates.
//!
//! Carries `ac:inline-comment-marker` elements from a page's current storage
//! XHTML into freshly rendered XHTML, so republishing does not orphan inline
//! comments. Reached through [`render`](fn@crate::render) when
//! [`RenderOptions::current_xhtml`](crate::RenderOptions::current_xhtml) is set.
//!
//! Both documents are parsed into trees ([`parser`]), nodes are paired by text
//! similarity ([`matcher`]), each marker is re-inserted around the same text in
//! its node's counterpart or, failing that, anywhere in the new tree
//! ([`transfer`](mod@transfer)), and the new tree is written back out ([`serializer`]).

mod entities;
mod matcher;
mod parser;
mod serializer;
mod transfer;
mod tree;

use parser::parse;
use serializer::serialize;
use transfer::transfer;

/// Comment that could not be placed in new HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmatchedComment {
    /// Comment reference ID.
    pub ref_id: String,
    /// Text content the marker was wrapping.
    pub text: String,
}

/// Result of comment preservation operation.
#[derive(Debug)]
pub(crate) struct PreserveResult {
    /// HTML with preserved comment markers.
    pub html: String,
    /// Comments that could not be placed in the new HTML.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Non-fatal preservation warnings (e.g., parse failure → fell back to
    /// new HTML unchanged). Structured failures like "marker could not be
    /// placed" are surfaced via `unmatched_comments`, not here.
    pub warnings: Vec<String>,
}

/// Preserve the inline comment markers of `old_html`, the page's current body,
/// in `new_html`, the fresh render.
///
/// If either document cannot be parsed, returns `new_html` unchanged with one
/// warning saying why.
pub(crate) fn preserve_comments(old_html: &str, new_html: &str) -> PreserveResult {
    try_preserve_comments(old_html, new_html).unwrap_or_else(|e| PreserveResult {
        html: new_html.to_owned(),
        unmatched_comments: vec![],
        warnings: vec![format!("comment preservation skipped: {e}")],
    })
}

/// The error is the warning's text, naming which document failed and why.
fn try_preserve_comments(old_html: &str, new_html: &str) -> Result<PreserveResult, String> {
    let old_tree = parse(old_html).map_err(|e| format!("cannot parse current XHTML: {e}"))?;
    let mut new_tree = parse(new_html).map_err(|e| format!("cannot parse rendered XHTML: {e}"))?;

    let unmatched_comments = transfer(&old_tree, &mut new_tree);

    // Serialized even with nothing transferred, so the output does not depend
    // on whether the current page had markers.
    Ok(PreserveResult {
        html: serialize(&new_tree),
        unmatched_comments,
        warnings: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preserve_comments_simple_case() {
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>"#;
        let new_html = "<p>marked text</p>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.html.contains("ac:inline-comment-marker"));
        assert!(result.html.contains(r#"ac:ref="abc""#));
    }

    #[test]
    fn test_preserve_comments_marker_in_tail() {
        let old_html = r#"<li><code>x</code> <ac:inline-comment-marker ac:ref="id">marked</ac:inline-comment-marker>, rest</li>"#;
        let new_html = "<li><code>x</code> marked, rest</li>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.html.contains("ac:inline-comment-marker"));
    }

    #[test]
    fn test_preserve_comments_cyrillic_text() {
        let old_html = r#"<li><code>gateway</code> <ac:inline-comment-marker ac:ref="xyz">проверяет тип</ac:inline-comment-marker>, активность</li>"#;
        let new_html = "<li><code>gateway</code> проверяет тип, активность</li>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.html.contains("проверяет тип"));
        assert!(result.html.contains("ac:inline-comment-marker"));
    }

    #[test]
    fn test_preserve_comments_multiple_markers() {
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="a">first paragraph text</ac:inline-comment-marker></p><p><ac:inline-comment-marker ac:ref="b">second paragraph text</ac:inline-comment-marker></p>"#;
        let new_html = "<p>first paragraph text</p><p>second paragraph text</p>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
        assert_eq!(result.html.matches("<ac:inline-comment-marker").count(), 2);
    }

    #[test]
    fn test_preserve_comments_unmatched_when_text_removed() {
        let old_html = r#"<p>Some text with <ac:inline-comment-marker ac:ref="abc">original word</ac:inline-comment-marker> in it</p>"#;
        let new_html = "<p>Some text with different word in it</p>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments.len(), 1);
        assert_eq!(result.unmatched_comments[0].ref_id, "abc");
        assert_eq!(result.unmatched_comments[0].text, "original word");
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_preserve_comments_unmatched_when_parent_not_matched() {
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="xyz">Original sentence here</ac:inline-comment-marker></p>"#;
        let new_html = "<p>Completely different content</p>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments.len(), 1);
        assert_eq!(result.unmatched_comments[0].ref_id, "xyz");
        assert_eq!(result.unmatched_comments[0].text, "Original sentence here");
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_preserve_comments_fallback_global_search() {
        let old_html = r#"<table><tbody>
            <tr><td><code>old-text</code></td><td><code>changed-value</code></td></tr>
            <tr><td><code><ac:inline-comment-marker ac:ref="marker-id">keep-this</ac:inline-comment-marker></code></td><td><code>same</code></td></tr>
        </tbody></table>"#;

        let new_html = r"<table><tbody>
            <tr><td><code>old-text</code></td><td><code>completely-different-value-here</code></td></tr>
            <tr><td><code>keep-this</code></td><td><code>same</code></td></tr>
        </tbody></table>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.html.contains("inline-comment-marker"));
        assert!(result.html.contains(r#"ac:ref="marker-id""#));
        assert!(result.html.contains("keep-this"));
    }

    #[test]
    fn test_preserve_comments_keeps_attribute_source_order() {
        let new_html = r#"<ac:structured-macro ac:name="code" ac:schema-version="1" zeta="1" alpha="2" mu="3" beta="4" omega="5" gamma="6" delta="7" epsilon="8"><ac:parameter ac:name="language">rust</ac:parameter></ac:structured-macro>"#;

        let result = preserve_comments("<p>old</p>", new_html);

        assert_eq!(result.warnings, [] as [String; 0]);
        assert_eq!(result.html, new_html);
    }

    #[test]
    fn test_preserve_comments_parse_failure_surfaces_warning() {
        // Malformed XML that quick-xml cannot parse.
        let old_html = "<p>unclosed paragraph";
        let new_html = "<p>new content</p>";

        let result = preserve_comments(old_html, new_html);

        // Falls back to new HTML unchanged.
        assert_eq!(result.html, new_html);
        assert_eq!(result.unmatched_comments, [] as [UnmatchedComment; 0]);

        // One warning, naming the document and the parser's reason.
        assert_eq!(result.warnings.len(), 1, "got: {:?}", result.warnings);
        assert_eq!(
            result.warnings[0],
            "comment preservation skipped: cannot parse current XHTML: \
             ill-formed document: start tag not closed: `</p>` not found before end of input"
        );
    }
}
