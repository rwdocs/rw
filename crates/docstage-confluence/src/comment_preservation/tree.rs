//! Tree node representation for Confluence HTML.

use std::collections::HashMap;

/// Confluence namespace URI.
pub const AC_NAMESPACE: &str = "http://www.atlassian.com/schema/confluence/4/ac/";

/// Node in parsed HTML tree.
#[derive(Debug, Clone, Default)]
pub struct TreeNode {
    /// Element tag name (may include namespace prefix or URI).
    pub tag: String,
    /// Direct text content.
    pub text: String,
    /// Text after element (XML tail).
    pub tail: String,
    /// Element attributes.
    pub attrs: HashMap<String, String>,
    /// Child nodes.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Create a new tree node with the given tag.
    #[must_use]
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            ..Default::default()
        }
    }

    /// Set text content.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Set tail content.
    #[must_use]
    pub fn with_tail(mut self, tail: impl Into<String>) -> Self {
        self.tail = tail.into();
        self
    }

    /// Set attributes.
    #[must_use]
    pub fn with_attrs(mut self, attrs: HashMap<String, String>) -> Self {
        self.attrs = attrs;
        self
    }

    /// Set children.
    #[must_use]
    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }

    /// Normalized text content from this node and all descendants for matching.
    ///
    /// Concatenates direct text, children's signatures, and tail text with spaces.
    #[must_use]
    pub fn text_signature(&self) -> String {
        let mut parts = Vec::new();

        let text_trimmed = self.text.trim();
        if !text_trimmed.is_empty() {
            parts.push(text_trimmed.to_string());
        }

        for child in &self.children {
            let sig = child.text_signature();
            if !sig.is_empty() {
                parts.push(sig);
            }
        }

        let tail_trimmed = self.tail.trim();
        if !tail_trimmed.is_empty() {
            parts.push(tail_trimmed.to_string());
        }

        parts.join(" ")
    }

    /// Check if this node is an inline comment marker.
    #[must_use]
    pub fn is_comment_marker(&self) -> bool {
        // Handle multiple formats:
        // - Full namespace URI: {http://...}inline-comment-marker
        // - Prefixed: ac:inline-comment-marker
        // - Plain: inline-comment-marker
        self.tag == format!("{{{AC_NAMESPACE}}}inline-comment-marker")
            || self.tag == "ac:inline-comment-marker"
            || self.tag.contains("inline-comment-marker")
    }

    /// Get the `ac:ref` attribute value from a comment marker.
    #[must_use]
    pub fn marker_ref(&self) -> Option<&str> {
        // Try namespaced version first
        self.attrs
            .get(&format!("{{{AC_NAMESPACE}}}ref"))
            .or_else(|| self.attrs.get("ac:ref"))
            .map(String::as_str)
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
    use super::*;

    #[test]
    fn test_text_signature_direct_text() {
        let node = TreeNode::new("p").with_text("Hello World");
        assert_eq!(node.text_signature(), "Hello World");
    }

    #[test]
    fn test_text_signature_with_children() {
        let child = TreeNode::new("strong").with_text("Bold").with_tail(" text");
        let node = TreeNode::new("p").with_children(vec![child]);
        assert_eq!(node.text_signature(), "Bold text");
    }

    #[test]
    fn test_text_signature_with_tail() {
        let node = TreeNode::new("span").with_text("Hello").with_tail(" World");
        assert_eq!(node.text_signature(), "Hello World");
    }

    #[test]
    fn test_is_comment_marker_namespaced() {
        let node = TreeNode::new(
            "{http://www.atlassian.com/schema/confluence/4/ac/}inline-comment-marker",
        );
        assert!(node.is_comment_marker());
    }

    #[test]
    fn test_is_comment_marker_prefixed() {
        let node = TreeNode::new("ac:inline-comment-marker");
        assert!(node.is_comment_marker());
    }

    #[test]
    fn test_is_comment_marker_false() {
        let node = TreeNode::new("p");
        assert!(!node.is_comment_marker());
    }

    #[test]
    fn test_marker_ref_prefixed() {
        let mut attrs = HashMap::new();
        attrs.insert("ac:ref".to_string(), "abc123".to_string());
        let node = TreeNode::new("ac:inline-comment-marker").with_attrs(attrs);
        assert_eq!(node.marker_ref(), Some("abc123"));
    }

    #[test]
    fn test_marker_ref_namespaced() {
        let mut attrs = HashMap::new();
        attrs.insert(
            "{http://www.atlassian.com/schema/confluence/4/ac/}ref".to_string(),
            "xyz789".to_string(),
        );
        let node = TreeNode::new("ac:inline-comment-marker").with_attrs(attrs);
        assert_eq!(node.marker_ref(), Some("xyz789"));
    }

    #[test]
    fn test_comment_markers() {
        let marker = TreeNode::new("ac:inline-comment-marker").with_text("marked");
        let span = TreeNode::new("span").with_text("normal");
        let node = TreeNode::new("p").with_children(vec![marker, span]);

        let markers = node.comment_markers();
        assert_eq!(markers.len(), 1);
        assert!(markers[0].is_comment_marker());
    }
}
