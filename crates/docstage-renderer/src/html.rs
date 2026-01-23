//! HTML backend for markdown rendering.
//!
//! Produces semantic HTML5 output suitable for web display.

use std::borrow::Cow;
use std::fmt::Write;

use crate::backend::{AlertKind, RenderBackend};
use crate::state::escape_html;

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
            AlertKind::Note => ("note", "ℹ️", "Note"),
            AlertKind::Tip => ("tip", "💡", "Tip"),
            AlertKind::Important => ("important", "❗", "Important"),
            AlertKind::Warning => ("warning", "⚠️", "Warning"),
            AlertKind::Caution => ("caution", "🔴", "Caution"),
        };
        write!(
            out,
            r#"<div class="alert alert-{class}"><div class="alert-title"><span class="alert-icon">{icon}</span>{title}</div><div class="alert-content">"#
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
        return url.to_string();
    }

    // Only process markdown links
    if !url.ends_with(".md") && !url.contains(".md#") {
        return url.to_string();
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
        path_part.trim_start_matches('/').to_string()
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
        assert!(out.contains("ℹ️"));
        assert!(out.contains("Note"));
        assert!(out.contains("<p>content</p>"));
    }

    #[test]
    fn test_alert_tip() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Tip, &mut out);
        HtmlBackend::alert_end(AlertKind::Tip, &mut out);
        assert!(out.contains(r#"class="alert alert-tip""#));
        assert!(out.contains("💡"));
        assert!(out.contains("Tip"));
    }

    #[test]
    fn test_alert_important() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Important, &mut out);
        HtmlBackend::alert_end(AlertKind::Important, &mut out);
        assert!(out.contains(r#"class="alert alert-important""#));
        assert!(out.contains("❗"));
        assert!(out.contains("Important"));
    }

    #[test]
    fn test_alert_warning() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Warning, &mut out);
        HtmlBackend::alert_end(AlertKind::Warning, &mut out);
        assert!(out.contains(r#"class="alert alert-warning""#));
        assert!(out.contains("⚠️"));
        assert!(out.contains("Warning"));
    }

    #[test]
    fn test_alert_caution() {
        let mut out = String::new();
        HtmlBackend::alert_start(AlertKind::Caution, &mut out);
        HtmlBackend::alert_end(AlertKind::Caution, &mut out);
        assert!(out.contains(r#"class="alert alert-caution""#));
        assert!(out.contains("🔴"));
        assert!(out.contains("Caution"));
    }
}
