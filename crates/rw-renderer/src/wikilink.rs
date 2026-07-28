//! Wikilink resolution and display-text helpers.
//!
//! Used by [`Walker`](crate::walker) when a parser emits a
//! [`Tag::Link`](pulldown_cmark::Tag::Link) with
//! [`LinkType::WikiLink`](pulldown_cmark::LinkType::WikiLink). Plain markdown
//! link helpers live in the sibling [`link`](crate::link) module.

use std::borrow::Cow;

use crate::config::RenderConfig;

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

    let resolved = cfg
        .sections
        .as_ref()
        .and_then(|s| s.resolve_refpath(dest_url, cfg.base_path.as_deref()));

    match resolved {
        Some((href, sp)) => WikilinkResolution::Resolved {
            href,
            section_ref: sp.section.to_string(),
            section_name: sp.section.name.clone(),
            subpath: sp.path.to_owned(),
        },
        None => WikilinkResolution::Broken,
    }
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
            if let Some(resolver) = &cfg.title_resolver {
                let path = href.strip_prefix('/').unwrap_or(href);
                let path = match path.find('#') {
                    Some(pos) => &path[..pos],
                    None => path,
                };
                if let Some(title) = resolver.resolve_title(path) {
                    return Cow::Owned(title);
                }
            }

            if !subpath.is_empty() {
                // unwrap: rsplit always yields at least one element
                return Cow::Borrowed(subpath.rsplit('/').next().unwrap());
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
    use super::*;
    use crate::config::TitleResolver;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
    }

    #[test]
    fn resolve_fragment_returns_fragment_variant() {
        let c = cfg();
        assert!(matches!(
            resolve(&c, "#some-fragment"),
            WikilinkResolution::Fragment
        ));
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
    fn display_text_fragment_strips_hash_and_replaces_dashes_with_spaces() {
        let c = cfg();
        let res = WikilinkResolution::Fragment;
        assert_eq!(
            display_text(&c, &res, "#hello-world-now"),
            "hello world now"
        );
    }

    #[test]
    fn display_text_broken_returns_raw_target() {
        let c = cfg();
        let res = WikilinkResolution::Broken;
        assert_eq!(display_text(&c, &res, "broken/target"), "broken/target");
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

    #[test]
    fn display_text_resolved_uses_resolver_when_present() {
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
        assert_eq!(
            display_text(&c, &res, "unused-for-resolved"),
            "Billing Overview"
        );
    }

    #[test]
    fn display_text_resolved_falls_through_when_resolver_returns_none() {
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
        assert_eq!(display_text(&c, &res, "unused-for-resolved"), "bar");
    }
}
