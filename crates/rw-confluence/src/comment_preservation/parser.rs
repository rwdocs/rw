//! Confluence XHTML parser with namespace support.

use quick_xml::XmlVersion;
use quick_xml::errors::{Error, IllFormedError};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use super::entities::resolve_entity;
use super::tree::TreeNode;

/// Storage format is a fragment using the `ac:` and `ri:` prefixes without
/// declaring them, so it is parsed inside a wrapper that does.
const ROOT_OPEN: &str = concat!(
    r#"<root xmlns:ac="http://www.atlassian.com/schema/confluence/4/ac/""#,
    r#" xmlns:ri="http://www.atlassian.com/schema/confluence/4/ri/">"#,
);

/// Parse a storage-format fragment into a tree whose root is the wrapper
/// element and whose children are the fragment's top-level elements.
///
/// # Errors
///
/// Returns an error if the fragment, attributes included, is not well-formed
/// XML, or if an attribute value references an entity [`resolve_entity`] does
/// not know.
pub fn parse(html: &str) -> Result<TreeNode, Error> {
    let wrapped = format!("{ROOT_OPEN}{html}</root>");

    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);

    // The wrapper's own start tag.
    reader.read_event()?;
    Ok(TreeNode {
        tag: "root".to_owned(),
        ..parse_content(&mut reader).map_err(hide_wrapper)?
    })
}

/// Restate an end-tag mismatch against the wrapper as the fragment's own
/// mistake; the caller never wrote `<root>`.
fn hide_wrapper(error: Error) -> Error {
    match error {
        Error::IllFormed(IllFormedError::MismatchedEndTag { expected, found })
            if found == "root" =>
        {
            Error::IllFormed(IllFormedError::MissingEndTag(expected))
        }
        Error::IllFormed(IllFormedError::MismatchedEndTag { expected, found })
            if expected == "root" =>
        {
            Error::IllFormed(IllFormedError::UnmatchedEndTag(found))
        }
        error => error,
    }
}

/// Read up to the end tag of the element just opened, returning its text and
/// children; the caller fills in the tag and attributes.
fn parse_content(reader: &mut Reader<&[u8]>) -> Result<TreeNode, Error> {
    let mut node = TreeNode::default();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let tag = decode_tag(&e);
                let attrs = decode_attrs(&e)?;
                let mut child = parse_content(reader)?;
                child.tag = tag;
                child.attrs = attrs;
                node.children.push(child);
            }
            Event::Empty(e) => {
                node.children.push(TreeNode {
                    tag: decode_tag(&e),
                    attrs: decode_attrs(&e)?,
                    ..Default::default()
                });
            }
            Event::Text(e) => append_text(&mut node, e.as_ref()),
            Event::GeneralRef(e) => {
                if let Some(ch) = e.resolve_char_ref()? {
                    append_text(&mut node, ch.encode_utf8(&mut [0; 4]));
                } else if let Some(text) = resolve_entity(e.as_ref()) {
                    append_text(&mut node, text);
                } else {
                    node.children.push(TreeNode::entity_ref(e.as_ref()));
                }
            }
            Event::CData(e) => append_text(&mut node, e.as_ref()),
            // quick-xml rejects a mismatched end tag, so this one closes `node`.
            Event::End(_) | Event::Eof => return Ok(node),
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
}

fn decode_tag(e: &BytesStart) -> String {
    e.name().as_ref().to_owned()
}

fn decode_attrs(e: &BytesStart) -> Result<Vec<(String, String)>, Error> {
    let mut attrs = Vec::new();
    for attr in e.attributes() {
        let attr = attr?;
        let key = attr.key.as_ref().to_owned();

        // Skip namespace declarations
        if key.starts_with("xmlns") {
            continue;
        }

        // Depth 1: no replacement text contains a reference.
        let value = attr.normalized_value_with(XmlVersion::Implicit1_0, 1, resolve_entity)?;

        attrs.push((key, value.into_owned()));
    }
    Ok(attrs)
}

/// Append text to node's text or last child's tail.
fn append_text(node: &mut TreeNode, text: &str) {
    if let Some(last_child) = node.children.last_mut() {
        last_child.tail.push_str(text);
    } else {
        node.text.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_element() {
        let tree = parse("<p>Hello</p>").unwrap();

        assert_eq!(tree.children.len(), 1);
        let p_node = &tree.children[0];
        assert_eq!(p_node.tag, "p");
        assert_eq!(p_node.text, "Hello");
    }

    #[test]
    fn test_parse_nested_elements() {
        let tree = parse("<p><strong>Bold</strong> text</p>").unwrap();

        let p_node = &tree.children[0];
        assert_eq!(p_node.tag, "p");
        assert_eq!(p_node.text, "");
        assert_eq!(p_node.children.len(), 1);

        let strong_node = &p_node.children[0];
        assert_eq!(strong_node.tag, "strong");
        assert_eq!(strong_node.text, "Bold");
        assert_eq!(strong_node.tail, " text");
    }

    #[test]
    fn test_parse_comment_marker() {
        let html = r#"<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>"#;
        let tree = parse(html).unwrap();

        let p_node = &tree.children[0];
        let marker = &p_node.children[0];
        assert!(marker.is_comment_marker());
        assert_eq!(marker.text, "marked");
        assert_eq!(marker.tail, " text");
    }

    #[test]
    fn test_parse_html_entities() {
        let tree = parse("<p>Hello&nbsp;World&mdash;Test</p>").unwrap();

        let p_node = &tree.children[0];
        assert!(p_node.text.contains('\u{00a0}')); // nbsp
        assert!(p_node.text.contains('\u{2014}')); // mdash
    }

    #[test]
    fn test_parse_html_entities_in_attributes() {
        let tree = parse(r#"<a title="&copy;&amp;&#x27;">x</a>"#).unwrap();

        assert_eq!(
            tree.children[0].attrs,
            [("title".to_owned(), "©&'".to_owned())]
        );
    }

    /// A value is stored decoded, so an entity with no known replacement has
    /// nothing it could be stored as.
    #[test]
    fn test_parse_error_for_unknown_entity_in_attribute() {
        assert!(parse(r#"<a title="&hearts;">x</a>"#).is_err());
    }

    /// quick-xml checks attributes only when they are read, and a value cannot
    /// be stored for one it rejects.
    #[test]
    fn test_parse_error_for_attribute_without_value() {
        assert_eq!(
            parse("<p><input disabled /></p>").unwrap_err().to_string(),
            "error while parsing attribute: position 15: \
             attribute key must be directly followed by `=` or space"
        );
    }

    #[test]
    fn test_parse_error_for_unclosed_tag_does_not_name_the_wrapper() {
        assert_eq!(
            parse("<p>unclosed").unwrap_err().to_string(),
            "ill-formed document: start tag not closed: `</p>` not found before end of input"
        );
    }

    #[test]
    fn test_parse_error_for_stray_end_tag_does_not_name_the_wrapper() {
        assert_eq!(
            parse("text</p>").unwrap_err().to_string(),
            "ill-formed document: close tag `</p>` does not match any open tag"
        );
    }

    #[test]
    fn test_parse_self_closing_elements() {
        let tree = parse("<p>Before<br />After</p>").unwrap();

        let p_node = &tree.children[0];
        assert_eq!(p_node.text, "Before");
        assert_eq!(p_node.children.len(), 1);
        assert_eq!(p_node.children[0].tag, "br");
        assert_eq!(p_node.children[0].tail, "After");
    }
}
