//! Render backend trait for format-specific rendering.
//!
//! This trait abstracts the differences between HTML and Confluence output formats,
//! allowing the main renderer to be generic over the output format.

use std::borrow::Cow;

/// Backend trait for format-specific rendering operations.
///
/// Implementations provide format-specific rendering for:
/// - Code blocks (HTML uses `<pre><code>`, Confluence uses `ac:structured-macro`)
/// - Blockquotes (HTML uses `<blockquote>`, Confluence uses info panel macro)
/// - Images (HTML uses `<img>`, Confluence uses `ac:image`)
/// - Link transformation (HTML resolves relative `.md` links)
pub trait RenderBackend {
    /// Whether to skip first H1 in output and shift heading levels.
    ///
    /// - `true` (Confluence): First H1 becomes page title, not rendered. H2→H1, H3→H2, etc.
    /// - `false` (HTML): First H1 is rendered normally, no level shifting.
    const TITLE_AS_METADATA: bool;

    /// Render a code block.
    ///
    /// # Arguments
    ///
    /// * `lang` - Optional language identifier (e.g., "rust", "python")
    /// * `content` - The code content
    /// * `out` - Output buffer to write to
    fn code_block(lang: Option<&str>, content: &str, out: &mut String);

    /// Render blockquote start tag.
    fn blockquote_start(out: &mut String);

    /// Render blockquote end tag.
    fn blockquote_end(out: &mut String);

    /// Render an image.
    ///
    /// # Arguments
    ///
    /// * `src` - Image source URL
    /// * `alt` - Alt text for the image
    /// * `title` - Optional title attribute
    /// * `out` - Output buffer to write to
    fn image(src: &str, alt: &str, title: &str, out: &mut String);

    /// Transform a link URL.
    ///
    /// Default implementation returns the URL unchanged.
    /// HTML backend overrides this to resolve relative `.md` links.
    ///
    /// # Arguments
    ///
    /// * `url` - The original link URL
    /// * `base_path` - Optional base path for resolving relative links
    #[must_use] 
    fn transform_link<'a>(url: &'a str, _base_path: Option<&str>) -> Cow<'a, str> {
        Cow::Borrowed(url)
    }

    /// Render a hard break.
    ///
    /// Default uses `<br>`. Override for format-specific rendering (e.g., `<br />`).
    fn hard_break(out: &mut String) {
        out.push_str("<br>");
    }

    /// Render a horizontal rule.
    ///
    /// Default uses `<hr>`. Override for format-specific rendering (e.g., `<hr />`).
    fn horizontal_rule(out: &mut String) {
        out.push_str("<hr>");
    }

    /// Render a task list marker.
    ///
    /// Default uses HTML checkbox. Override for format-specific rendering.
    fn task_list_marker(checked: bool, out: &mut String) {
        if checked {
            out.push_str(r#"<input type="checkbox" checked disabled> "#);
        } else {
            out.push_str(r#"<input type="checkbox" disabled> "#);
        }
    }
}
