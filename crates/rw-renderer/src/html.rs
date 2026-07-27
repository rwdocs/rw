//! HTML backend for markdown rendering.
//!
//! [`HtmlBackend`] produces semantic HTML5 suitable for the RW web viewer.
//! It resolves relative `.md` links to clean URL paths and renders
//! GitHub-style alerts with SVG icons from the [Octicons] set.
//!
//! [Octicons]: https://primer.style/octicons/

use std::borrow::Cow;
use std::fmt::Write;

use crate::status::StatusColor;
use crate::tabs::TabInfo;

use base64::prelude::{BASE64_STANDARD, Engine};
use rw_diagrams::{Asset, DiagramContent, Size};

use crate::backend::RenderBackend;
use crate::diagram::{DiagramView, splice_link_attrs, write_diagram_id_attr};
use crate::util::escape_into;
use rw_parser::AlertKind;

// SVG icons for alerts (GitHub Octicons-style, 16x16)
const SVG_INFO: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8Zm8-6.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM6.5 7.75A.75.75 0 0 1 7.25 7h1a.75.75 0 0 1 .75.75v2.75h.25a.75.75 0 0 1 0 1.5h-2a.75.75 0 0 1 0-1.5h.25v-2h-.25a.75.75 0 0 1-.75-.75ZM8 6a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;
const SVG_LIGHTBULB: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 1.5c-2.363 0-4 1.69-4 3.75 0 .984.424 1.625.984 2.304l.214.253c.223.264.47.556.673.848.284.411.537.896.621 1.49a.75.75 0 0 1-1.484.211c-.04-.282-.163-.547-.37-.847a8.456 8.456 0 0 0-.542-.68c-.084-.1-.173-.205-.268-.32C3.201 7.75 2.5 6.766 2.5 5.25 2.5 2.31 4.863 0 8 0s5.5 2.31 5.5 5.25c0 1.516-.701 2.5-1.328 3.259-.095.115-.184.22-.268.319-.207.245-.383.453-.541.681-.208.3-.33.565-.37.847a.751.751 0 0 1-1.485-.212c.084-.593.337-1.078.621-1.489.203-.292.45-.584.673-.848.075-.088.147-.173.213-.253.561-.679.985-1.32.985-2.304 0-2.06-1.637-3.75-4-3.75ZM5.75 12h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1 0-1.5ZM6 15.25a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 0 1.5h-2.5a.75.75 0 0 1-.75-.75Z"></path></svg>"#;
const SVG_REPORT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v9.5A1.75 1.75 0 0 1 14.25 13H8.06l-2.573 2.573A1.458 1.458 0 0 1 3 14.543V13H1.75A1.75 1.75 0 0 1 0 11.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm7 2.25v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 9a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_ALERT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_STOP: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4.47.22A.749.749 0 0 1 5 0h6c.199 0 .389.079.53.22l4.25 4.25c.141.14.22.331.22.53v6a.749.749 0 0 1-.22.53l-4.25 4.25A.749.749 0 0 1 11 16H5a.749.749 0 0 1-.53-.22L.22 11.53A.749.749 0 0 1 0 11V5c0-.199.079-.389.22-.53Zm.84 1.28L1.5 5.31v5.38l3.81 3.81h5.38l3.81-3.81V5.31L10.69 1.5ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;

/// [`RenderBackend`] implementation that produces semantic HTML5.
///
/// Code blocks use `<pre><code>` with a `language-*` class for syntax
/// highlighting, images use `<img>`, and relative `.md` links are resolved
/// to clean URL paths (e.g., `./sibling.md` → `/base/path/sibling`).
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Providers};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_base_path("/docs/guide")
///     .render("[Setup](./setup.md#install)", &Providers::empty());
///
/// assert!(result.html.contains(r#"href="/docs/guide/setup#install""#));
/// ```
pub struct HtmlBackend;

