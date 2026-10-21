//! Wikilink resolution and display-text helpers.
//!
//! Used by [`Walker`](crate::walker) when a parser emits a
//! [`Tag::Link`](rw_parser::Tag::Link) with
//! [`LinkKind::Wiki`](rw_parser::LinkKind::Wiki). Plain markdown
//! link helpers live in the sibling [`link`](crate::link) module.

use std::borrow::Cow;

use percent_encoding::percent_decode_str;

use crate::config::RenderConfig;
use crate::link::encode_url_path;

/// Result of resolving a wikilink target.
///
/// [`Fragment`](Self::Fragment) and [`Broken`](Self::Broken) carry no copy of
/// the target: the raw `dest_url` the caller already holds *is* the href
/// (fragment) or the display text (broken), so [`display_text`] takes it
/// alongside the resolution.
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
    Fragment,
    /// Target could not be resolved — render as broken link.
    Broken,
}

/// Resolve a wikilink target to a [`WikilinkResolution`].
///
/// Fragment-only targets (`#heading`) always resolve to
/// [`WikilinkResolution::Fragment`]. Any other target requires a sections
/// registry on `cfg`; without one the result is
/// [`WikilinkResolution::Broken`]. Current-section links (`[[::path]]`)
/// additionally need `cfg.base_path`.
pub(crate) fn resolve(cfg: &RenderConfig, dest_url: &str) -> WikilinkResolution {
    if dest_url.starts_with('#') {
        return WikilinkResolution::Fragment;
    }

    let Some((section_root, sp, fragment)) = cfg
        .sections
        .as_ref()
        .and_then(|s| s.resolve_refpath(dest_url, cfg.base_path.as_deref()))
    else {
        return WikilinkResolution::Broken;
    };

    let (subpath, query) = sp.path.split_at(sp.path.find('?').unwrap_or(sp.path.len()));
    let (page, subpath) = published_page(cfg, section_root, subpath);
    let mut href = format!("/{}{query}", encode_url_path(&page));
    if let Some(fragment) = fragment {
        href.push('#');
        href.push_str(fragment);
    }
    WikilinkResolution::Resolved {
        href,
        section_ref: sp.section.to_string(),
        section_name: sp.section.name.clone(),
        subpath: format!("{}{query}", encode_url_path(&subpath)),
    }
}

/// The page `subpath` names in the section at `section_root`, and its path in
/// that section; moved to the page published from the file `<page>.md` when the
/// link resolver knows one.
fn published_page(cfg: &RenderConfig, section_root: &str, subpath: &str) -> (String, String) {
    let path = match (section_root, subpath) {
        (root, "") => root.to_owned(),
        ("", subpath) => subpath.to_owned(),
        (root, subpath) => format!("{root}/{subpath}"),
    };
    let page = cfg
        .link_resolver
        .as_deref()
        .and_then(|resolver| resolver.page_url(&format!("/{path}.md")))
        .filter(|page| *page != path);
    let Some(page) = page else {
        return (path, subpath.to_owned());
    };
    let subpath = page
        .strip_prefix(section_root)
        .map_or(subpath, |rest| rest.trim_start_matches('/'))
        .to_owned();
    (page, subpath)
}

