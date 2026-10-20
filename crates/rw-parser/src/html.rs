//! Questions about raw HTML that more than one pass over a page asks.

/// Whether `html` holds nothing but HTML comments and whitespace.
///
/// rw-meta and rw-renderer both skip such HTML blocks when looking for the H1
/// that opens a page, so a `<!-- markdownlint-disable MD041 -->` above it does
/// not cost the page its title.
///
/// ```
/// use rw_parser::is_comment_only_html;
///
/// assert!(is_comment_only_html("<!-- a -->\n<!--\nb\n-->\n"));
/// assert!(!is_comment_only_html("<!-- a --><div></div>\n"));
/// ```
#[must_use]
pub fn is_comment_only_html(html: &str) -> bool {
    let mut rest = html.trim_start();
    while !rest.is_empty() {
        let Some(comment) = rest.strip_prefix("<!--") else {
            return false;
        };
        // `<!-->` and `<!--->` are complete comments in CommonMark.
        let after = if let Some(after) = comment.strip_prefix('>') {
            after
        } else if let Some(after) = comment.strip_prefix("->") {
            after
        } else if let Some(end) = comment.find("-->") {
            &comment[end + 3..]
        } else {
            return false;
        };
        rest = after.trim_start();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_whitespace_only() {
        for html in [
            "",
            "\n",
            "<!-- x -->\n",
            "<!-- a -->\n\n<!-- b -->\n",
            "<!--\nline\n-->\n",
            "<!---->",
            "<!-->",
            "<!--->",
            "<!-- a --><!-- b -->",
        ] {
            assert!(is_comment_only_html(html), "{html:?}");
        }
    }

    #[test]
    fn anything_else_is_not() {
        for html in [
            "<div></div>\n",
            "<!-- x --><div></div>\n",
            "<!-- x --> text\n",
            "<!-- unclosed\n",
            "<!-- x -->\n<p>y</p>\n",
        ] {
            assert!(!is_comment_only_html(html), "{html:?}");
        }
    }
}