/// Writes ` width="…" height="…"` for a diagram whose display size is known.
///
/// An unknown size writes nothing, giving an unsized `<img>`: a raster diagram
/// whose header would not parse still renders, just without the aspect-ratio
/// box that keeps surrounding content from reflowing once it decodes.
fn write_size_attrs(size: Option<Size>, out: &mut String) {
    if let Some(Size { width, height }) = size {
        write!(out, r#" width="{width}" height="{height}""#).unwrap();
    }
}

impl RenderBackend for HtmlBackend {
    const TITLE_AS_METADATA: bool = false;

    /// Renders a status badge as a colored pill span.
    ///
    /// `color` is a closed enum ([`StatusColor`]), never raw interpolated
    /// attribute text, so there is no value that can inject markup into the
    /// class attribute — see the `status_open`/`status_close` tests below.
    fn status_open(color: StatusColor, out: &mut String) {
        write!(out, r#"<span class="status status-{color}">"#).unwrap();
    }

    fn status_close(out: &mut String) {
        out.push_str("</span>");
    }

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        if let Some(lang) = lang {
            out.push_str(r#"<pre><code class="language-"#);
            escape_into(lang, out);
            out.push_str(r#"">"#);
            escape_into(content, out);
            out.push_str("</code></pre>");
        } else {
            out.push_str("<pre><code>");
            escape_into(content, out);
            out.push_str("</code></pre>");
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
        out.push_str(r#"<img src=""#);
        escape_into(src, out);
        out.push('"');
        if !title.is_empty() {
            out.push_str(r#" title=""#);
            escape_into(title, out);
            out.push('"');
        }
        out.push_str(r#" alt=""#);
        escape_into(alt, out);
        out.push_str(r#"">"#);
    }

    /// Renders a diagram as a `<figure class="diagram">`.
    ///
    /// An inline SVG is wrapped in `<rw-diagram>`, which the viewer upgrades
    /// into a shadow root. Diagram generators emit ids that are unique only
    /// within one SVG (Vega hard-codes `clip0, clip1, …`; Mermaid roots every
    /// SVG on `container`), so without a per-diagram tree scope a `url(#clip1)`
    /// reference resolves document-wide to whichever diagram came first —
    /// silently painting one diagram with another's clip paths. The other two
    /// shapes hold an `<img>`, which has no ids to collide.
    fn diagram(view: &DiagramView<'_>, out: &mut String) {
        out.push_str(r#"<figure class="diagram""#);
        write_diagram_id_attr(view.id, out);
        out.push('>');
        match view.asset {
            Asset::Inline(DiagramContent::Svg(svg)) => {
                out.push_str("<rw-diagram>");
                splice_link_attrs(svg, view.links, out);
                out.push_str("</rw-diagram>");
            }
            Asset::Inline(DiagramContent::Png(bytes)) => {
                out.push_str(r#"<img src="data:image/png;base64,"#);
                BASE64_STANDARD.encode_string(bytes, out);
                out.push('"');
                write_size_attrs(view.size, out);
                out.push_str(r#" alt="diagram">"#);
            }
            Asset::Reference(name) => {
                out.push_str(r#"<img src=""#);
                escape_into(name, out);
                out.push('"');
                write_size_attrs(view.size, out);
                out.push_str(r#" alt="diagram">"#);
            }
        }
        out.push_str("</figure>");
    }

    fn table_start(out: &mut String) {
        out.push_str(
            r#"<div class="table-wrap" role="group" tabindex="0" aria-label="Table"><table>"#,
        );
    }

    fn table_end(out: &mut String) {
        out.push_str("</tbody></table></div>");
    }

    fn transform_link<'a>(url: &'a str, base_path: Option<&str>) -> Cow<'a, str> {
        match base_path {
            Some(base) => Cow::Owned(resolve_link(url, base)),
            None => Cow::Borrowed(url),
        }
    }

    fn tabs_open(group_id: usize, tabs: &[TabInfo], out: &mut String) {
        let _ = write!(out, r#"<div class="tabs" id="tabs-{group_id}">"#);
        out.push_str(r#"<div class="tabs-buttons" role="tablist">"#);
        for tab in tabs {
            let selected = tab.is_first;
            let _ = write!(
                out,
                r#"<button role="tab" id="tab-{group_id}-{0}" aria-controls="panel-{group_id}-{0}" aria-selected="{selected}" tabindex="{tabindex}">"#,
                tab.id,
                selected = selected,
                tabindex = if selected { "0" } else { "-1" },
            );
            escape_into(&tab.label, out);
            out.push_str("</button>");
        }
        out.push_str("</div>");
    }

    fn tab_panel_open(group_id: usize, tab: &TabInfo, out: &mut String) {
        let hidden = if tab.is_first { "" } else { " hidden" };
        let _ = write!(
            out,
            r#"<div role="tabpanel" id="panel-{group_id}-{}" aria-labelledby="tab-{group_id}-{}"{hidden}>"#,
            tab.id, tab.id
        );
    }

    fn tab_panel_close(out: &mut String) {
        out.push_str("</div>");
    }

    fn tabs_close(out: &mut String) {
        out.push_str("</div>");
    }
}

/// Resolve a markdown link URL relative to a base URL path (with leading `/`).
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
    use crate::tabs::TabInfo;

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
                "/domains/billing/systems/payment-gateway/adr"
            ),
            "/domains/billing/systems/payment-gateway/adr/adr-101"
        );
    }

    #[test]
    fn test_resolve_link_parent() {
        assert_eq!(
            resolve_link("../other.md", "/domains/billing/guide"),
            "/domains/billing/other"
        );
    }

    #[test]
    fn test_resolve_link_current_dir() {
        assert_eq!(
            resolve_link("./sibling.md", "/domains/billing/guide"),
            "/domains/billing/guide/sibling"
        );
    }

    #[test]
    fn test_resolve_link_external_unchanged() {
        assert_eq!(
            resolve_link("https://example.com", "/base/path"),
            "https://example.com"
        );
        assert_eq!(
            resolve_link("mailto:test@example.com", "/base/path"),
            "mailto:test@example.com"
        );
    }

    #[test]
    fn test_resolve_link_fragment_only() {
        assert_eq!(resolve_link("#section", "/base/path"), "#section");
    }

    #[test]
    fn test_resolve_link_with_fragment() {
        assert_eq!(
            resolve_link("./page.md#section", "/base/path"),
            "/base/path/page#section"
        );
    }

    #[test]
    fn test_resolve_link_non_md_unchanged() {
        assert_eq!(resolve_link("./image.png", "/base/path"), "./image.png");
    }

    #[test]
    fn test_resolve_link_absolute() {
        assert_eq!(
            resolve_link("/absolute/path.md", "/base/path"),
            "/absolute/path"
        );
    }

    #[test]
    fn test_resolve_link_traversal_clamped() {
        assert_eq!(
            resolve_link("../../../etc/passwd.md", "/a/b"),
            "/etc/passwd"
        );
    }

    #[test]
    fn test_transform_link_with_base_path() {
        let result = HtmlBackend::transform_link("./page.md", Some("/base/path"));
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

    #[test]
    fn status_open_emits_colored_span() {
        let mut out = String::new();
        HtmlBackend::status_open(StatusColor::Green, &mut out);
        assert_eq!(out, r#"<span class="status status-green">"#);
    }

    // `StatusColor` is a closed enum, not a raw string, so `status_open`
    // cannot be handed markup to inject into the class attribute — there is
    // no variant whose `Display` output is anything but one of the fixed
    // `status-<color>` tokens exercised here and below.
    #[test]
    fn status_open_renders_grey() {
        let mut out = String::new();
        HtmlBackend::status_open(StatusColor::Grey, &mut out);
        assert_eq!(out, r#"<span class="status status-grey">"#);
    }

    #[test]
    fn status_close_emits_span_close() {
        let mut out = String::new();
        HtmlBackend::status_close(&mut out);
        assert_eq!(out, "</span>");
    }

    #[test]
    fn tabs_open_matches_legacy_bar_markup() {
        let tabs = [
            TabInfo {
                id: 0,
                label: "macOS".to_owned(),
                is_first: true,
            },
            TabInfo {
                id: 1,
                label: "Linux".to_owned(),
                is_first: false,
            },
        ];
        let mut out = String::new();
        HtmlBackend::tabs_open(0, &tabs, &mut out);
        assert_eq!(
            out,
            r#"<div class="tabs" id="tabs-0"><div class="tabs-buttons" role="tablist"><button role="tab" id="tab-0-0" aria-controls="panel-0-0" aria-selected="true" tabindex="0">macOS</button><button role="tab" id="tab-0-1" aria-controls="panel-0-1" aria-selected="false" tabindex="-1">Linux</button></div>"#
        );
    }

    #[test]
    fn tab_panel_open_hidden_for_non_first() {
        let mut out = String::new();
        HtmlBackend::tab_panel_open(
            0,
            &TabInfo {
                id: 1,
                label: "L".to_owned(),
                is_first: false,
            },
            &mut out,
        );
        assert_eq!(
            out,
            r#"<div role="tabpanel" id="panel-0-1" aria-labelledby="tab-0-1" hidden>"#
        );
    }

    #[test]
    fn tab_panel_open_not_hidden_for_first() {
        let mut out = String::new();
        HtmlBackend::tab_panel_open(
            0,
            &TabInfo {
                id: 0,
                label: "L".to_owned(),
                is_first: true,
            },
            &mut out,
        );
        assert_eq!(
            out,
            r#"<div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0">"#
        );
    }

    #[test]
    fn tab_panel_close_emits_div_close() {
        let mut out = String::new();
        HtmlBackend::tab_panel_close(&mut out);
        assert_eq!(out, "</div>");
    }

    #[test]
    fn tabs_close_emits_div_close() {
        let mut out = String::new();
        HtmlBackend::tabs_close(&mut out);
        assert_eq!(out, "</div>");
    }

    #[test]
    fn tabs_open_escapes_label() {
        let tabs = [TabInfo {
            id: 0,
            label: "a < b & c".to_owned(),
            is_first: true,
        }];
        let mut out = String::new();
        HtmlBackend::tabs_open(0, &tabs, &mut out);
        assert!(out.contains("a &lt; b &amp; c"), "got: {out}");
    }

    /// The three diagram figure shapes are served to browsers with CSS and JS
    /// bound to their exact markup, so every assertion below is byte-exact.
    mod diagram {
        use crate::DiagramLink;

        use super::*;

        fn render(view: &DiagramView<'_>) -> String {
            let mut out = String::new();
            HtmlBackend::diagram(view, &mut out);
            out
        }

        fn svg(id: Option<&str>, source: &str, links: &[DiagramLink]) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Inline(DiagramContent::Svg(source.to_owned())),
                size: None,
                links,
            })
        }

        fn png(id: Option<&str>, bytes: &[u8], size: Option<Size>) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Inline(DiagramContent::Png(bytes.to_vec())),
                size,
                links: &[],
            })
        }

        fn reference(id: Option<&str>, name: &str, size: Option<Size>) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Reference(name.to_owned()),
                size,
                links: &[],
            })
        }

        #[test]
        fn an_inline_svg_is_wrapped_in_rw_diagram() {
            assert_eq!(
                svg(None, "<svg><g/></svg>", &[]),
                r#"<figure class="diagram"><rw-diagram><svg><g/></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn an_inline_svg_carries_its_diagram_id() {
            assert_eq!(
                svg(Some("diagram-2"), "<svg/>", &[]),
                r#"<figure class="diagram" data-diagram-id="diagram-2"><rw-diagram><svg/></rw-diagram></figure>"#
            );
        }

        #[test]
        fn a_diagram_id_is_attribute_escaped() {
            assert_eq!(
                svg(Some(r#"a"b"#), "<svg/>", &[]),
                r#"<figure class="diagram" data-diagram-id="a&quot;b"><rw-diagram><svg/></rw-diagram></figure>"#
            );
        }

        #[test]
        fn resolved_links_are_spliced_into_the_svg() {
            let links = [DiagramLink {
                href: "/domains/billing/api".to_owned(),
                section_ref: "domain:default/billing".to_owned(),
                section_path: "api".to_owned(),
            }];
            assert_eq!(
                svg(
                    None,
                    r#"<svg><a href="/domains/billing/api">x</a></svg>"#,
                    &links
                ),
                r#"<figure class="diagram"><rw-diagram><svg><a href="/domains/billing/api" data-section-ref="domain:default/billing" data-section-path="api">x</a></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn an_svg_with_no_resolved_links_is_untouched() {
            assert_eq!(
                svg(None, r#"<svg><a href="/x">x</a></svg>"#, &[]),
                r#"<figure class="diagram"><rw-diagram><svg><a href="/x">x</a></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn inline_png_bytes_become_a_sized_data_uri() {
            assert_eq!(
                png(
                    Some("d1"),
                    b"PNG",
                    Some(Size {
                        width: 200,
                        height: 100
                    })
                ),
                concat!(
                    r#"<figure class="diagram" data-diagram-id="d1">"#,
                    r#"<img src="data:image/png;base64,UE5H" width="200" height="100" alt="diagram">"#,
                    "</figure>",
                )
            );
        }

        /// A PNG whose header would not parse has no size to report. It renders
        /// unsized rather than not at all.
        #[test]
        fn an_unsized_png_renders_without_dimension_attributes() {
            assert_eq!(
                png(None, b"PNG", None),
                r#"<figure class="diagram"><img src="data:image/png;base64,UE5H" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn a_reference_points_at_the_written_name() {
            assert_eq!(
                reference(
                    Some("d1"),
                    "diagram_abc123.png",
                    Some(Size {
                        width: 640,
                        height: 480
                    })
                ),
                concat!(
                    r#"<figure class="diagram" data-diagram-id="d1">"#,
                    r#"<img src="diagram_abc123.png" width="640" height="480" alt="diagram">"#,
                    "</figure>",
                )
            );
        }

        #[test]
        fn an_unsized_reference_renders_without_dimension_attributes() {
            assert_eq!(
                reference(None, "diagram_abc123.png", None),
                r#"<figure class="diagram"><img src="diagram_abc123.png" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn a_reference_name_is_attribute_escaped() {
            assert_eq!(
                reference(None, r#"a"b.png"#, None),
                r#"<figure class="diagram"><img src="a&quot;b.png" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn an_error_figure_carries_the_escaped_message() {
            let mut out = String::new();
            HtmlBackend::diagram_error(Some("d1"), "syntax error at <line 3>", &mut out);
            assert_eq!(
                out,
                concat!(
                    r#"<figure class="diagram diagram-error" data-diagram-id="d1">"#,
                    "<pre>Diagram rendering failed: syntax error at &lt;line 3&gt;</pre>",
                    "</figure>",
                )
            );
        }

        #[test]
        fn an_error_figure_without_an_id_omits_the_attribute() {
            let mut out = String::new();
            HtmlBackend::diagram_error(None, "boom", &mut out);
            assert_eq!(
                out,
                r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: boom</pre></figure>"#
            );
        }

        /// An unresolved diagram fence stays a code block.
        #[test]
        fn an_unresolved_fence_renders_as_a_code_block() {
            let mut out = String::new();
            HtmlBackend::diagram_source("plantuml", "@startuml\n@enduml", &mut out);
            assert_eq!(
                out,
                "<pre><code class=\"language-plantuml\">@startuml\n@enduml</code></pre>"
            );
        }
    }
}
