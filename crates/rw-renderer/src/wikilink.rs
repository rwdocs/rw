//! Wikilink resolution and display-text helpers.
//!
//! Used by [`Walker`](crate::walker) when a parser emits a
//! [`Tag::Link`](pulldown_cmark::Tag::Link) with
//! [`LinkType::WikiLink`](pulldown_cmark::LinkType::WikiLink). Plain markdown
//! link helpers live in the sibling [`link`](crate::link) module.

use crate::config::RenderConfig;

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

/// Resolve a wikilink target to a [`WikilinkResolution`].
///
/// Fragment-only targets (`#heading`) always resolve to
/// [`WikilinkResolution::Fragment`]. Any other target requires a sections
/// registry on `cfg`; without one the result is
/// [`WikilinkResolution::Broken`]. Current-section links (`[[::path]]`)
/// additionally need `cfg.base_path`.
pub(crate) fn resolve(cfg: &RenderConfig, dest_url: &str) -> WikilinkResolution {
    if let Some(fragment) = dest_url.strip_prefix('#') {
        return WikilinkResolution::Fragment(fragment.to_owned());
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
        None => WikilinkResolution::Broken {
            raw_target: dest_url.to_owned(),
        },
    }
}

/// Return the display text to render for a wikilink, given its resolution.
///
/// For [`WikilinkResolution::Resolved`] the priority is: title resolver (when
/// configured) → last segment of `subpath` → `section_name` → raw `href`.
/// [`WikilinkResolution::Fragment`] replaces `-` with spaces; broken targets
/// render their raw text.
pub(crate) fn display_text(cfg: &RenderConfig, resolution: &WikilinkResolution) -> String {
    match resolution {
        WikilinkResolution::Broken { raw_target } => raw_target.clone(),
        WikilinkResolution::Fragment(fragment) => fragment.replace('-', " "),
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
        match resolve(&c, "#some-fragment") {
            WikilinkResolution::Fragment(s) => assert_eq!(s, "some-fragment"),
            other => panic!("expected Fragment, got {other:?}"),
        }
    }

    #[test]
    fn resolve_no_sections_returns_broken() {
        let c = cfg();
        match resolve(&c, "domain:billing::overview") {
            WikilinkResolution::Broken { raw_target } => {
                assert_eq!(raw_target, "domain:billing::overview");
            }
            other => panic!("expected Broken, got {other:?}"),
        }
    }

    #[test]
    fn display_text_fragment_replaces_dashes_with_spaces() {
        let c = cfg();
        let res = WikilinkResolution::Fragment("hello-world-now".to_owned());
        assert_eq!(display_text(&c, &res), "hello world now");
    }

    #[test]
    fn display_text_broken_returns_raw_target() {
        let c = cfg();
        let res = WikilinkResolution::Broken {
            raw_target: "broken/target".to_owned(),
        };
        assert_eq!(display_text(&c, &res), "broken/target");
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
        assert_eq!(display_text(&c, &res), "bar");
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
        assert_eq!(display_text(&c, &res), "billing");
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
        assert_eq!(display_text(&c, &res), "Billing Overview");
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
        assert_eq!(display_text(&c, &res), "bar");
    }
}
