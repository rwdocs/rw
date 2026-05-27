//! Settings for [`MarkdownRenderer`](crate::MarkdownRenderer).
//!
//! [`RenderConfig`] holds the configuration the renderer was built with —
//! base paths, GFM/wikilink flags, sections, title resolver. It is non-generic
//! on purpose: none of the read-only helpers reference the backend type `B`.
//! Settings live for the lifetime of the renderer; per-render scratch state
//! lives on `Walker` and is freshly constructed for every call.

use std::borrow::Cow;
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
pub trait TitleResolver {
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
    /// - Resolving current-section wikilinks (`[[::path]]`) via
    ///   `resolve_wikilink` — backend-agnostic.
    ///
    /// Non-HTML backends (Confluence, search) ignore the URL-resolution
    /// use but still benefit from wikilink resolution when this is set.
    pub(crate) base_path: Option<String>,
    /// Origin prefix (with trailing slash) for files outside `source_dir`.
    /// Set by [`with_origin`](crate::MarkdownRenderer::with_origin).
    pub(crate) origin_prefix: Option<String>,
    /// GFM features (tables, strikethrough, tasklists) enabled.
    pub(crate) gfm: bool,
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
    /// Defaults: GFM on, no wikilinks, no title extraction.
    pub(crate) fn new() -> Self {
        Self {
            base_path: None,
            origin_prefix: None,
            gfm: true,
            wikilinks: false,
            extract_title: false,
            sections: None,
            title_resolver: None,
        }
    }

    /// Returns pulldown-cmark `Options` reflecting the current GFM and
    /// wikilink configuration.
    #[must_use]
    pub(crate) fn parser_options(&self) -> Options {
        let mut opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
        if self.gfm {
            opts |= Options::ENABLE_TABLES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_GFM;
        }
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

    /// Build ref data attributes for a resolved path, if applicable.
    ///
    /// Returns `None` for:
    /// - External or relative links (not starting with `/`)
    /// - No section registry configured (`with_sections` not called)
    /// - Links not matching any section
    ///
    /// Returns `Some((section_ref_string, section_path))` for internal links matching a section.
    pub(crate) fn section_ref_attrs(&self, href: &str) -> Option<(String, String)> {
        if !href.starts_with('/') {
            return None;
        }
        let sp = self.sections.as_ref()?.find(href)?;
        Some((sp.section.to_string(), sp.path.to_owned()))
    }

    /// Strip the origin prefix from a URL if it matches.
    ///
    /// For files outside `source_dir` (e.g., README.md at the project root),
    /// relative links like `docs/guide.md` include the source directory name.
    /// This strips that prefix so the link resolves correctly in URL space.
    pub(crate) fn strip_origin<'a>(&self, url: &'a str) -> Cow<'a, str> {
        if let Some(prefix) = &self.origin_prefix
            && let Some(stripped) = url.strip_prefix(prefix.as_str())
        {
            return Cow::Borrowed(stripped);
        }
        Cow::Borrowed(url)
    }

    /// Resolve a wikilink `dest_url` to a `WikilinkResolution`.
    pub(crate) fn resolve_wikilink(&self, dest_url: &str) -> WikilinkResolution {
        if let Some(fragment) = dest_url.strip_prefix('#') {
            return WikilinkResolution::Fragment(fragment.to_owned());
        }

        let resolved = self
            .sections
            .as_ref()
            .and_then(|s| s.resolve_refpath(dest_url, self.base_path.as_deref()));

        match resolved {
            Some((href, sp)) => WikilinkResolution::Resolved {
                href,
                section_ref: sp.section.to_string(),
                section_name: sp.section.name.clone(),
                subpath: sp.path.to_owned(),
            },
            None => WikilinkResolution::Broken {
                raw_target: dest_url.to_owned(),
            },
        }
    }

    /// Get display text for a resolved wikilink.
    pub(crate) fn wikilink_display_text(&self, resolution: &WikilinkResolution) -> String {
        match resolution {
            WikilinkResolution::Broken { raw_target } => raw_target.clone(),
            WikilinkResolution::Fragment(fragment) => fragment.replace('-', " "),
            WikilinkResolution::Resolved {
                href,
                subpath,
                section_name,
                ..
            } => {
                if let Some(resolver) = &self.title_resolver {
                    let path = href.strip_prefix('/').unwrap_or(href);
                    let path = match path.find('#') {
                        Some(pos) => &path[..pos],
                        None => path,
                    };
                    if let Some(title) = resolver.resolve_title(path) {
                        return title;
                    }
                }

                if !subpath.is_empty() {
                    // unwrap: rsplit always yields at least one element
                    return subpath.rsplit('/').next().unwrap().to_owned();
                }

                if !section_name.is_empty() {
                    return section_name.clone();
                }

                href.clone()
            }
        }
    }
}

