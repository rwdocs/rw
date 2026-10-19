//! Confluence rejects a storage body that is not XML only at upload, far from
//! the Markdown at fault, so the render checks it first. Raw HTML is the usual
//! cause: the backend passes it through.

use quick_xml::XmlVersion;
use quick_xml::errors::{Error, IllFormedError};
use quick_xml::escape::EscapeError;
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// A fragment may have several top-level elements. The `ac:` and `ri:`
/// prefixes need no declaring: a plain `Reader` ignores namespaces.
const ROOT_OPEN: &str = "<root>";

const BARE_AMPERSAND: &str = "`&` does not start a reference; write `&amp;`";

const EXCERPT_CHARS: usize = 40;

/// A warning naming the first reason `xhtml` is not well-formed XML, or `None`
/// if it is.
pub(crate) fn warning(xhtml: &str) -> Option<String> {
    let (reason, offset) = first_error(xhtml)?;
    let before = &xhtml[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    let rest = xhtml[offset..].lines().next().unwrap_or_default();
    let mut excerpt = String::new();
    for c in rest.chars().take(EXCERPT_CHARS) {
        // A control character would act on the terminal the warning goes to.
        if c.is_control() {
            excerpt.extend(c.escape_unicode());
        } else {
            excerpt.push(c);
        }
    }
    if rest.chars().nth(EXCERPT_CHARS).is_some() {
        excerpt.push('…');
    }
    Some(format!(
        "page.xhtml is not well-formed XML: {reason}, at line {line}, column {column}: \
         `{excerpt}` — Confluence will reject it"
    ))
}

/// The first reason `xhtml` is not XML, and the byte offset in `xhtml` of the
/// markup at fault.
fn first_error(xhtml: &str) -> Option<(String, usize)> {
    let markup = markup_error(xhtml);
    let end = markup.as_ref().map_or(xhtml.len(), |(_, at)| *at);
    xhtml[..end]
        .char_indices()
        .find(|&(_, c)| !is_xml_char(c))
        .map(|(at, c)| (not_allowed(c), at))
        .or(markup)
}

fn markup_error(xhtml: &str) -> Option<(String, usize)> {
    let wrapped = format!("{ROOT_OPEN}{xhtml}</root>");
    let mut reader = Reader::from_str(&wrapped);
    // The wrapper's own start tag.
    reader.read_event().ok()?;

    // Where each open element's start tag is, because an element that is never
    // closed is noticed only at a later end tag, which is not the mistake.
    let mut open = Vec::new();
    let content_end = (ROOT_OPEN.len() + xhtml.len()) as u64;
    let (reason, at) = loop {
        let start = reader.buffer_position();
        let event = reader.read_event();
        // A tag cut off at the end of the page reads the wrapper's end tag as
        // part of itself, leaving no end tag to mismatch.
        if event.is_ok() && start < content_end && reader.buffer_position() > content_end {
            break ("a tag is not closed".to_owned(), start);
        }
        match event {
            Ok(Event::Start(tag)) => {
                if let Some(reason) = attribute_error(&tag) {
                    break (reason, start);
                }
                open.push(start);
            }
            Ok(Event::Empty(tag)) => {
                if let Some(reason) = attribute_error(&tag) {
                    break (reason, start);
                }
            }
            Ok(Event::End(_)) => {
                open.pop();
            }
            Ok(Event::GeneralRef(reference)) => match reference.resolve_char_ref() {
                Ok(Some(c)) if !is_xml_char(c) => break (not_allowed(c), start),
                Ok(_) => {}
                Err(error) => break (error.to_string(), start),
            },
            Ok(Event::Eof) => return None,
            Ok(_) => {}
            Err(Error::IllFormed(IllFormedError::MismatchedEndTag { expected, found })) => {
                break match open.last() {
                    Some(&at) => (format!("`<{expected}>` is not closed"), at),
                    None => (
                        format!("`</{found}>` closes no open element"),
                        reader.error_position(),
                    ),
                };
            }
            Err(Error::IllFormed(IllFormedError::UnclosedReference)) => {
                break (BARE_AMPERSAND.to_owned(), reader.error_position());
            }
            Err(error) => break (error.to_string(), reader.error_position()),
        }
    };
    let offset = usize::try_from(at)
        .unwrap_or(usize::MAX)
        .saturating_sub(ROOT_OPEN.len())
        .min(xhtml.len());
    Some((reason, xhtml.floor_char_boundary(offset)))
}

fn attribute_error(tag: &BytesStart) -> Option<String> {
    for attribute in tag.attributes() {
        let attribute = match attribute {
            Ok(attribute) => attribute,
            Err(error) => {
                let reason = match error {
                    AttrError::ExpectedEq(_) | AttrError::ExpectedValue(_) => {
                        "an attribute has no value"
                    }
                    AttrError::UnquotedValue(_) => "an attribute value is not quoted",
                    AttrError::ExpectedQuote(..) => "an attribute value has no closing quote",
                    AttrError::Duplicated(..) => "an attribute is repeated",
                };
                return Some(reason.to_owned());
            }
        };
        if attribute.value.contains('<') {
            return Some("an attribute value contains `<`; write `&lt;`".to_owned());
        }
        // Accept any entity name to avoid false positives: storage format
        // knows HTML's entities.
        match attribute.normalized_value_with(XmlVersion::Implicit1_0, 1, |_| Some("")) {
            Ok(value) => {
                if let Some(c) = value.chars().find(|&c| !is_xml_char(c)) {
                    return Some(not_allowed(c));
                }
            }
            Err(Error::Escape(EscapeError::UnterminatedEntity(_))) => {
                return Some(BARE_AMPERSAND.to_owned());
            }
            Err(error) => return Some(error.to_string()),
        }
    }
    None
}

/// XML 1.0's `Char`, which Confluence enforces. quick-xml follows XML 1.1,
/// which allows control characters.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' '..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

fn not_allowed(c: char) -> String {
    format!("U+{:04X} is not a character XML allows", u32::from(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(xhtml: &str) -> String {
        warning(xhtml).expect("not well-formed")
    }

    #[test]
    fn storage_markup_is_well_formed() {
        assert_eq!(
            warning(concat!(
                r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#,
                r#"<p>a &amp; b &nbsp; &hearts; &#x27;<br /></p>"#,
                r#"<ac:image><ri:attachment ri:filename="a&amp;b.png" /></ac:image>"#,
                r#"<ac:structured-macro ac:name="code"><ac:plain-text-body>"#,
                r#"<![CDATA[a & <b>]]></ac:plain-text-body></ac:structured-macro>"#,
                "<!-- a comment -->",
            )),
            None,
        );
    }

    #[test]
    fn an_unclosed_element_is_named_at_its_start_tag() {
        assert_eq!(
            reason("<p>one</p>\n<p>two <em>open\n</p>"),
            "page.xhtml is not well-formed XML: `<em>` is not closed, \
             at line 2, column 8: `<em>open` — Confluence will reject it",
        );
    }

    #[test]
    fn an_element_left_open_at_the_end_is_named_at_its_start_tag() {
        assert_eq!(
            reason("<ul><li>one</li><li>two</ul>"),
            "page.xhtml is not well-formed XML: `<li>` is not closed, \
             at line 1, column 17: `<li>two</ul>` — Confluence will reject it",
        );
        assert_eq!(
            reason("<p>unclosed"),
            "page.xhtml is not well-formed XML: `<p>` is not closed, \
             at line 1, column 1: `<p>unclosed` — Confluence will reject it",
        );
    }

    /// A tag cut off by the end of the page would otherwise swallow the
    /// wrapper's end tag and leave nothing to mismatch.
    #[test]
    fn a_tag_cut_off_at_the_end_is_named() {
        assert_eq!(
            reason("<p>x</p><p"),
            "page.xhtml is not well-formed XML: a tag is not closed, \
             at line 1, column 9: `<p` — Confluence will reject it",
        );
        assert_eq!(
            reason("<div>\n<p"),
            "page.xhtml is not well-formed XML: a tag is not closed, \
             at line 2, column 1: `<p` — Confluence will reject it",
        );
        assert_eq!(
            reason("<p>x</p>\n<img src=\"a.png\""),
            "page.xhtml is not well-formed XML: a tag is not closed, \
             at line 2, column 1: `<img src=\"a.png\"` — Confluence will reject it",
        );
    }

    #[test]
    fn a_less_than_sign_in_an_attribute_value_is_named_at_its_tag() {
        assert_eq!(
            reason(r#"<abbr title="a<b">x</abbr>"#),
            "page.xhtml is not well-formed XML: an attribute value contains `<`; \
             write `&lt;`, at line 1, column 1: `<abbr title=\"a<b\">x</abbr>` \
             — Confluence will reject it",
        );
    }

    #[test]
    fn an_end_tag_without_a_start_tag_is_named() {
        assert_eq!(
            reason("<p>x</p></div>"),
            "page.xhtml is not well-formed XML: `</div>` closes no open element, \
             at line 1, column 9: `</div>` — Confluence will reject it",
        );
    }

    #[test]
    fn an_attribute_without_a_value_is_named_at_its_tag() {
        assert_eq!(
            reason(r#"<p><input type="checkbox" disabled /></p>"#),
            "page.xhtml is not well-formed XML: an attribute has no value, \
             at line 1, column 4: `<input type=\"checkbox\" disabled /></p>` — Confluence will reject it",
        );
    }

    #[test]
    fn an_unquoted_attribute_value_is_named_at_its_tag() {
        assert_eq!(
            reason("<table><tr><td colspan=2>x</td></tr></table>"),
            "page.xhtml is not well-formed XML: an attribute value is not quoted, \
             at line 1, column 12: `<td colspan=2>x</td></tr></table>` — Confluence will reject it",
        );
    }

    #[test]
    fn a_duplicated_attribute_is_named_at_its_tag() {
        assert_eq!(
            reason(r#"<p class="a" class="b">x</p>"#),
            "page.xhtml is not well-formed XML: an attribute is repeated, \
             at line 1, column 1: `<p class=\"a\" class=\"b\">x</p>` — Confluence will reject it",
        );
    }

    #[test]
    fn a_bare_ampersand_in_text_is_named() {
        assert_eq!(
            reason("<p>Q & A</p>"),
            "page.xhtml is not well-formed XML: `&` does not start a reference; \
             write `&amp;`, at line 1, column 6: `& A</p>` — Confluence will reject it",
        );
    }

    #[test]
    fn a_bare_ampersand_in_an_attribute_is_named_at_its_tag() {
        assert_eq!(
            reason(r#"<a href="?a=1&b=2">x</a>"#),
            "page.xhtml is not well-formed XML: `&` does not start a reference; \
             write `&amp;`, at line 1, column 1: `<a href=\"?a=1&b=2\">x</a>` — Confluence will reject it",
        );
    }

    #[test]
    fn an_invalid_character_reference_is_named() {
        assert_eq!(
            reason("<p>a&#0;b</p>"),
            "page.xhtml is not well-formed XML: invalid character reference: \
             0x0 character is not permitted in XML, \
             at line 1, column 5: `&#0;b</p>` — Confluence will reject it",
        );
    }

    /// XML 1.0 forbids most control characters, written or referenced;
    /// quick-xml, following XML 1.1, rejects only NUL.
    #[test]
    fn a_control_character_is_named() {
        assert_eq!(
            reason("<p>a\u{1}b</p>"),
            "page.xhtml is not well-formed XML: U+0001 is not a character XML allows, \
             at line 1, column 5: `\\u{1}b</p>` — Confluence will reject it",
        );
        assert_eq!(
            reason("<p>a&#1;b</p>"),
            "page.xhtml is not well-formed XML: U+0001 is not a character XML allows, \
             at line 1, column 5: `&#1;b</p>` — Confluence will reject it",
        );
        assert_eq!(
            reason(r#"<p title="&#x1F;">x</p>"#),
            "page.xhtml is not well-formed XML: U+001F is not a character XML allows, \
             at line 1, column 1: `<p title=\"&#x1F;\">x</p>` — Confluence will reject it",
        );
        assert_eq!(warning("<p>\t\r\n\u{FFFD}\u{10000}&#9;&#xD7FF;</p>"), None);
    }

    #[test]
    fn the_earliest_error_is_named() {
        assert_eq!(
            reason("<p>a</p></p>\u{FFFE}"),
            "page.xhtml is not well-formed XML: `</p>` closes no open element, \
             at line 1, column 9: `</p>\u{FFFE}` — Confluence will reject it",
        );
        assert_eq!(
            reason("<p>\u{FFFF}</p></p>"),
            "page.xhtml is not well-formed XML: U+FFFF is not a character XML allows, \
             at line 1, column 4: `\u{FFFF}</p></p>` — Confluence will reject it",
        );
    }

    /// The excerpt is one line, so the CLI prints the warning on one line.
    #[test]
    fn the_excerpt_stops_at_the_end_of_its_line_and_after_forty_characters() {
        assert_eq!(
            reason("<div>\n<img src=\"a.png\"\nalt=\"x\">\n</div>"),
            "page.xhtml is not well-formed XML: `<img>` is not closed, \
             at line 2, column 1: `<img src=\"a.png\"` — Confluence will reject it",
        );
        assert_eq!(
            reason(&format!("<p>{}", "é".repeat(50))),
            format!(
                "page.xhtml is not well-formed XML: `<p>` is not closed, \
                 at line 1, column 1: `<p>{}…` — Confluence will reject it",
                "é".repeat(37),
            ),
        );
    }

    #[test]
    fn other_errors_keep_the_parsers_reason() {
        assert_eq!(
            reason("<p>x</p><!-- open"),
            "page.xhtml is not well-formed XML: syntax error: comment not closed: \
             `-->` not found before end of input, \
             at line 1, column 9: `<!-- open` — Confluence will reject it",
        );
    }
}
