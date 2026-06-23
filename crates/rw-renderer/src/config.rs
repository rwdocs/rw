//! Settings for [`MarkdownRenderer`](crate::MarkdownRenderer).
//!
//! [`RenderConfig`] holds the configuration the renderer was built with —
//! base paths, the wikilink flag, sections, title resolver. It is non-generic
//! on purpose: none of the read-only helpers reference the backend type `B`.
//! Settings live for the lifetime of the renderer; per-render scratch state
//! lives on `Walker` and is freshly constructed for every call.

use std::sync::Arc;

use pulldown_cmark::{Options, Parser};
use rw_sections::Sections;

/// Resolves page paths to their display titles for wikilink rendering.
///
/// When a wikilink like `[[domain:billing::overview]]` has no explicit display
/// text, the renderer calls this trait to look up a human-readable title.
/// If the resolver returns `None`, the renderer falls back to the last path
/// segment.
///
/// # Examples
///
/// ```
/// use rw_renderer::TitleResolver;
///
/// struct MapResolver(std::collections::HashMap<String, String>);
///
/// impl TitleResolver for MapResolver {
///     fn resolve_title(&self, path: &str) -> Option<String> {
///         self.0.get(path).cloned()
///     }
/// }
///
/// let mut titles = std::collections::HashMap::new();
/// titles.insert("domains/billing/overview".into(), "Billing Overview".into());
/// let resolver = MapResolver(titles);
///
/// assert_eq!(
///     resolver.resolve_title("domains/billing/overview"),
///     Some("Billing Overview".into()),
/// );
/// assert_eq!(resolver.resolve_title("unknown/page"), None);
/// ```
pub trait TitleResolver: Send + Sync {
    /// Returns the display title for a page at `path`, or `None` if unknown.
    ///
    /// `path` is an absolute path without leading slash
    /// (e.g., `"domains/billing/overview"`).
    fn resolve_title(&self, path: &str) -> Option<String>;
}

/// Configuration for [`MarkdownRenderer`](crate::MarkdownRenderer).
///
/// Built up via the renderer's `with_*` builders and read by both the
/// renderer's pipeline methods and the `Walker` during event processing.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RenderConfig {
    /// Base path used for two purposes:
    /// - Resolving relative URLs in `HtmlBackend::transform_link`.
    /// - Resolving current-section wikilinks (`[[::path]]`) in
    ///   [`wikilink::resolve`](crate::wikilink::resolve); backend-agnostic.
    ///
    /// Non-HTML backends (Confluence, search) ignore the URL-resolution
    /// use but still benefit from wikilink resolution when this is set.
    pub(crate) base_path: Option<String>,
    /// Origin prefix (with trailing slash) for files outside `source_dir`.
    /// Set by [`with_origin`](crate::MarkdownRenderer::with_origin).
    pub(crate) origin_prefix: Option<String>,
    /// True when the current page's URL denotes a directory (`index.md` or the
    /// root/README homepage) rather than a single file (a leaf `name.md`). Leaf
    /// pages resolve relative links against their *containing directory*, so the
    /// page's own URL slug is dropped from the link base. Defaults to `true`.
    pub(crate) is_dir: bool,
    /// `[[wikilink]]` parsing enabled.
    pub(crate) wikilinks: bool,
    /// Extract title from first H1.
    pub(crate) extract_title: bool,
    /// Section registry for wikilink resolution and link annotation.
    pub(crate) sections: Option<Arc<Sections>>,
    /// Title resolver for wikilink display text.
    pub(crate) title_resolver: Option<Box<dyn TitleResolver>>,
}

impl RenderConfig {
    /// Defaults: no wikilinks, no title extraction.
    pub(crate) fn new() -> Self {
        Self {
            base_path: None,
            origin_prefix: None,
            is_dir: true,
            wikilinks: false,
            extract_title: false,
            sections: None,
            title_resolver: None,
        }
    }

    /// Returns pulldown-cmark `Options`: the always-on GFM features and
    /// metadata blocks, plus wikilinks when configured.
    #[must_use]
    pub(crate) fn parser_options(&self) -> Options {
        let mut opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_GFM;
        if self.wikilinks {
            opts |= Options::ENABLE_WIKILINKS;
        }
        opts
    }

    /// Creates a pulldown-cmark `Parser` with the renderer's current options.
    #[must_use]
    pub(crate) fn create_parser<'a>(&self, markdown: &'a str) -> Parser<'a> {
        Parser::new_ext(markdown, self.parser_options())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Options;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
    }

    #[test]
    fn parser_options_defaults_include_gfm_and_metadata() {
        let opts = cfg().parser_options();
        assert!(opts.contains(Options::ENABLE_TABLES));
        assert!(opts.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(opts.contains(Options::ENABLE_TASKLISTS));
        assert!(opts.contains(Options::ENABLE_GFM));
        assert!(opts.contains(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS));
        assert!(!opts.contains(Options::ENABLE_WIKILINKS));
    }

    #[test]
    fn parser_options_enables_wikilinks_when_flag_on() {
        let mut c = cfg();
        c.wikilinks = true;
        assert!(c.parser_options().contains(Options::ENABLE_WIKILINKS));
    }
}
