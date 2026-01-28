//! Confluence XHTML parser with namespace support.

#![allow(clippy::unused_self)] // Unit struct methods have &self for API consistency

use std::collections::HashMap;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use super::entities::convert_html_entities;
use super::tree::TreeNode;
use crate::error::CommentPreservationError;

/// Confluence XML namespaces.
const NAMESPACES: &[(&str, &str)] = &[
    ("ac", "http://www.atlassian.com/schema/confluence/4/ac/"),
    ("ri", "http://www.atlassian.com/schema/confluence/4/ri/"),
];

/// Parse Confluence XHTML with namespace support.
pub struct ConfluenceXmlParser;

impl ConfluenceXmlParser {
    /// Create a new parser.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Parse HTML string to `TreeNode` structure.
    ///
    /// Adds namespace declarations for `ac:` and `ri:` prefixes, then parses
    /// the XML into a tree structure.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTML cannot be parsed as valid XML.
    pub fn parse(&self, html: &str) -> Result<TreeNode, CommentPreservationError> {
        // Convert HTML entities to Unicode
        let html = convert_html_entities(html);

        // Add namespace declarations to root
        let namespace_decls = NAMESPACES
            .iter()
            .map(|(prefix, uri)| format!(r#"xmlns:{prefix}="{uri}""#))
            .collect::<Vec<_>>()
            .join(" ");
        let wrapped = format!("<root {namespace_decls}>{html}</root>");

        let mut reader = Reader::from_str(&wrapped);
        reader.config_mut().trim_text(false);

        self.parse_element(&mut reader)
    }

    fn parse_element<R: BufRead>(
        &self,
        reader: &mut Reader<R>,
    ) -> Result<TreeNode, CommentPreservationError> {
        let mut buf = Vec::new();
        let mut node = TreeNode::default();
        let mut first_element = true;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    if first_element {
                        // This is our root element
                        node.tag = self.decode_tag(reader, &e);
                        node.attrs = self.decode_attrs(reader, &e);
                        first_element = false;
                    } else {
                        // Child element - parse recursively
                        let child_tag = self.decode_tag(reader, &e);
                        let child_attrs = self.decode_attrs(reader, &e);
                        let mut child = self.parse_children(reader, &child_tag)?;
                        child.tag = child_tag;
                        child.attrs = child_attrs;
                        node.children.push(child);
                    }
                }
                Event::Empty(e) => {
                    if first_element {
                        node.tag = self.decode_tag(reader, &e);
                        node.attrs = self.decode_attrs(reader, &e);
                        return Ok(node);
                    }
                    // Self-closing child element
                    let child = TreeNode {
                        tag: self.decode_tag(reader, &e),
                        attrs: self.decode_attrs(reader, &e),
                        ..Default::default()
                    };
                    node.children.push(child);
                }
                Event::Text(e) => {
                    if first_element {
                        continue;
                    }
                    let text = reader.decoder().decode(&e)?.into_owned();
                    append_text(&mut node, &text);
                }
                Event::GeneralRef(e) => {
                    if first_element {
                        continue;
                    }
                    // Handle entity references (e.g., &lt; &gt; &amp;)
                    let entity = reader.decoder().decode(&e)?.into_owned();
                    let text = decode_entity(&entity);
                    append_text(&mut node, &text);
                }
                Event::CData(e) => {
                    if first_element {
                        continue;
                    }
                    let text = String::from_utf8_lossy(&e).into_owned();
                    append_text(&mut node, &text);
                }
                Event::End(_) | Event::Eof => {
                    // End of current element or document
                    return Ok(node);
                }
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {
                    // Ignore these
                }
            }
            buf.clear();
        }
    }

    fn parse_children<R: BufRead>(
        &self,
        reader: &mut Reader<R>,
        parent_tag: &str,
    ) -> Result<TreeNode, CommentPreservationError> {
        let mut buf = Vec::new();
        let mut node = TreeNode::default();

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    // Child element
                    let child_tag = self.decode_tag(reader, &e);
                    let child_attrs = self.decode_attrs(reader, &e);
                    let mut child = self.parse_children(reader, &child_tag)?;
                    child.tag = child_tag;
                    child.attrs = child_attrs;
                    node.children.push(child);
                }
                Event::Empty(e) => {
                    // Self-closing child element
                    let child = TreeNode {
                        tag: self.decode_tag(reader, &e),
                        attrs: self.decode_attrs(reader, &e),
                        ..Default::default()
                    };
                    node.children.push(child);
                }
                Event::Text(e) => {
                    let text = reader.decoder().decode(&e)?.into_owned();
                    append_text(&mut node, &text);
                }
                Event::GeneralRef(e) => {
                    // Handle entity references (e.g., &lt; &gt; &amp;)
                    let entity = reader.decoder().decode(&e)?.into_owned();
                    let text = decode_entity(&entity);
                    append_text(&mut node, &text);
                }
                Event::CData(e) => {
                    let text = String::from_utf8_lossy(&e).into_owned();
                    append_text(&mut node, &text);
                }
                Event::End(e) => {
                    let end_tag = self.decode_tag_from_bytes(reader, e.name().as_ref());
                    if end_tag == parent_tag {
                        return Ok(node);
                    }
                    // Mismatched end tag - continue
                }
                Event::Eof => {
                    return Ok(node);
                }
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
            }
            buf.clear();
        }
    }

    fn decode_tag<R: BufRead>(&self, reader: &Reader<R>, e: &BytesStart) -> String {
        self.decode_tag_from_bytes(reader, e.name().as_ref())
    }

    fn decode_tag_from_bytes<R: BufRead>(&self, reader: &Reader<R>, name: &[u8]) -> String {
        reader.decoder().decode(name).map_or_else(
            |_| String::from_utf8_lossy(name).into_owned(),
            std::borrow::Cow::into_owned,
        )
    }

    fn decode_attrs<R: BufRead>(
        &self,
        reader: &Reader<R>,
        e: &BytesStart,
    ) -> HashMap<String, String> {
        let mut attrs = HashMap::new();
        for attr in e.attributes().flatten() {
            let key = reader.decoder().decode(attr.key.as_ref()).map_or_else(
                |_| String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                std::borrow::Cow::into_owned,
            );

            // Skip namespace declarations
            if key.starts_with("xmlns") {
                continue;
            }

            let value = attr.unescape_value().map_or_else(
                |_| String::from_utf8_lossy(&attr.value).into_owned(),
                std::borrow::Cow::into_owned,
            );

            attrs.insert(key, value);
        }
        attrs
    }
}

