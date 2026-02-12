//! HTML backend for markdown rendering.
//!
//! Produces semantic HTML5 output suitable for web display.

use std::borrow::Cow;
use std::fmt::Write;

use crate::backend::{AlertKind, RenderBackend};
use crate::state::escape_html;

// SVG icons for alerts (GitHub Octicons-style, 16x16)
const SVG_INFO: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8Zm8-6.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM6.5 7.75A.75.75 0 0 1 7.25 7h1a.75.75 0 0 1 .75.75v2.75h.25a.75.75 0 0 1 0 1.5h-2a.75.75 0 0 1 0-1.5h.25v-2h-.25a.75.75 0 0 1-.75-.75ZM8 6a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;
const SVG_LIGHTBULB: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 1.5c-2.363 0-4 1.69-4 3.75 0 .984.424 1.625.984 2.304l.214.253c.223.264.47.556.673.848.284.411.537.896.621 1.49a.75.75 0 0 1-1.484.211c-.04-.282-.163-.547-.37-.847a8.456 8.456 0 0 0-.542-.68c-.084-.1-.173-.205-.268-.32C3.201 7.75 2.5 6.766 2.5 5.25 2.5 2.31 4.863 0 8 0s5.5 2.31 5.5 5.25c0 1.516-.701 2.5-1.328 3.259-.095.115-.184.22-.268.319-.207.245-.383.453-.541.681-.208.3-.33.565-.37.847a.751.751 0 0 1-1.485-.212c.084-.593.337-1.078.621-1.489.203-.292.45-.584.673-.848.075-.088.147-.173.213-.253.561-.679.985-1.32.985-2.304 0-2.06-1.637-3.75-4-3.75ZM5.75 12h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1 0-1.5ZM6 15.25a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 0 1.5h-2.5a.75.75 0 0 1-.75-.75Z"></path></svg>"#;
const SVG_REPORT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v9.5A1.75 1.75 0 0 1 14.25 13H8.06l-2.573 2.573A1.458 1.458 0 0 1 3 14.543V13H1.75A1.75 1.75 0 0 1 0 11.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm7 2.25v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 9a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_ALERT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_STOP: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4.47.22A.749.749 0 0 1 5 0h6c.199 0 .389.079.53.22l4.25 4.25c.141.14.22.331.22.53v6a.749.749 0 0 1-.22.53l-4.25 4.25A.749.749 0 0 1 11 16H5a.749.749 0 0 1-.53-.22L.22 11.53A.749.749 0 0 1 0 11V5c0-.199.079-.389.22-.53Zm.84 1.28L1.5 5.31v5.38l3.81 3.81h5.38l3.81-3.81V5.31L10.69 1.5ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;

/// HTML render backend.
///
/// Produces semantic HTML5 with:
/// - `<pre><code>` for code blocks
/// - `<blockquote>` for blockquotes
/// - `<img>` for images
/// - Relative `.md` link resolution
pub struct HtmlBackend;

impl RenderBackend for HtmlBackend {
    const TITLE_AS_METADATA: bool = false;

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        if let Some(lang) = lang {
            write!(
                out,
                r#"<pre><code class="language-{}">{}</code></pre>"#,
                escape_html(lang),
                escape_html(content)
            )
            .unwrap();
        } else {
            write!(out, "<pre><code>{}</code></pre>", escape_html(content)).unwrap();
        }
    }

    fn blockquote_start(out: &mut String) {
        out.push_str("<blockquote>");
    }

    fn blockquote_end(out: &mut String) {
        out.push_str("</blockquote>");
    }

    fn alert_start(kind: AlertKind, out: &mut String) {
        let (class, icon, title) = match kind {
            AlertKind::Note => ("note", SVG_INFO, "Note"),
            AlertKind::Tip => ("tip", SVG_LIGHTBULB, "Tip"),
            AlertKind::Important => ("important", SVG_REPORT, "Important"),
            AlertKind::Warning => ("warning", SVG_ALERT, "Warning"),
            AlertKind::Caution => ("caution", SVG_STOP, "Caution"),
        };
        write!(
            out,
            r#"<div class="alert alert-{class}"><div class="alert-title">{icon}{title}</div><div class="alert-content">"#
        )
        .unwrap();
    }

    fn alert_end(_kind: AlertKind, out: &mut String) {
        out.push_str("</div></div>");
    }

    fn image(src: &str, alt: &str, title: &str, out: &mut String) {
        let title_attr = if title.is_empty() {
            String::new()
        } else {
            format!(r#" title="{}""#, escape_html(title))
        };
        write!(
            out,
            r#"<img src="{}"{title_attr} alt="{}">"#,
            escape_html(src),
            escape_html(alt)
        )
        .unwrap();
    }

    fn transform_link<'a>(url: &'a str, base_path: Option<&str>) -> Cow<'a, str> {
        match base_path {
            Some(base) => Cow::Owned(resolve_link(url, base)),
            None => Cow::Borrowed(url),
        }
    }
}

