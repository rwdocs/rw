//! Wikilink resolution and display-text helpers.

use std::borrow::Cow;

use crate::config::RenderConfig;
use crate::link::encode_url_path;

/// Result of resolving a wikilink target.
///
/// [`Fragment`](Self::Fragment) and [`Broken`](Self::Broken) carry no copy of
/// the target; [`display_text`] takes the raw `dest_url` alongside.
#[derive(Debug)]
pub(crate) enum WikilinkResolution {
    /// Successfully resolved to a concrete href with section metadata.
    Resolved {
        href: String,
        section_ref: String,
        subpath: String,
        /// The display text: the target page's title, else the subpath's last
        /// segment, else the section name, else `href`.
        name: String,
    },
    /// Fragment-only link (`#heading`) — same page, no section resolution.
    Fragment,
    /// No section, or no page in it, matches the target; the reason why.
    Broken(String),
}

/// Resolve a wikilink target to a [`WikilinkResolution`].
///
/// Fragment-only targets (`#heading`) always resolve to
/// [`WikilinkResolution::Fragment`]. Any other target requires a sections
/// registry on `cfg`; without one the result is
/// [`WikilinkResolution::Broken`], as is a page the title resolver doesn't
/// know. Current-section links (`[[::path]]`) additionally need
/// `cfg.base_path`.
pub(crate) fn resolve(cfg: &RenderConfig, dest_url: &str) -> WikilinkResolution {
    if dest_url.starts_with('#') {
        return WikilinkResolution::Fragment;
    }

    let Some((section_root, sp, fragment)) = cfg
        .sections
        .as_ref()
        .and_then(|s| s.resolve_refpath(dest_url, cfg.base_path.as_deref()))
    else {
        return WikilinkResolution::Broken("no such section".to_owned());
    };

    let (written, query) = sp.path.split_at(sp.path.find('?').unwrap_or(sp.path.len()));
    let (page, subpath) = published_page(cfg, section_root, written);
    let title = cfg
        .title_resolver
        .as_deref()
        .map(|r| r.resolve_title(&page));
    if title == Some(None) {
        let section = sp.section;
        return WikilinkResolution::Broken(if written.is_empty() {
            format!("section `{section}` has no root page")
        } else {
            format!("no page `{written}` in section `{section}`")
        });
    }
    let mut href = format!("/{}{query}", encode_url_path(&page));
    if let Some(fragment) = fragment {
        href.push('#');
        href.push_str(fragment);
    }
    let name = title
        .flatten()
        .unwrap_or_else(|| match subpath.rsplit('/').next() {
            Some(name) if !subpath.is_empty() => name.to_owned(),
            _ if !sp.section.name.is_empty() => sp.section.name.clone(),
            _ => href.clone(),
        });
    WikilinkResolution::Resolved {
        href,
        section_ref: sp.section.to_string(),
        subpath: format!("{}{query}", encode_url_path(&subpath)),
        name,
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
/// [`WikilinkResolution::Fragment`] strips the `#` and replaces `-` with
/// spaces; broken targets render `dest_url` as written. Borrowed where the
/// text already exists.
pub(crate) fn display_text<'a>(
    resolution: &'a WikilinkResolution,
    dest_url: &'a str,
) -> Cow<'a, str> {
    match resolution {
        WikilinkResolution::Broken(_) => Cow::Borrowed(dest_url),
        WikilinkResolution::Fragment => {
            let fragment = dest_url.strip_prefix('#').unwrap_or(dest_url);
            Cow::Owned(fragment.replace('-', " "))
        }
        WikilinkResolution::Resolved { name, .. } => Cow::Borrowed(name),
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
            WikilinkResolution::Broken(_)
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
        c.base_path = Some("reserved/sys#1/api".to_owned());
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
            ("::api/", "/reserved/sys%231/api/", "api/", ""),
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
            assert_eq!(display_text(&resolution, target), display, "{target}");
        }
    }
}