impl Default for ConfluenceXmlParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Append text to node's text or last child's tail.
fn append_text(node: &mut TreeNode, text: &str) {
    if let Some(last_child) = node.children.last_mut() {
        last_child.tail.push_str(text);
    } else {
        node.text.push_str(text);
    }
}

/// Decode XML entity references to their character values.
fn decode_entity(entity: &str) -> String {
    match entity {
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "amp" => "&".to_string(),
        "apos" => "'".to_string(),
        "quot" => "\"".to_string(),
        // Numeric character references
        s if s.starts_with('#') => {
            let code = if s.starts_with("#x") || s.starts_with("#X") {
                u32::from_str_radix(&s[2..], 16).ok()
            } else {
                s[1..].parse::<u32>().ok()
            };
            code.and_then(char::from_u32)
                .map_or_else(|| format!("&{entity};"), |c| c.to_string())
        }
        // Unknown entity - preserve as-is
        _ => format!("&{entity};"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_element() {
        let parser = ConfluenceXmlParser::new();
        let tree = parser.parse("<p>Hello</p>").unwrap();

        assert_eq!(tree.children.len(), 1);
        let p_node = &tree.children[0];
        assert_eq!(p_node.tag, "p");
        assert_eq!(p_node.text, "Hello");
    }

    #[test]
    fn test_parse_nested_elements() {
        let parser = ConfluenceXmlParser::new();
        let tree = parser.parse("<p><strong>Bold</strong> text</p>").unwrap();

        let p_node = &tree.children[0];
        assert_eq!(p_node.tag, "p");
        assert!(p_node.text.is_empty());
        assert_eq!(p_node.children.len(), 1);

        let strong_node = &p_node.children[0];
        assert_eq!(strong_node.tag, "strong");
        assert_eq!(strong_node.text, "Bold");
        assert_eq!(strong_node.tail, " text");
    }

    #[test]
    fn test_parse_comment_marker() {
        let parser = ConfluenceXmlParser::new();
        let html = r#"<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>"#;
        let tree = parser.parse(html).unwrap();

        let p_node = &tree.children[0];
        let marker = &p_node.children[0];
        assert!(marker.is_comment_marker());
        assert_eq!(marker.text, "marked");
        assert_eq!(marker.tail, " text");
    }

    #[test]
    fn test_parse_html_entities() {
        let parser = ConfluenceXmlParser::new();
        let tree = parser.parse("<p>Hello&nbsp;World&mdash;Test</p>").unwrap();

        let p_node = &tree.children[0];
        assert!(p_node.text.contains('\u{00a0}')); // nbsp
        assert!(p_node.text.contains('\u{2014}')); // mdash
    }

    #[test]
    fn test_parse_self_closing_elements() {
        let parser = ConfluenceXmlParser::new();
        let tree = parser.parse("<p>Before<br />After</p>").unwrap();

        let p_node = &tree.children[0];
        assert_eq!(p_node.text, "Before");
        assert_eq!(p_node.children.len(), 1);
        assert_eq!(p_node.children[0].tag, "br");
        assert_eq!(p_node.children[0].tail, "After");
    }
}