/// Resolve a markdown link URL relative to a base path.
///
/// Transforms relative `.md` links to absolute paths suitable for SPA navigation:
/// - `./sibling.md` → `/base/path/sibling`
/// - `../parent.md` → `/base/parent`
/// - `subdir/page.md` → `/base/path/subdir/page`
/// - `adr-101/index.md` → `/base/path/adr-101`
///
/// External links, fragment-only links, and non-markdown links are returned unchanged.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn resolve_link(url: &str, base_path: &str) -> String {
    // Skip external links, fragments, and non-local URLs
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
        || url.starts_with('#')
    {
        return url.to_owned();
    }

    // Only process markdown links
    if !url.ends_with(".md") && !url.contains(".md#") {
        return url.to_owned();
    }

    // Split URL into path and fragment
    let (path_part, fragment) = if let Some(hash_pos) = url.find('#') {
        (&url[..hash_pos], Some(&url[hash_pos..]))
    } else {
        (url, None)
    };

    // Resolve the path
    let resolved = if path_part.starts_with('/') {
        // Absolute path - strip leading slash since we add /docs/ prefix later
        path_part.trim_start_matches('/').to_owned()
    } else {
        // Relative path - resolve against base
        resolve_relative_path(path_part, base_path)
    };

    // Strip .md extension and /index suffix for clean URLs
    let clean = resolved.strip_suffix(".md").unwrap_or(&resolved);
    let clean = clean.strip_suffix("/index").unwrap_or(clean);

    // Add leading slash and fragment
    let with_prefix = format!("/{clean}");
    match fragment {
        Some(frag) => format!("{with_prefix}{frag}"),
        None => with_prefix,
    }
}

