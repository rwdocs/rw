//! Confluence backend for markdown rendering.
//!
//! Produces Confluence XHTML storage format for the REST API.

use std::fmt::Write;

use crate::backend::RenderBackend;
use crate::state::escape_html;

/// Confluence render backend.
///
/// Produces Confluence XHTML storage format with:
/// - `ac:structured-macro` for code blocks
/// - Info panel macro for blockquotes
/// - `ac:image` with `ri:url` or `ri:attachment` for images
/// - Title extraction from first H1 with level shifting
pub struct ConfluenceBackend;

impl RenderBackend for ConfluenceBackend {
    const TITLE_AS_METADATA: bool = true;

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        out.push_str(r#"<ac:structured-macro ac:name="code" ac:schema-version="1">"#);
        if let Some(lang) = lang {
            write!(
                out,
                r#"<ac:parameter ac:name="language">{}</ac:parameter>"#,
                escape_html(lang)
            )
            .unwrap();
        }
        out.push_str(r#"<ac:parameter ac:name="linenumbers">true</ac:parameter>"#);
        // CDATA content is not escaped
        write!(
            out,
            r"<ac:plain-text-body><![CDATA[{content}]]></ac:plain-text-body>"
        )
        .unwrap();
        out.push_str("</ac:structured-macro>");
    }

    fn blockquote_start(out: &mut String) {
        out.push_str(
            r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body>"#,
        );
    }

    fn blockquote_end(out: &mut String) {
        out.push_str("</ac:rich-text-body></ac:structured-macro>");
    }

    fn image(src: &str, _alt: &str, _title: &str, out: &mut String) {
        // Confluence doesn't use alt/title attributes in the same way
        let is_external = src.starts_with("http://") || src.starts_with("https://");
        let inner = if is_external {
            format!(r#"ri:url ri:value="{}""#, escape_html(src))
        } else {
            // Local file - assume it will be uploaded as attachment
            let filename = src.rsplit('/').next().unwrap_or(src);
            format!(r#"ri:attachment ri:filename="{}""#, escape_html(filename))
        };
        write!(out, "<ac:image><{inner} /></ac:image>").unwrap();
    }

    fn hard_break(out: &mut String) {
        out.push_str("<br />");
    }

    fn horizontal_rule(out: &mut String) {
        out.push_str("<hr />");
    }

    fn task_list_marker(checked: bool, out: &mut String) {
        out.push_str(if checked { "[x] " } else { "[ ] " });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_with_language() {
        let mut out = String::new();
        ConfluenceBackend::code_block(Some("python"), "print('hello')", &mut out);
        assert!(out.contains(r#"ac:name="code""#));
        assert!(out.contains(r#"ac:name="language">python"#));
        assert!(out.contains("print('hello')"));
        assert!(out.contains("<![CDATA["));
    }

    #[test]
    fn test_code_block_without_language() {
        let mut out = String::new();
        ConfluenceBackend::code_block(None, "plain code", &mut out);
        assert!(out.contains(r#"ac:name="code""#));
        assert!(!out.contains(r#"ac:name="language""#));
        assert!(out.contains("plain code"));
    }

    #[test]
    fn test_blockquote() {
        let mut out = String::new();
        ConfluenceBackend::blockquote_start(&mut out);
        out.push_str("content");
        ConfluenceBackend::blockquote_end(&mut out);
        assert!(out.contains(r#"ac:name="info""#));
        assert!(out.contains("<ac:rich-text-body>content</ac:rich-text-body>"));
    }

    #[test]
    fn test_external_image() {
        let mut out = String::new();
        ConfluenceBackend::image("https://example.com/image.png", "alt", "title", &mut out);
        assert!(out.contains(r"<ac:image>"));
        assert!(out.contains(r#"ri:url ri:value="https://example.com/image.png""#));
    }

    #[test]
    fn test_local_image() {
        let mut out = String::new();
        ConfluenceBackend::image("./images/diagram.png", "alt", "title", &mut out);
        assert!(out.contains(r"<ac:image>"));
        assert!(out.contains(r#"ri:attachment ri:filename="diagram.png""#));
    }

    #[test]
    fn test_hard_break() {
        let mut out = String::new();
        ConfluenceBackend::hard_break(&mut out);
        assert_eq!(out, "<br />");
    }

    #[test]
    fn test_horizontal_rule() {
        let mut out = String::new();
        ConfluenceBackend::horizontal_rule(&mut out);
        assert_eq!(out, "<hr />");
    }

    #[test]
    fn test_task_list_marker() {
        let mut out = String::new();
        ConfluenceBackend::task_list_marker(false, &mut out);
        assert_eq!(out, "[ ] ");

        let mut out = String::new();
        ConfluenceBackend::task_list_marker(true, &mut out);
        assert_eq!(out, "[x] ");
    }
}
