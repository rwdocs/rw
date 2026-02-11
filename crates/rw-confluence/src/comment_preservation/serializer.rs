//! Confluence XHTML serializer with CDATA support.

#![allow(clippy::unused_self)] // Unit struct methods have &self for API consistency

use std::fmt::Write;
use std::sync::LazyLock;

use regex::Regex;

use super::tree::TreeNode;

/// Pattern for matching plain-text-body elements.
static PLAIN_TEXT_BODY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(<(?:ac:|ns\d+:)?plain-text-body[^>]*>)(.*?)(</(?:ac:|ns\d+:)?plain-text-body>)")
        .expect("invalid plain-text-body regex")
});

/// Serialize `TreeNode` back to Confluence storage format.
pub struct ConfluenceXmlSerializer;

impl ConfluenceXmlSerializer {
    /// Create a new serializer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Serialize tree to HTML string.
    ///
    /// The root wrapper element is removed, and CDATA sections are restored
    /// for `ac:plain-text-body` elements.
    pub fn serialize(&self, tree: &TreeNode) -> String {
        let mut out = String::with_capacity(4096);

        // Serialize children of root (skip the wrapper)
        for child in &tree.children {
            serialize_node(child, &mut out);
        }

        // Restore CDATA sections
        restore_cdata_sections(&out)
    }
}

impl Default for ConfluenceXmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Serialize a single node recursively.
fn serialize_node(node: &TreeNode, out: &mut String) {
    // Opening tag
    out.push('<');
    out.push_str(&node.tag);

    // Attributes
    for (key, value) in &node.attrs {
        write!(out, r#" {}="{}""#, key, escape_attr(value)).unwrap();
    }

    if node.children.is_empty() && node.text.is_empty() {
        // Self-closing tag
        out.push_str(" />");
    } else {
        out.push('>');

        // Text content
        if !node.text.is_empty() {
            out.push_str(&escape_text(&node.text));
        }

        // Children
        for child in &node.children {
            serialize_node(child, out);
        }

        // Closing tag
        write!(out, "</{}>", node.tag).unwrap();
    }

    // Tail text
    if !node.tail.is_empty() {
        out.push_str(&escape_text(&node.tail));
    }
}

/// Escape text for XML content.
fn escape_text(text: &str) -> String {
    escape_xml(text, false)
}

/// Escape text for XML attribute values.
fn escape_attr(text: &str) -> String {
    escape_xml(text, true)
}

/// Escape XML special characters.
fn escape_xml(text: &str, escape_quotes: bool) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' if escape_quotes => result.push_str("&quot;"),
            '\'' if escape_quotes => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Restore CDATA sections for plain-text-body elements.
fn restore_cdata_sections(html: &str) -> String {
    PLAIN_TEXT_BODY_PATTERN
        .replace_all(html, |caps: &regex::Captures| {
            let tag_start = &caps[1];
            let content = &caps[2];
            let tag_end = &caps[3];

            // Unescape XML entities that were escaped during serialization
            let content = content
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
                .replace("&quot;", "\"")
                .replace("&apos;", "'");

            format!("{tag_start}<![CDATA[{content}]]>{tag_end}")
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_simple_element() {
        let node = TreeNode::new("root").with_children(vec![TreeNode::new("p").with_text("Hello")]);
        let serializer = ConfluenceXmlSerializer::new();

        let html = serializer.serialize(&node);
        assert_eq!(html, "<p>Hello</p>");
    }

    #[test]
    fn test_serialize_with_children() {
        let strong = TreeNode::new("strong").with_text("Bold").with_tail(" text");
        let p = TreeNode::new("p").with_children(vec![strong]);
        let root = TreeNode::new("root").with_children(vec![p]);

        let serializer = ConfluenceXmlSerializer::new();
        let html = serializer.serialize(&root);

        assert_eq!(html, "<p><strong>Bold</strong> text</p>");
    }

    #[test]
    fn test_serialize_self_closing() {
        let br = TreeNode::new("br").with_tail("After");
        let p = TreeNode::new("p")
            .with_text("Before")
            .with_children(vec![br]);
        let root = TreeNode::new("root").with_children(vec![p]);

        let serializer = ConfluenceXmlSerializer::new();
        let html = serializer.serialize(&root);

        assert_eq!(html, "<p>Before<br />After</p>");
    }

    #[test]
    fn test_serialize_with_attributes() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("ac:ref".to_owned(), "abc".to_owned());
        let marker = TreeNode::new("ac:inline-comment-marker")
            .with_attrs(attrs)
            .with_text("marked");
        let p = TreeNode::new("p").with_children(vec![marker]);
        let root = TreeNode::new("root").with_children(vec![p]);

        let serializer = ConfluenceXmlSerializer::new();
        let html = serializer.serialize(&root);

        assert!(html.contains(r#"ac:ref="abc""#));
        assert!(html.contains("marked"));
    }

    #[test]
    fn test_escape_special_chars() {
        let p = TreeNode::new("p").with_text("a < b & c > d");
        let root = TreeNode::new("root").with_children(vec![p]);

        let serializer = ConfluenceXmlSerializer::new();
        let html = serializer.serialize(&root);

        assert_eq!(html, "<p>a &lt; b &amp; c &gt; d</p>");
    }

    #[test]
    fn test_restore_cdata_sections() {
        let html = "<ac:plain-text-body>&lt;code&gt;</ac:plain-text-body>";
        let result = restore_cdata_sections(html);
        assert_eq!(
            result,
            "<ac:plain-text-body><![CDATA[<code>]]></ac:plain-text-body>"
        );
    }
}
