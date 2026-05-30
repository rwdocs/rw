//! Path helpers for plain markdown links.
//!
//! Used by [`Walker`](crate::walker) when a parser emits a non-wikilink
//! [`Tag::Link`](pulldown_cmark::Tag::Link) or
//! [`Tag::Image`](pulldown_cmark::Tag::Image). Wikilink-specific resolution
//! lives in the sibling [`wikilink`](crate::wikilink) module.

use std::borrow::Cow;

use crate::config::RenderConfig;

/// Strip the origin prefix from a URL if it matches.
///
/// For files outside `source_dir` (e.g., README.md at the project root),
/// relative links like `docs/guide.md` include the source directory name.
/// This strips that prefix so the link resolves correctly in URL space.
pub(crate) fn strip_origin<'a>(cfg: &RenderConfig, url: &'a str) -> Cow<'a, str> {
    if let Some(prefix) = &cfg.origin_prefix
        && let Some(stripped) = url.strip_prefix(prefix.as_str())
    {
        return Cow::Borrowed(stripped);
    }
    Cow::Borrowed(url)
}

/// Build ref data attributes for a resolved path, if applicable.
///
/// Returns `None` for:
/// - External or relative links (not starting with `/`)
/// - No section registry configured (`with_sections` not called)
/// - Links not matching any section
///
/// Returns `Some((section_ref_string, section_path))` for internal links
/// matching a section.
pub(crate) fn section_ref_attrs(cfg: &RenderConfig, href: &str) -> Option<(String, String)> {
    if !href.starts_with('/') {
        return None;
    }
    let sp = cfg.sections.as_ref()?.find(href)?;
    Some((sp.section.to_string(), sp.path.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
    }

    #[test]
    fn strip_origin_no_prefix_returns_borrowed_url_unchanged() {
        let c = cfg();
        let result = strip_origin(&c, "docs/guide.md");
        assert_eq!(result, "docs/guide.md");
        assert_matches!(result, Cow::Borrowed(_));
    }

    #[test]
    fn strip_origin_matching_prefix_strips_and_borrows() {
        let mut c = cfg();
        c.origin_prefix = Some("docs/".to_owned());
        let result = strip_origin(&c, "docs/guide.md");
        assert_eq!(result, "guide.md");
        assert_matches!(result, Cow::Borrowed(_));
    }

    #[test]
    fn strip_origin_non_matching_prefix_returns_borrowed_url_unchanged() {
        let mut c = cfg();
        c.origin_prefix = Some("docs/".to_owned());
        let result = strip_origin(&c, "other/page.md");
        assert_eq!(result, "other/page.md");
        assert_matches!(result, Cow::Borrowed(_));
    }

    #[test]
    fn section_ref_attrs_non_internal_returns_none() {
        let c = cfg();
        assert!(section_ref_attrs(&c, "https://example.com").is_none());
        assert!(section_ref_attrs(&c, "./relative.md").is_none());
    }

    #[test]
    fn section_ref_attrs_no_sections_returns_none() {
        let c = cfg();
        assert!(section_ref_attrs(&c, "/anything").is_none());
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

        let result = section_ref_attrs(&c, "/domains/billing/overview");
        let (section_ref, section_path) = result.expect("internal link should resolve");
        assert_eq!(section_ref, "domain:default/billing");
        assert_eq!(section_path, "overview");
    }
}
