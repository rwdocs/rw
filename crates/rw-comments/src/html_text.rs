//! Extract the textContent-equivalent string from rendered HTML.
//!
//! The viewer's anchoring algorithm (`packages/viewer/src/lib/anchoring.ts`)
//! matches quotes against `article.textContent` — the flattened text stream
//! of the rendered HTML. The quote resolver needs the same stream so the
//! offsets it computes line up with what the browser sees.

/// Flatten rendered HTML to a textContent-equivalent string.
///
/// Concatenates all descendant text nodes in document order, matching the
/// DOM `Node.textContent` specification: tags and HTML comments are
/// stripped, entities are decoded, and whitespace between blocks is kept
/// as-is.
///
/// The input is trusted — it comes from our own pulldown-cmark renderer,
/// so tags are well-formed and `<` never appears inside an attribute.
pub(crate) fn html_to_text_content(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        push_decoded(&mut out, &rest[..lt]);
        rest = &rest[lt..];

        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }

        rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
    }
    push_decoded(&mut out, rest);
    out
}

fn push_decoded(out: &mut String, chunk: &str) {
    if chunk.contains('&') {
        out.push_str(&html_escape::decode_html_entities(chunk));
    } else {
        out.push_str(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(html_to_text_content("hello world"), "hello world");
    }

    #[test]
    fn tags_are_stripped() {
        assert_eq!(
            html_to_text_content("<p>hello <strong>world</strong></p>"),
            "hello world"
        );
    }

    #[test]
    fn multiple_paragraphs_preserve_whitespace_between() {
        let html = "<p>foo</p>\n<p>bar</p>";
        let text = html_to_text_content(html);
        assert!(text.contains("foo"));
        assert!(text.contains("bar"));
        assert_ne!(text.find("foo"), text.find("bar"));
    }

    #[test]
    fn nested_inline_formatting_flattens() {
        let html = "<p>The <em>foo</em> does <code>X</code>, Y, Z.</p>";
        assert_eq!(html_to_text_content(html), "The foo does X, Y, Z.");
    }

    #[test]
    fn code_block_content_is_included() {
        let html =
            r#"<pre><code class="language-rust"><span>let</span> <span>x</span> = 1;</code></pre>"#;
        assert_eq!(html_to_text_content(html), "let x = 1;");
    }

    #[test]
    fn empty_input_produces_empty_output() {
        assert_eq!(html_to_text_content(""), "");
    }

    #[test]
    fn html_comments_are_skipped() {
        assert_eq!(
            html_to_text_content("<p>foo <!-- hidden --> baz</p>"),
            "foo  baz"
        );
    }

    #[test]
    fn named_entities_are_decoded() {
        assert_eq!(
            html_to_text_content("<p>Tom &amp; Jerry</p>"),
            "Tom & Jerry"
        );
    }

    #[test]
    fn numeric_entities_are_decoded() {
        assert_eq!(html_to_text_content("<p>rocket &#x1F680;</p>"), "rocket 🚀");
    }
}
