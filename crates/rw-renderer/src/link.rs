//! Path helpers for plain markdown links. Wikilinks live in
//! [`wikilink`](crate::wikilink).

use std::borrow::Cow;
use std::fmt::Display;

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use rw_sections::Sections;

use crate::config::RenderConfig;
use crate::file::{SEPARATORS, file_path, is_external, split_query};
use crate::util::escape_into;

/// What a url path segment must percent-encode, besides non-ASCII. Browsers
/// read a `\` as `/`.
pub(crate) const PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// `path` as url text, the one spelling of every page href.
pub fn encode_url_path(path: &str) -> impl Display + '_ {
    utf8_percent_encode(path, PATH)
}

/// The href for the markdown link `url` on the page at `source_path`; see
/// [`MarkdownRenderer::with_link_resolver`](crate::MarkdownRenderer::with_link_resolver).
///
/// External links, fragment-only links, and non-markdown links are returned
/// unchanged — borrowed, so the common case allocates nothing.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(crate) fn resolve_href<'a>(
    cfg: &RenderConfig,
    source_path: Option<&str>,
    url: &'a str,
) -> Cow<'a, str> {
    let Some(resolver) = cfg.link_resolver.as_deref() else {
        return Cow::Borrowed(url);
    };
    let (path, suffix) = split_query(url);
    if is_external(url) || !path.ends_with(".md") {
        return Cow::Borrowed(url);
    }

    let (root, dir, path) = if let Some(path) = path.strip_prefix(SEPARATORS) {
        ("/", "", path)
    } else if let Some(source_path) = source_path {
        let dir = source_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        ("", dir, path)
    } else {
        return Cow::Borrowed(url);
    };
    let Some(target) = file_path(dir, path).filter(|target| !target.starts_with("../")) else {
        return Cow::Borrowed(url);
    };
    match resolver.page_url(&format!("{root}{target}")) {
        Some(page) => Cow::Owned(format!("/{}{suffix}", encode_url_path(&page))),
        None => Cow::Borrowed(url),
    }
}

/// Resolve a site-absolute href (`/…`) to its `(section_ref, section_path)`
/// pair; `None` for external (protocol-relative `//…` too) or relative hrefs
/// and paths outside every section. Prose links ([`section_ref_attrs`]) and
/// diagram links ([`scan_svg_links`](crate::diagram::scan_svg_links)) must
/// agree on it.
///
/// The section is the one enclosing the page the href names, so `/c%23`
/// matches a section rooted at `c#`. The section path keeps the href's
/// encoding and `?query` and drops its `#fragment`.
pub(crate) fn resolve_section_href(sections: &Sections, href: &str) -> Option<(String, String)> {
    // Browsers read `/\host` as `//host`.
    let href = href
        .strip_prefix('/')
        .filter(|h| !h.starts_with(['/', '\\']))?;
    let href = href.split_once('#').map_or(href, |(head, _)| head);
    let (path, query) = href.split_at(href.find('?').unwrap_or(href.len()));
    let page = percent_decode_str(path).decode_utf8_lossy();
    let sp = sections.find_page(&page)?;
    let tail = if sp.path.is_empty() {
        ""
    } else {
        // The section root's depth, read off `page`: a `%2F` in the tail is a
        // `/` there but not in `path`.
        let skip = page.split('/').count() - sp.path.split('/').count();
        path.splitn(skip + 1, '/').last().unwrap_or(path)
    };
    Some((sp.section.to_string(), format!("{tail}{query}")))
}

/// Build ref data attributes for a resolved path, if applicable.
///
/// [`resolve_section_href`] behind the config: also `None` when no section
/// registry is configured (`with_sections` not called).
pub(crate) fn section_ref_attrs(cfg: &RenderConfig, href: &str) -> Option<(String, String)> {
    resolve_section_href(cfg.sections.as_deref()?, href)
}

