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

/// The base path for resolving relative links, corrected for the page's source
/// shape.
///
/// Directory pages (`index.md`, the root/README homepage) resolve relative
/// links against their own URL. Leaf pages (`name.md`) resolve against their
/// *containing directory*, so the page's own final URL segment is dropped —
/// matching `CommonMark`, where `./sibling.md` is a sibling of the source file.
///
/// Kept separate from [`RenderConfig::base_path`], which wikilink resolution
/// reads unchanged; only plain-link resolution uses this corrected base.
/// `/specs/notif` (leaf) -> `/specs`; `/guide` (leaf) -> `/`.
pub(crate) fn link_base(cfg: &RenderConfig) -> Option<&str> {
    let base = cfg.base_path.as_deref()?;
    if cfg.is_dir {
        return Some(base);
    }
    // Drop the page's own final URL segment, keeping the leading slash, so the
    // result is the containing directory. A leaf at the root collapses to `/`.
    match base.trim_end_matches('/').rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => Some(dir),
        _ => Some("/"),
    }
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
    fn link_base_is_dir_returns_base_unchanged() {
        let mut c = cfg();
        c.base_path = Some("/specs/notif".to_owned());
        c.is_dir = true;
        assert_eq!(link_base(&c), Some("/specs/notif"));
    }

    #[test]
    fn link_base_leaf_drops_last_segment() {
        let mut c = cfg();
        c.base_path = Some("/specs/notif".to_owned());
        c.is_dir = false;
        assert_eq!(link_base(&c), Some("/specs"));
    }

    #[test]
    fn link_base_leaf_at_root_drops_to_root() {
        let mut c = cfg();
        c.base_path = Some("/guide".to_owned());
        c.is_dir = false;
        assert_eq!(link_base(&c), Some("/"));
    }

    #[test]
    fn link_base_none_when_unset() {
        let c = cfg();
        assert_eq!(link_base(&c), None);
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
