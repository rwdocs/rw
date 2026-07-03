//! Extract the commentable-text string from rendered HTML.
//!
//! The viewer's anchoring algorithm (`packages/viewer/src/lib/anchoring.ts`)
//! matches quotes against the article's text stream. The quote resolver needs
//! the same stream so the offsets it computes line up with what the browser
//! sees — including that both sides exclude rendered diagrams: the viewer's
//! `buildTextIndex` filters out `figure.diagram` subtrees, so this must too, or
//! a CLI-created comment could anchor to diagram text the browser can't reach.

/// Flatten rendered HTML to the commentable-text string.
///
/// Concatenates descendant text nodes in document order like
/// `Node.textContent` — tags and HTML comments stripped, entities decoded,
/// inter-block whitespace kept — with one deliberate exception: the contents of
/// any `<figure class="…diagram…">` are omitted, matching the viewer's
/// `figure.diagram` comment-exclusion boundary (see [`is_diagram_figure_open`]).
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

        let Some(gt) = rest.find('>') else {
            rest = ""; // unterminated tag at EOF: drop it, like the browser does
            break;
        };
        let tag = &rest[..gt];
        let after = &rest[gt + 1..];

        // A diagram figure (`<figure class="…diagram…">`) and everything inside
        // it is excluded, matching the viewer's `figure.diagram` boundary. This
        // covers inlined SVG labels AND the `diagram-error` <pre> message.
        if is_diagram_figure_open(tag) {
            rest = skip_to_figure_close(after);
            continue;
        }

        rest = after;
    }
    push_decoded(&mut out, rest);
    out
}

/// True for an opening `<figure>` tag carrying the class *token* `diagram`,
/// mirroring the viewer's `figure.diagram` boundary. A substring check would
/// wrongly match `class="diagrammatic"` or a stray "diagram" in another
/// attribute; token matching keeps Rust and the browser in agreement. The
/// renderer emits double-quoted class attributes (`class="diagram"` /
/// `class="diagram diagram-error"`).
fn is_diagram_figure_open(tag: &str) -> bool {
    if !tag.starts_with("<figure") {
        return false;
    }
    let Some(class_start) = tag.find("class=\"") else {
        return false;
    };
    let after = &tag[class_start + "class=\"".len()..];
    let value = after.find('"').map_or(after, |end| &after[..end]);
    value.split_whitespace().any(|token| token == "diagram")
}

/// Given the text just after a `<figure …>` open tag, return the slice after the
/// matching `</figure>`, honoring nested `<figure>` opens with a depth counter.
fn skip_to_figure_close(after_open: &str) -> &str {
    let mut rest = after_open;
    let mut depth = 1usize;
    while depth > 0 {
        let Some(lt) = rest.find('<') else {
            return "";
        };
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        if rest.starts_with("</figure") {
            depth -= 1;
        } else if rest.starts_with("<figure") {
            depth += 1;
        }
        rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
    }
    rest
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

    #[test]
    fn diagram_figure_svg_text_is_excluded() {
        let html = r#"<p>before</p><figure class="diagram"><svg><text>Billing</text></svg></figure><p>after</p>"#;
        let text = html_to_text_content(html);
        assert!(!text.contains("Billing"), "diagram label leaked: {text:?}");
        assert!(text.contains("before") && text.contains("after"));
    }

    #[test]
    fn diagram_error_pre_is_excluded() {
        let html = r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: boom</pre></figure>"#;
        assert_eq!(html_to_text_content(html), "");
    }

    #[test]
    fn png_diagram_figure_is_a_no_op() {
        let html = r#"<figure class="diagram"><img src="data:image/png;base64,AAAA" alt="diagram"></figure>"#;
        assert_eq!(html_to_text_content(html), "");
    }

    #[test]
    fn non_diagram_figure_is_kept() {
        let html = r#"<figure class="photo"><figcaption>Hello</figcaption></figure>"#;
        assert_eq!(html_to_text_content(html), "Hello");
    }

    #[test]
    fn nested_figures_inside_a_diagram_are_all_skipped() {
        let html = r#"<figure class="diagram"><figure class="inner"><text>no</text></figure></figure><p>ok</p>"#;
        assert_eq!(html_to_text_content(html), "ok");
    }

    #[test]
    fn figure_with_diagram_substring_in_another_attr_is_kept() {
        // Only the class *token* `diagram` marks a diagram figure, matching the
        // browser's `figure.diagram`. A stray "diagram" elsewhere must not skip.
        let html = r#"<figure class="photo" title="pipeline diagram"><figcaption>Cap</figcaption></figure>"#;
        assert_eq!(html_to_text_content(html), "Cap");
    }

    #[test]
    fn figure_with_diagram_like_class_token_is_kept() {
        let html = r#"<figure class="diagrammatic"><figcaption>X</figcaption></figure>"#;
        assert_eq!(html_to_text_content(html), "X");
    }

    #[test]
    fn unterminated_tag_at_eof_is_dropped_not_leaked() {
        // Matches the browser: an unclosed tag at EOF contributes no text; the
        // literal "<span" must not leak into the output.
        assert_eq!(html_to_text_content("hello <span"), "hello ");
    }
}
