//! Tree node representation for Confluence HTML.

/// Node in parsed HTML tree.
#[derive(Debug, Default)]
pub struct TreeNode {
    /// Element tag name, prefix included (`ac:inline-comment-marker`), or for
    /// an [entity reference](Self::entity_ref) the reference itself.
    pub tag: String,
    /// Direct text content.
    pub text: String,
    /// Text after element (XML tail).
    pub tail: String,
    /// Element attributes, in source order.
    pub attrs: Vec<(String, String)>,
    /// Child nodes.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// A reference to the entity `name`, which the parser could not resolve.
    ///
    /// Kept as a node: text is escaped on output, which would turn the
    /// reference into the literal `&name;`.
    #[must_use]
    pub fn entity_ref(name: &str) -> Self {
        Self {
            tag: format!("&{name};"),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn is_entity_ref(&self) -> bool {
        self.tag.starts_with('&')
    }

    /// Normalized text content from this node and all descendants for matching.
    ///
    /// Concatenates direct text, children's signatures, and tail text with spaces.
    #[must_use]
    pub fn text_signature(&self) -> String {
        let mut parts = Vec::new();

        let text_trimmed = self.text.trim();
        if !text_trimmed.is_empty() {
            parts.push(text_trimmed.to_owned());
        }

        for child in &self.children {
            let sig = child.text_signature();
            if !sig.is_empty() {
                parts.push(sig);
            }
        }

        let tail_trimmed = self.tail.trim();
        if !tail_trimmed.is_empty() {
            parts.push(tail_trimmed.to_owned());
        }

        parts.join(" ")
    }

    /// Check if this node is an inline comment marker.
    #[must_use]
    pub fn is_comment_marker(&self) -> bool {
        self.tag.contains("inline-comment-marker")
    }

    /// Get the `ac:ref` attribute value from a comment marker.
    #[must_use]
    pub fn marker_ref(&self) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == "ac:ref")
            .map(|(_, value)| value.as_str())
    }

    /// Get all comment marker children of this node.
    #[must_use]
    pub fn comment_markers(&self) -> Vec<&TreeNode> {
        self.children
            .iter()
            .filter(|child| child.is_comment_marker())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::comment_preservation::parser::parse;

    #[test]
    fn test_text_signature_direct_text() {
        let tree = parse("<p>Hello World</p>").unwrap();
        assert_eq!(tree.children[0].text_signature(), "Hello World");
    }

    #[test]
    fn test_text_signature_with_children() {
        let tree = parse("<p><strong>Bold</strong> text</p>").unwrap();
        assert_eq!(tree.children[0].text_signature(), "Bold text");
    }

    #[test]
    fn test_text_signature_with_tail() {
        let tree = parse("<p><span>Hello</span> World</p>").unwrap();
        assert_eq!(tree.children[0].children[0].text_signature(), "Hello World");
    }

    #[test]
    fn test_is_comment_marker_false() {
        let tree = parse("<p>text</p>").unwrap();
        assert!(!tree.children[0].is_comment_marker());
    }

    #[test]
    fn test_marker_ref() {
        let tree =
            parse(r#"<ac:inline-comment-marker ac:ref="abc123">x</ac:inline-comment-marker>"#)
                .unwrap();
        assert_eq!(tree.children[0].marker_ref(), Some("abc123"));
    }

    #[test]
    fn test_comment_markers() {
        let tree = parse(
            r#"<p><ac:inline-comment-marker ac:ref="a">marked</ac:inline-comment-marker><span>normal</span></p>"#,
        )
        .unwrap();

        let markers = tree.children[0].comment_markers();
        assert_eq!(markers.len(), 1);
        assert!(markers[0].is_comment_marker());
    }
}