/// Return the display text to render for a wikilink, given its resolution and
/// the raw `dest_url` it was resolved from.
///
/// For [`WikilinkResolution::Resolved`] the priority is: title resolver (when
/// configured) → last segment of `subpath` → `section_name` → raw `href`.
/// [`WikilinkResolution::Fragment`] strips the `#` and replaces `-` with
/// spaces; broken targets render `dest_url` as written. Borrowed where the
/// text already exists.
pub(crate) fn display_text<'a>(
    cfg: &RenderConfig,
    resolution: &'a WikilinkResolution,
    dest_url: &'a str,
) -> Cow<'a, str> {
    match resolution {
        WikilinkResolution::Broken => Cow::Borrowed(dest_url),
        WikilinkResolution::Fragment => {
            let fragment = dest_url.strip_prefix('#').unwrap_or(dest_url);
            Cow::Owned(fragment.replace('-', " "))
        }
        WikilinkResolution::Resolved {
            href,
            subpath,
            section_name,
            ..
        } => {
            // `resolve` encodes the page path and leaves `?` and `#` only as
            // delimiters, so decoding the part before them is exact.
            if let Some(resolver) = &cfg.title_resolver {
                let path = href[1..].split(['?', '#']).next().unwrap_or_default();
                let path = percent_decode_str(path).decode_utf8_lossy();
                if let Some(title) = resolver.resolve_title(&path) {
                    return Cow::Owned(title);
                }
            }

            let subpath = subpath.split('?').next().unwrap_or_default();
            if !subpath.is_empty() {
                // unwrap: rsplit always yields at least one element
                let name = subpath.rsplit('/').next().unwrap();
                return percent_decode_str(name).decode_utf8_lossy();
            }

            if !section_name.is_empty() {
                return Cow::Borrowed(section_name);
            }

            Cow::Borrowed(href)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use rw_sections::{Namespace, Section, Sections};

    use super::*;
    use crate::link::Pages;

    fn cfg() -> RenderConfig {
        RenderConfig::default()
    }

    #[test]
    fn resolve_no_sections_returns_broken() {
        let c = cfg();
        assert!(matches!(
            resolve(&c, "domain:billing::overview"),
            WikilinkResolution::Broken
        ));
    }

    #[test]
    fn page_paths_are_encoded() {
        let mut c = cfg();
        c.sections = Some(Arc::new(Sections::with_implicit_root(
            HashMap::from([(
                "reserved/sys#1".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    namespace: Namespace::default(),
                    name: "sys1".to_owned(),
                },
            )]),
            Namespace::default(),
        )));
        c.base_path = Some("/reserved/sys#1/api".to_owned());
        c.link_resolver = Some(Box::new(Pages(&[(
            "reserved/sys#1/index.md",
            "reserved/sys#1",
        )])));
        for (target, expected_href, expected_subpath, display) in [
            (
                "system:sys1::api#top",
                "/reserved/sys%231/api#top",
                "api",
                "api",
            ),
            ("system:sys1", "/reserved/sys%231", "", "sys1"),
            ("::index?x", "/reserved/sys%231?x", "?x", "sys1"),
            (
                "::100%/a?x",
                "/reserved/sys%231/100%25/a?x",
                "100%25/a?x",
                "a",
            ),
            ("::100%", "/reserved/sys%231/100%25", "100%25", "100%"),
            (
                "::my гайд",
                "/reserved/sys%231/my%20%D0%B3%D0%B0%D0%B9%D0%B4",
                "my%20%D0%B3%D0%B0%D0%B9%D0%B4",
                "my гайд",
            ),
        ] {
            let resolution = resolve(&c, target);
            let WikilinkResolution::Resolved { href, subpath, .. } = &resolution else {
                panic!("{target} should resolve");
            };
            assert_eq!(
                (href.as_str(), subpath.as_str()),
                (expected_href, expected_subpath),
                "{target}"
            );
            assert_eq!(display_text(&c, &resolution, target), display, "{target}");
        }
    }

    #[test]
    fn display_text_resolved_uses_subpath_basename_when_no_resolver() {
        let c = cfg();
        let res = WikilinkResolution::Resolved {
            href: "/foo/bar".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: "foo/bar".to_owned(),
        };
        assert_eq!(display_text(&c, &res, "unused-for-resolved"), "bar");
    }

    #[test]
    fn display_text_resolved_falls_back_to_section_name_when_subpath_empty() {
        let c = cfg();
        let res = WikilinkResolution::Resolved {
            href: "/foo".to_owned(),
            section_ref: "domain:billing".to_owned(),
            section_name: "billing".to_owned(),
            subpath: String::new(),
        };
        assert_eq!(display_text(&c, &res, "unused-for-resolved"), "billing");
    }
}
