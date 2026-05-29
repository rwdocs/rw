//! Comment preservation for Confluence page updates.
//!
//! This module preserves inline comment markers when updating Confluence pages from markdown.
//! It uses tree-based comparison to match content between old and new HTML and transfers
//! comment markers to matching positions.
//!
//! # Architecture
//!
//! The module is organized into:
//! - [`tree`]: Tree node representation with text signature and marker detection
//! - [`parser`]: XML parser with Confluence namespace handling
//! - [`matcher`]: Tree matcher with 80% similarity threshold
//! - [`transfer`]: Comment marker transfer with global fallback
//! - [`serializer`]: XML serializer with CDATA support
//! - [`entities`]: HTML entity to Unicode conversion
//!
//! # Usage
//!
//! Call `preserve_comments(old_html, new_html)` with the existing Confluence HTML
//! and the freshly rendered HTML. The returned `PreserveResult` contains the new
//! HTML with `ac:inline-comment-marker` elements transferred from matching nodes.

#![allow(clippy::similar_names)] // matcher/matches, old_tree/new_tree are intentional naming patterns

mod entities;
mod matcher;
mod parser;
mod serializer;
mod transfer;
mod tree;

use matcher::TreeMatcher;
use parser::ConfluenceXmlParser;
use serializer::ConfluenceXmlSerializer;
use transfer::CommentMarkerTransfer;

use crate::error::CommentPreservationError;

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
pub struct PreserveResult {
    /// HTML with preserved comment markers.
    pub html: String,
    /// Comments that could not be placed in the new HTML.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Non-fatal preservation warnings (e.g., parse failure → fell back to
    /// new HTML unchanged). Structured failures like "marker could not be
    /// placed" are surfaced via `unmatched_comments`, not here.
    pub warnings: Vec<String>,
}

/// Preserve inline comment markers from old HTML in new HTML.
///
/// This function:
/// 1. Parses both HTML strings to tree structures
/// 2. Matches nodes between trees using text similarity (80% threshold)
/// 3. Transfers comment markers from matched old nodes to new nodes
/// 4. Falls back to global text search for unmatched parent nodes
/// 5. Returns the modified HTML with preserved markers
///
/// # Arguments
///
/// * `old_html` - Current page HTML with comment markers
/// * `new_html` - New HTML from markdown conversion
///
/// # Returns
///
/// `PreserveResult` containing the modified HTML and any unmatched comments.
///
/// # Failure mode
///
/// On parse failure, returns the new HTML unchanged plus a warning in
/// `warnings`. The error is also logged via `tracing`.
pub fn preserve_comments(old_html: &str, new_html: &str) -> PreserveResult {
    tracing::info!("Starting comment preservation");
    tracing::debug!(old_html_len = old_html.len(), "Old HTML length");
    tracing::debug!(new_html_len = new_html.len(), "New HTML length");

    match try_preserve_comments(old_html, new_html) {
        Ok(result) => {
            tracing::info!("Comment preservation completed");
            result
        }
        Err(e) => {
            tracing::error!(error = %e, "Comment preservation failed");
            tracing::warn!("Falling back to new HTML without comment preservation");
            PreserveResult {
                html: new_html.to_owned(),
                unmatched_comments: vec![],
                warnings: vec![format!("comment preservation skipped: {e}")],
            }
        }
    }
}

fn try_preserve_comments(
    old_html: &str,
    new_html: &str,
) -> Result<PreserveResult, CommentPreservationError> {
    let parser = ConfluenceXmlParser::new();
    let serializer = ConfluenceXmlSerializer::new();

    // Parse both HTMLs
    tracing::debug!("Parsing old HTML...");
    let old_tree = parser.parse(old_html)?;
    tracing::debug!("Parsing new HTML...");
    let mut new_tree = parser.parse(new_html)?;

    // Match nodes
    tracing::debug!("Matching nodes...");
    let matcher = TreeMatcher::new(&old_tree, &new_tree);
    let matches = matcher.find_matches();
    tracing::info!(count = matches.len(), "Found matching nodes");

    // Transfer markers
    tracing::debug!("Transferring markers...");
    let mut transfer = CommentMarkerTransfer::new();
    transfer.transfer(&matches, &mut new_tree, &old_tree);

    // Serialize back
    tracing::debug!("Serializing result...");
    let html = serializer.serialize(&new_tree);
    tracing::debug!(result_html_len = html.len(), "Result HTML length");

    Ok(PreserveResult {
        html,
        unmatched_comments: transfer.into_unmatched_comments(),
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

        assert!(result.unmatched_comments.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("ac:inline-comment-marker"));
        assert!(result.html.contains(r#"ac:ref="abc""#));
    }

    #[test]
    fn test_preserve_comments_marker_in_tail() {
        let old_html = r#"<li><code>x</code> <ac:inline-comment-marker ac:ref="id">marked</ac:inline-comment-marker>, rest</li>"#;
        let new_html = "<li><code>x</code> marked, rest</li>";

        let result = preserve_comments(old_html, new_html);

        assert!(result.unmatched_comments.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("ac:inline-comment-marker"));
    }

    #[test]
    fn test_preserve_comments_cyrillic_text() {
        let old_html = r#"<li><code>gateway</code> <ac:inline-comment-marker ac:ref="xyz">проверяет тип</ac:inline-comment-marker>, активность</li>"#;
        let new_html = "<li><code>gateway</code> проверяет тип, активность</li>";

        let result = preserve_comments(old_html, new_html);

        assert!(result.unmatched_comments.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("проверяет тип"));
        assert!(result.html.contains("ac:inline-comment-marker"));
    }

    #[test]
    fn test_preserve_comments_multiple_markers() {
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="a">first paragraph text</ac:inline-comment-marker></p><p><ac:inline-comment-marker ac:ref="b">second paragraph text</ac:inline-comment-marker></p>"#;
        let new_html = "<p>first paragraph text</p><p>second paragraph text</p>";

        let result = preserve_comments(old_html, new_html);

        assert!(result.unmatched_comments.is_empty());
        assert!(result.warnings.is_empty());
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
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_preserve_comments_unmatched_when_parent_not_matched() {
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="xyz">Original sentence here</ac:inline-comment-marker></p>"#;
        let new_html = "<p>Completely different content</p>";

        let result = preserve_comments(old_html, new_html);

        assert_eq!(result.unmatched_comments.len(), 1);
        assert_eq!(result.unmatched_comments[0].ref_id, "xyz");
        assert_eq!(result.unmatched_comments[0].text, "Original sentence here");
        assert!(result.warnings.is_empty());
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

        assert!(result.unmatched_comments.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("inline-comment-marker"));
        assert!(result.html.contains(r#"ac:ref="marker-id""#));
        assert!(result.html.contains("keep-this"));
    }

    #[test]
    fn test_preserve_comments_parse_failure_surfaces_warning() {
        // Malformed XML that quick-xml cannot parse.
        let old_html = "<p>unclosed paragraph";
        let new_html = "<p>new content</p>";

        let result = preserve_comments(old_html, new_html);

        // Falls back to new HTML unchanged.
        assert_eq!(result.html, new_html);
        assert!(result.unmatched_comments.is_empty());

        // The parse failure produces exactly one warning string.
        assert_eq!(result.warnings.len(), 1, "got: {:?}", result.warnings);
        assert!(
            result.warnings[0].starts_with("comment preservation skipped:"),
            "warning was: {}",
            result.warnings[0]
        );
    }
}
