//! Confluence XHTML serializer.

use std::fmt::Write;

use rw_renderer::escape_into;

use super::tree::TreeNode;
use crate::backend::write_cdata;

/// Serialize the children of `tree`, dropping the wrapper root that
/// [`parse`](super::parser::parse) added.
pub fn serialize(tree: &TreeNode) -> String {
    let mut out = String::with_capacity(4096);
    for child in &tree.children {
        serialize_node(child, &mut out);
    }
    out
}

/// Serialize a single node recursively.
///
/// Text and attributes are escaped as the fresh render escapes them, so what
/// the renderer wrote serializes back to the same bytes.
fn serialize_node(node: &TreeNode, out: &mut String) {
    if node.is_entity_ref() {
        out.push_str(&node.tag);
    } else {
        serialize_element(node, out);
    }
    escape_into(&node.tail, out);
}

fn serialize_element(node: &TreeNode, out: &mut String) {
    // Opening tag
    out.push('<');
    out.push_str(&node.tag);

    // Attributes
    for (key, value) in &node.attrs {
        write!(out, r#" {key}=""#).unwrap();
        escape_into(value, out);
        out.push('"');
    }

    // Even empty, the code macro body is CDATA in a fresh render.
    let cdata = node.tag == "ac:plain-text-body";
    if node.children.is_empty() && node.text.is_empty() && !cdata {
        // Self-closing tag
        out.push_str(" />");
    } else {
        out.push('>');

        // Text content
        if cdata {
            write_cdata(&node.text, out);
        } else {
            escape_into(&node.text, out);
        }

        // Children
        for child in &node.children {
            serialize_node(child, out);
        }

        // Closing tag
        write!(out, "</{}>", node.tag).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_preservation::parser::parse;

    fn round_trip(html: &str) -> String {
        serialize(&parse(html).unwrap())
    }

    #[test]
    fn test_serialize_simple_element() {
        assert_eq!(round_trip("<p>Hello</p>"), "<p>Hello</p>");
    }

    #[test]
    fn test_serialize_with_children() {
        assert_eq!(
            round_trip("<p><strong>Bold</strong> text</p>"),
            "<p><strong>Bold</strong> text</p>"
        );
    }

    #[test]
    fn test_serialize_self_closing() {
        assert_eq!(
            round_trip("<p>Before<br/>After</p>"),
            "<p>Before<br />After</p>"
        );
    }

    #[test]
    fn test_serialize_with_attributes() {
        let html = r#"<p><ac:inline-comment-marker ac:ref="a&quot;b">marked</ac:inline-comment-marker></p>"#;
        assert_eq!(round_trip(html), html);
    }

    #[test]
    fn test_escape_special_chars() {
        assert_eq!(
            round_trip("<p>a &lt; b &amp; c &gt; d</p>"),
            "<p>a &lt; b &amp; c &gt; d</p>"
        );
    }

    /// The code macro is rendered with a CDATA body; writing it back as
    /// escaped text would make every preserved page differ from a fresh one.
    #[test]
    fn test_plain_text_body_stays_cdata() {
        let html = "<ac:plain-text-body><![CDATA[if a < b && c\n]]></ac:plain-text-body>";
        assert_eq!(round_trip(html), html);
    }

    #[test]
    fn test_empty_plain_text_body_stays_cdata() {
        assert_eq!(
            round_trip("<ac:plain-text-body><![CDATA[]]></ac:plain-text-body>"),
            "<ac:plain-text-body><![CDATA[]]></ac:plain-text-body>"
        );
    }

    #[test]
    fn test_plain_text_body_splits_cdata_terminator() {
        let html = "<ac:plain-text-body>a]]&gt;b</ac:plain-text-body>";
        assert_eq!(
            round_trip(html),
            "<ac:plain-text-body><![CDATA[a]]]]><![CDATA[>b]]></ac:plain-text-body>"
        );
    }
}