/// Result of resolving a wikilink target.
#[derive(Debug)]
pub(crate) enum WikilinkResolution {
    /// Successfully resolved to a concrete href with section metadata.
    Resolved {
        href: String,
        section_ref: String,
        section_name: String,
        subpath: String,
    },
    /// Fragment-only link (`#heading`) — same page, no section resolution.
    Fragment(String),
    /// Target could not be resolved — render as broken link.
    Broken { raw_target: String },
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
    fn parser_options_disables_gfm_when_flag_off() {
        let mut c = cfg();
        c.gfm = false;
        let opts = c.parser_options();
        assert!(!opts.contains(Options::ENABLE_TABLES));
        assert!(!opts.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(!opts.contains(Options::ENABLE_TASKLISTS));
        assert!(!opts.contains(Options::ENABLE_GFM));
        // Metadata blocks always on, independent of GFM.
        assert!(opts.contains(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS));
    }

    #[test]
    fn parser_options_enables_wikilinks_when_flag_on() {
        let mut c = cfg();
        c.wikilinks = true;
        assert!(c.parser_options().contains(Options::ENABLE_WIKILINKS));
    }

    #[test]
    fn strip_origin_no_prefix_returns_borrowed_url_unchanged() {
        let c = cfg();
        let result = c.strip_origin("docs/guide.md");
        assert_eq!(result, "docs/guide.md");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn strip_origin_matching_prefix_strips_and_borrows() {
        let mut c = cfg();
        c.origin_prefix = Some("docs/".to_owned());
        let result = c.strip_origin("docs/guide.md");
        assert_eq!(result, "guide.md");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn strip_origin_non_matching_prefix_returns_borrowed_url_unchanged() {
        let mut c = cfg();
        c.origin_prefix = Some("docs/".to_owned());
        let result = c.strip_origin("other/page.md");
        assert_eq!(result, "other/page.md");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn section_ref_attrs_non_internal_returns_none() {
        let c = cfg();
        assert!(c.section_ref_attrs("https://example.com").is_none());
        assert!(c.section_ref_attrs("./relative.md").is_none());
    }

    #[test]
    fn section_ref_attrs_no_sections_returns_none() {
        let c = cfg();
        // /abs/path with no sections registry → None.
        assert!(c.section_ref_attrs("/anything").is_none());
    }

    #[test]
    fn resolve_wikilink_fragment_returns_fragment_variant() {
        let c = cfg();
        match c.resolve_wikilink("#some-fragment") {
            WikilinkResolution::Fragment(s) => assert_eq!(s, "some-fragment"),
            other => panic!("expected Fragment, got {other:?}"),
        }
    }

    #[test]
    fn resolve_wikilink_no_sections_returns_broken() {
        let c = cfg();
        match c.resolve_wikilink("domain:billing::overview") {
            WikilinkResolution::Broken { raw_target } => {
                assert_eq!(raw_target, "domain:billing::overview");
            }
            other => panic!("expected Broken, got {other:?}"),
        }
    }

    #[test]
    fn wikilink_display_text_fragment_replaces_dashes_with_spaces() {
        let c = cfg();
        let res = WikilinkResolution::Fragment("hello-world-now".to_owned());
        assert_eq!(c.wikilink_display_text(&res), "hello world now");
    }

    #[test]
    fn wikilink_display_text_broken_returns_raw_target() {
        let c = cfg();
        let res = WikilinkResolution::Broken {
            raw_target: "broken/target".to_owned(),
        };
        assert_eq!(c.wikilink_display_text(&res), "broken/target");
    }

    #[test]
    fn wikilink_display_text_resolved_uses_subpath_basename_when_no_resolver() {
        let c = cfg();
        let res = WikilinkResolution::Resolved {
            href: "/foo/bar".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: "foo/bar".to_owned(),
        };
        assert_eq!(c.wikilink_display_text(&res), "bar");
    }

    #[test]
    fn wikilink_display_text_resolved_falls_back_to_section_name_when_subpath_empty() {
        let c = cfg();
        let res = WikilinkResolution::Resolved {
            href: "/foo".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: String::new(),
        };
        assert_eq!(c.wikilink_display_text(&res), "billing");
    }

    #[test]
    fn section_ref_attrs_internal_path_with_matching_section_returns_attrs() {
        use rw_sections::{Namespace, Section, Sections};

        let mut sections_map = std::collections::HashMap::new();
        sections_map.insert(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        );
        let sections = std::sync::Arc::new(Sections::new(sections_map));

        let mut c = cfg();
        c.sections = Some(sections);

        let result = c.section_ref_attrs("/domains/billing/overview");
        let (section_ref, section_path) = result.expect("internal link should resolve");
        assert_eq!(section_ref, "domain:default/billing");
        assert_eq!(section_path, "overview");
    }

    #[test]
    fn wikilink_display_text_resolved_uses_resolver_when_present() {
        struct StaticResolver;
        impl TitleResolver for StaticResolver {
            fn resolve_title(&self, path: &str) -> Option<String> {
                if path == "domains/billing/overview" {
                    Some("Billing Overview".to_owned())
                } else {
                    None
                }
            }
        }

        let mut c = cfg();
        c.title_resolver = Some(Box::new(StaticResolver));

        let res = WikilinkResolution::Resolved {
            href: "/domains/billing/overview".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: "domains/billing/overview".to_owned(),
        };
        assert_eq!(c.wikilink_display_text(&res), "Billing Overview");
    }

    #[test]
    fn wikilink_display_text_resolved_falls_through_when_resolver_returns_none() {
        struct AlwaysNoneResolver;
        impl TitleResolver for AlwaysNoneResolver {
            fn resolve_title(&self, _path: &str) -> Option<String> {
                None
            }
        }

        let mut c = cfg();
        c.title_resolver = Some(Box::new(AlwaysNoneResolver));

        let res = WikilinkResolution::Resolved {
            href: "/foo/bar".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: "foo/bar".to_owned(),
        };
        // Resolver returned None → falls through to subpath basename.
        assert_eq!(c.wikilink_display_text(&res), "bar");
    }
}