/// Append ` data-section-ref="…"` — and ` data-section-path="…"` when the path
/// is non-empty — with attribute-escaped values. Shared by prose and diagram
/// links so they cannot drift apart.
pub(crate) fn write_section_attrs(section_ref: &str, section_path: &str, out: &mut String) {
    out.push_str(r#" data-section-ref=""#);
    escape_into(section_ref, out);
    out.push('"');
    if !section_path.is_empty() {
        out.push_str(r#" data-section-path=""#);
        escape_into(section_path, out);
        out.push('"');
    }
}

/// Knows the pages it lists as `(source path, url path)`; any other file
/// would be published at its path without `.md`.
#[cfg(test)]
pub(crate) struct Pages(pub(crate) &'static [(&'static str, &'static str)]);

#[cfg(test)]
impl crate::LinkResolver for Pages {
    fn page_url(&self, path: &str) -> Option<String> {
        let path = path.trim_start_matches('/');
        let page = self.0.iter().find(|(source, _)| *source == path);
        Some(
            page.map_or(path.trim_end_matches(".md"), |(_, url)| url)
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RenderConfig {
        RenderConfig::default()
    }

    fn resolve(
        source_path: &str,
        pages: &'static [(&'static str, &'static str)],
        url: &str,
    ) -> String {
        let mut c = cfg();
        c.link_resolver = Some(Box::new(Pages(pages)));
        resolve_href(&c, Some(source_path), url).into_owned()
    }

    #[test]
    fn markdown_links_resolve_from_the_source_files_directory() {
        for (source, url, href) in [
            ("a/b/index.md", "./c.md", "/a/b/c"),
            ("a/b/README.md", "c.md", "/a/b/c"),
            ("a/b.md", "./c.md", "/a/c"),
            ("a/b.md", "../c.md#x", "/c#x"),
            ("a/b.md", "/c/d.md", "/c/d"),
            ("index.md", "a/./b/../c.md", "/a/c"),
            ("a/b.md", "c.md?x=1", "/a/c?x=1"),
            ("a/b.md", "c.md?x=1#y", "/a/c?x=1#y"),
        ] {
            assert_eq!(resolve(source, &[], url), href, "{url} from {source}");
        }
    }

    #[test]
    fn markdown_links_lead_to_the_page_a_file_is_published_as() {
        const PAGES: &[(&str, &str)] = &[("a/index.md", "a"), ("README.md", "")];
        assert_eq!(resolve("b.md", PAGES, "a/index.md#x"), "/a#x");
        assert_eq!(resolve("a/c.md", PAGES, "../README.md"), "/");
        assert_eq!(resolve("a/c.md", PAGES, "/README.md"), "/");
        assert_eq!(resolve("b.md", PAGES, "c/index.md"), "/c/index");
    }

    #[test]
    fn a_page_href_is_encoded_however_the_link_spells_it() {
        const PAGES: &[(&str, &str)] = &[
            ("my dir/index.md", "my dir"),
            ("a/README.md", "a"),
            ("гайд/index.md", "гайд"),
        ];
        for (url, href) in [
            ("my%20dir/index.md#x", "/my%20dir#x"),
            ("my dir/index.md#x", "/my%20dir#x"),
            ("%61/README.md", "/a"),
            (
                "%D0%B3%D0%B0%D0%B9%D0%B4/index.md",
                "/%D0%B3%D0%B0%D0%B9%D0%B4",
            ),
            ("гайд/index.md", "/%D0%B3%D0%B0%D0%B9%D0%B4"),
            ("no%20dir/index.md", "/no%20dir/index"),
            ("no dir/index.md", "/no%20dir/index"),
            ("нет.md", "/%D0%BD%D0%B5%D1%82"),
        ] {
            assert_eq!(resolve("b.md", PAGES, url), href, "{url}");
        }
    }

    #[test]
    fn url_delimiters_in_file_names_are_encoded() {
        const PAGES: &[(&str, &str)] = &[("c#/b.md", "c#/b"), ("100%/b.md", "100%/b")];
        for (source, url, href) in [
            ("c#/a.md", "b.md#x", "/c%23/b#x"),
            ("c#/a.md", "missing.md", "/c%23/missing"),
            ("100%/a.md", "b.md", "/100%25/b"),
            ("a.md", "100%25/b.md", "/100%25/b"),
            ("a?/a.md", "b.md", "/a%3F/b"),
            ("my dir/a.md", "b.md", "/my%20dir/b"),
            ("a\\b/a.md", "b.md", "/a%5Cb/b"),
        ] {
            assert_eq!(resolve(source, PAGES, url), href, "{url} from {source}");
        }
    }

    #[test]
    fn markdown_links_read_paths_as_images_do() {
        for (url, href) in [
            ("%2E%2E/c.md", "/c"),
            ("x/.%2e/%2E/c.md", "/a/c"),
            ("x\\..\\c.md", "/a/c"),
            ("x\\c.md", "/a/x/c"),
            ("\\c.md", "/c"),
            ("a%FF.md", "/a/a%25FF"),
        ] {
            assert_eq!(resolve("a/b.md", &[], url), href, "{url}");
        }
    }

    #[test]
    fn markdown_links_above_the_root_stay_as_written() {
        for url in [
            "../../c.md",
            "/../c.md",
            "%2E%2E/%2E%2E/c.md",
            "..\\..\\c.md",
        ] {
            assert_eq!(resolve("a/b.md", &[], url), url);
        }
    }

    #[test]
    fn markdown_links_with_an_encoded_separator_stay_as_written() {
        for url in [
            "x%2Fc.md",
            "x%2fc.md",
            "x%5Cc.md",
            "x%00c.md",
            "%2E%2E%2Fc.md",
        ] {
            assert_eq!(resolve("a/b.md", &[], url), url);
        }
    }

    #[test]
    fn markdown_links_the_resolver_places_nowhere_stay_as_written() {
        struct Nowhere;
        impl crate::LinkResolver for Nowhere {
            fn page_url(&self, _: &str) -> Option<String> {
                None
            }
        }
        let mut c = cfg();
        c.link_resolver = Some(Box::new(Nowhere));
        assert_eq!(resolve_href(&c, Some("a/b.md"), "c.md#x"), "c.md#x");
    }

    #[test]
    fn other_links_stay_as_written() {
        for url in [
            "https://example.com/a.md",
            "HTTPS://example.com/a.md",
            "ftp://example.com/a.md",
            "vscode://file/a.md",
            "file:///etc/a.md",
            "//example.com/a.md",
            "/\\example.com/a.md",
            "mailto:test@example.com",
            "#a.md",
            "./image.png",
            "a#b.md",
            "a?b.md",
        ] {
            assert!(matches!(resolve_href(&cfg(), None, url), Cow::Borrowed(_)));
            assert_eq!(resolve("a/b.md", &[], url), url, "{url}");
        }
    }

    #[test]
    fn markdown_links_need_a_resolver_and_relative_ones_a_source_path() {
        assert_eq!(resolve_href(&cfg(), Some("a/b.md"), "c.md"), "c.md");
        let mut c = cfg();
        c.link_resolver = Some(Box::new(Pages(&[])));
        assert_eq!(resolve_href(&c, None, "c.md"), "c.md");
        assert_eq!(resolve_href(&c, None, "/c.md"), "/c");
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

    #[test]
    fn a_protocol_relative_href_belongs_to_no_section() {
        use std::collections::HashMap;

        use rw_sections::Namespace;

        let sections = Sections::with_implicit_root(HashMap::new(), Namespace::default());
        assert_eq!(resolve_section_href(&sections, "//example.com/a"), None);
        assert_eq!(resolve_section_href(&sections, "/\\example.com/a"), None);
        assert!(resolve_section_href(&sections, "/example.com/a").is_some());
    }

    #[test]
    fn section_href_matches_the_page_its_encoded_path_names() {
        use rw_sections::{Namespace, Section, Sections};

        let system = |name: &str| Section {
            kind: "system".to_owned(),
            namespace: Namespace::default(),
            name: name.to_owned(),
        };
        let sections = Sections::with_implicit_root(
            std::collections::HashMap::from([
                ("billing".to_owned(), system("billing")),
                ("billing/sys#1".to_owned(), system("sys#1")),
                ("reserved/100%".to_owned(), system("100%")),
                ("гайд".to_owned(), system("гайд")),
            ]),
            Namespace::default(),
        );
        for (href, page, section_path) in [
            ("/reserved/100%25", "reserved/100%", ""),
            ("/reserved/100%25/a%20b", "reserved/100%/a b", "a%20b"),
            ("/billing/sys%231/api?x#top", "billing/sys#1/api", "api?x"),
            ("/billing/sys%231?x", "billing/sys#1", "?x"),
            ("/billing?next=/a%23/b", "billing", "?next=/a%23/b"),
            ("/billing#top", "billing", ""),
            ("/billing/c%23/intro", "billing/c#/intro", "c%23/intro"),
            ("/%D0%B3%D0%B0%D0%B9%D0%B4/a", "гайд/a", "a"),
            ("/billing/a%2Fb", "billing/a/b", "a%2Fb"),
            ("/billing/x/a%2Fb/y", "billing/x/a/b/y", "x/a%2Fb/y"),
            (
                "/reserved/c%23/intro",
                "reserved/c#/intro",
                "reserved/c%23/intro",
            ),
        ] {
            let page_ref = sections.find_page(page).unwrap().section.to_string();
            assert_eq!(
                resolve_section_href(&sections, href),
                Some((page_ref, section_path.to_owned())),
                "{href}"
            );
        }
    }
}
