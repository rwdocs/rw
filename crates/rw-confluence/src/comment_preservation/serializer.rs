//! Confluence XHTML serializer.

use std::fmt::Write;

use super::tree::TreeNode;

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
            if node.tag == "ac:plain-text-body" {
                write_cdata(&node.text, out);
            } else {
                out.push_str(&escape_text(&node.text));
            }
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

/// Write `text` as CDATA, the form the code macro body is rendered in.
fn write_cdata(text: &str, out: &mut String) {
    out.push_str("<![CDATA[");
    // A literal `]]>` would end the section early.
    out.push_str(&text.replace("]]>", "]]]]><![CDATA[>"));
    out.push_str("]]>");
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
    fn test_plain_text_body_splits_cdata_terminator() {
        let html = "<ac:plain-text-body>a]]&gt;b</ac:plain-text-body>";
        assert_eq!(
            round_trip(html),
            "<ac:plain-text-body><![CDATA[a]]]]><![CDATA[>b]]></ac:plain-text-body>"
        );
    }
}