/// Resolve a relative path against a base path.
///
/// Handles `.` (current), `..` (parent), and plain relative paths.
fn resolve_relative_path(relative: &str, base: &str) -> String {
    // Split base into segments (the base is treated as a directory)
    let mut segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();

    // Process each component of the relative path
    for component in relative.split('/') {
        match component {
            "" | "." => {} // Current directory, skip
            ".." => {
                // Parent directory - ignore if already at root to prevent traversal
                segments.pop();
            }
            _ => segments.push(component),
        }
    }

    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_with_language() {
        let mut out = String::new();
        HtmlBackend::code_block(Some("rust"), "fn main() {}", &mut out);
        assert_eq!(
            out,
            r#"<pre><code class="language-rust">fn main() {}</code></pre>"#
        );
    }

    #[test]
    fn test_code_block_without_language() {
        let mut out = String::new();
        HtmlBackend::code_block(None, "plain code", &mut out);
        assert_eq!(out, "<pre><code>plain code</code></pre>");
    }

    #[test]
    fn test_blockquote() {
        let mut out = String::new();
        HtmlBackend::blockquote_start(&mut out);
        out.push_str("content");
        HtmlBackend::blockquote_end(&mut out);
        assert_eq!(out, "<blockquote>content</blockquote>");
    }

    #[test]
    fn test_image() {
        let mut out = String::new();
        HtmlBackend::image("image.png", "Alt text", "", &mut out);
        assert_eq!(out, r#"<img src="image.png" alt="Alt text">"#);
    }

    #[test]
    fn test_image_with_title() {
        let mut out = String::new();
        HtmlBackend::image("image.png", "Alt text", "Image title", &mut out);
        assert_eq!(
            out,
            r#"<img src="image.png" title="Image title" alt="Alt text">"#
        );
    }

    #[test]
    fn test_resolve_link_relative() {
        assert_eq!(
            resolve_link(
                "adr-101/index.md",
                "domains/billing/systems/payment-gateway/adr"
            ),
            "/domains/billing/systems/payment-gateway/adr/adr-101"
        );
    }

    #[test]
    fn test_resolve_link_parent() {
        assert_eq!(
            resolve_link("../other.md", "domains/billing/guide"),
            "/domains/billing/other"
        );
    }

    #[test]
    fn test_resolve_link_current_dir() {
        assert_eq!(
            resolve_link("./sibling.md", "domains/billing/guide"),
            "/domains/billing/guide/sibling"
        );
    }

    #[test]
    fn test_resolve_link_external_unchanged() {
        assert_eq!(
            resolve_link("https://example.com", "base/path"),
            "https://example.com"
        );
        assert_eq!(
            resolve_link("mailto:test@example.com", "base/path"),
            "mailto:test@example.com"
        );
    }

    #[test]
    fn test_resolve_link_fragment_only() {
        assert_eq!(resolve_link("#section", "base/path"), "#section");
    }

    #[test]
    fn test_resolve_link_with_fragment() {
        assert_eq!(
            resolve_link("./page.md#section", "base/path"),
            "/base/path/page#section"
        );
    }

    #[test]
    fn test_resolve_link_non_md_unchanged() {
        assert_eq!(resolve_link("./image.png", "base/path"), "./image.png");
    }

    #[test]
    fn test_resolve_link_absolute() {
        assert_eq!(
            resolve_link("/absolute/path.md", "base/path"),
            "/absolute/path"
        );
    }

    #[test]
    fn test_resolve_link_traversal_clamped() {
        assert_eq!(resolve_link("../../../etc/passwd.md", "a/b"), "/etc/passwd");
    }

    #[test]
    fn test_transform_link_with_base_path() {
        let result = HtmlBackend::transform_link("./page.md", Some("base/path"));
        assert_eq!(result, "/base/path/page");
    }

    #[test]
    fn test_transform_link_without_base_path() {
        let result = HtmlBackend::transform_link("./page.md", None);
        assert_eq!(result, "./page.md");
    }

    #[test]
    fn test_alert_note() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Note, &mut out);
        out.push_str("<p>content</p>");
        HtmlBackend::alert_end(AlertKind::Note, &mut out);
        assert!(out.contains(r#"class="alert alert-note""#));
        assert!(out.contains(r#"<svg class="alert-icon""#));
        assert!(out.contains("Note"));
        assert!(out.contains("<p>content</p>"));
    }

    #[test]
    fn test_alert_tip() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Tip, &mut out);
        HtmlBackend::alert_end(AlertKind::Tip, &mut out);
        assert!(out.contains(r#"class="alert alert-tip""#));
        assert!(out.contains(r#"<svg class="alert-icon""#));
        assert!(out.contains("Tip"));
    }

    #[test]
    fn test_alert_important() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Important, &mut out);
        HtmlBackend::alert_end(AlertKind::Important, &mut out);
        assert!(out.contains(r#"class="alert alert-important""#));
        assert!(out.contains(r#"<svg class="alert-icon""#));
        assert!(out.contains("Important"));
    }

    #[test]
    fn test_alert_warning() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Warning, &mut out);
        HtmlBackend::alert_end(AlertKind::Warning, &mut out);
        assert!(out.contains(r#"class="alert alert-warning""#));
        assert!(out.contains(r#"<svg class="alert-icon""#));
        assert!(out.contains("Warning"));
    }

    #[test]
    fn test_alert_caution() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Caution, &mut out);
        HtmlBackend::alert_end(AlertKind::Caution, &mut out);
        assert!(out.contains(r#"class="alert alert-caution""#));
        assert!(out.contains(r#"<svg class="alert-icon""#));
        assert!(out.contains("Caution"));
    }

}
