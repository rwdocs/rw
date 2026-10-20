//! Path helpers for plain markdown links.
//!
//! Used by [`Walker`](crate::walker) when a parser emits a non-wikilink
//! [`Tag::Link`](rw_parser::Tag::Link) or
//! [`Tag::Image`](rw_parser::Tag::Image). Wikilink-specific resolution
//! lives in the sibling [`wikilink`](crate::wikilink) module.

use std::borrow::Cow;

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use rw_sections::Sections;

use crate::config::RenderConfig;
use crate::util::escape_into;

/// `url` without the origin prefix, when it has one.
///
/// For files outside `source_dir` (e.g., README.md at the project root),
/// relative links like `docs/guide.md` include the source directory name.
pub(crate) fn strip_origin<'a>(cfg: &RenderConfig, url: &'a str) -> Option<&'a str> {
    url.strip_prefix(cfg.origin_prefix.as_deref()?)
}

/// What a url path segment must percent-encode, besides non-ASCII.
const PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// `path` with the characters that would end or escape a url path encoded.
/// Everything else stays as the file names have it.
fn escape_url_path(path: &str) -> Cow<'_, str> {
    if !path.contains(['%', '#', '?']) {
        return Cow::Borrowed(path);
    }
    let mut out = String::with_capacity(path.len() + 4);
    for c in path.chars() {
        match c {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// The href for the markdown link `url`; see
/// [`MarkdownRenderer::with_link_resolver`](crate::MarkdownRenderer::with_link_resolver).
///
/// External links, fragment-only links, and non-markdown links are returned
/// unchanged — borrowed, so the common case allocates nothing.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(crate) fn resolve_href<'a>(cfg: &RenderConfig, url: &'a str) -> Cow<'a, str> {
    let (Some(resolver), Some(source_path)) =
        (cfg.link_resolver.as_deref(), cfg.source_path.as_deref())
    else {
        return Cow::Borrowed(url);
    };
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("//")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
        || url.starts_with('#')
        || !(url.ends_with(".md") || url.contains(".md#"))
    {
        return Cow::Borrowed(url);
    }

    let (path, fragment) = url.split_at(url.find('#').unwrap_or(url.len()));
    let (dir, path) = if let Some(path) = path.strip_prefix('/') {
        ("", path)
    } else if let Some(path) = strip_origin(cfg, path) {
        ("", path)
    } else {
        (
            source_path.rsplit_once('/').map_or("", |(dir, _)| dir),
            path,
        )
    };
    // The link is url text, so the page's own directory has to be too.
    let dir = escape_url_path(dir);

    let mut ups = 0;
    let mut segments: Vec<&str> = Vec::new();
    for segment in dir.split('/').chain(path.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    ups += 1;
                }
            }
            _ => segments.push(segment),
        }
    }
    let target = segments.join("/");
    // The parser hands over the destination as written, and files have
    // decoded names; an encoded link gets an encoded href back.
    let decoded = percent_decode_str(&target).decode_utf8_lossy();
    let encoded = *decoded != *target;

    let target_path = if ups == 0 {
        decoded
    } else {
        Cow::Owned(format!("{}{decoded}", "../".repeat(ups)))
    };
    match resolver.page_url(&target_path) {
        Some(page) if encoded => {
            Cow::Owned(format!("/{}{fragment}", utf8_percent_encode(&page, PATH)))
        }
        Some(page) => Cow::Owned(format!("/{}{fragment}", escape_url_path(&page))),
        None if ups > 0 => Cow::Borrowed(url),
        None => {
            let page = target.strip_suffix(".md").unwrap_or(&target);
            Cow::Owned(format!("/{page}{fragment}"))
        }
    }
}

/// Resolve a site-absolute href (`/…`) to its `(section_ref, section_path)`
/// pair; `None` for external or relative hrefs and paths outside every
/// section. The one definition of "this href belongs to a site section" —
/// prose links ([`section_ref_attrs`]) and diagram links
/// ([`scan_svg_links`](crate::diagram::scan_svg_links)) must agree on it.
///
/// The section is the one enclosing the page the href names, so `/c%23`
/// matches a section rooted at `c#`. The section path keeps the href's
/// encoding and `?query` and drops its `#fragment`.
pub(crate) fn resolve_section_href(sections: &Sections, href: &str) -> Option<(String, String)> {
    let href = href.strip_prefix('/')?;
    let href = href.split_once('#').map_or(href, |(head, _)| head);
    let (path, query) = href.split_at(href.find('?').unwrap_or(href.len()));
    let page = percent_decode_str(path).decode_utf8_lossy();
    let sp = sections.find_page(&page)?;
    let tail = if sp.path.is_empty() {
        ""
    } else {
        // The same trailing segments, as the href spells them. Saturating: an
        // encoded `%2F` decodes into an extra segment.
        let skip = path
            .split('/')
            .count()
            .saturating_sub(sp.path.split('/').count());
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
/// is non-empty — with attribute-escaped values. The one writer of these
/// attributes, shared by the prose-link default
/// ([`RenderBackend::link_start`](crate::RenderBackend::link_start)) and the
/// diagram SVG splice
/// ([`splice_link_attrs`](crate::diagram::splice_link_attrs)), so prose
/// anchors and diagram anchors cannot drift apart.
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

/// Knows the pages it lists as `(source path, url path)`.
#[cfg(test)]
pub(crate) struct Pages(pub(crate) &'static [(&'static str, &'static str)]);

#[cfg(test)]
impl crate::LinkResolver for Pages {
    fn page_url(&self, source_path: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(source, _)| *source == source_path)
            .map(|(_, url)| (*url).to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
    }

    fn resolve(
        source_path: &str,
        pages: &'static [(&'static str, &'static str)],
        url: &str,
    ) -> String {
        let mut c = cfg();
        c.source_path = Some(source_path.to_owned());
        c.link_resolver = Some(Box::new(Pages(pages)));
        resolve_href(&c, url).into_owned()
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
        ] {
            assert_eq!(resolve(source, &[], url), href, "{url} from {source}");
        }
    }

    #[test]
    fn markdown_links_lead_to_the_page_a_file_is_published_as() {
        const PAGES: &[(&str, &str)] =
            &[("a/index.md", "a"), ("README.md", ""), ("../README.md", "")];
        assert_eq!(resolve("b.md", PAGES, "a/index.md#x"), "/a#x");
        assert_eq!(resolve("a/c.md", PAGES, "../README.md"), "/");
        assert_eq!(resolve("b.md", PAGES, "../README.md"), "/");
        assert_eq!(resolve("b.md", PAGES, "c/index.md"), "/c/index");
    }

    #[test]
    fn percent_encoded_links_find_the_decoded_file() {
        const PAGES: &[(&str, &str)] = &[
            ("my dir/index.md", "my dir"),
            ("a/README.md", "a"),
            ("гайд/index.md", "гайд"),
        ];
        for (url, href) in [
            ("my%20dir/index.md#x", "/my%20dir#x"),
            ("%61/README.md", "/a"),
            (
                "%D0%B3%D0%B0%D0%B9%D0%B4/index.md",
                "/%D0%B3%D0%B0%D0%B9%D0%B4",
            ),
            ("гайд/index.md", "/гайд"),
            ("no%20dir/index.md", "/no%20dir/index"),
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
            ("my dir/a.md", "b.md", "/my dir/b"),
        ] {
            assert_eq!(resolve(source, PAGES, url), href, "{url} from {source}");
        }
    }

    #[test]
    fn markdown_links_out_of_the_source_dir_stay_as_written() {
        assert_eq!(resolve("a/b.md", &[], "../../c.md"), "../../c.md");
        assert_eq!(resolve("../README.md", &[], "CHANGELOG.md"), "CHANGELOG.md");
    }

    #[test]
    fn origin_links_resolve_from_the_source_dir() {
        let mut c = cfg();
        c.source_path = Some("../README.md".to_owned());
        c.origin_prefix = Some("docs/".to_owned());
        c.link_resolver = Some(Box::new(Pages(&[("../README.md", "")])));
        assert_eq!(resolve_href(&c, "docs/a/b.md#x"), "/a/b#x");
        assert_eq!(resolve_href(&c, "README.md"), "/");
    }

    #[test]
    fn other_links_stay_as_written() {
        for url in [
            "https://example.com/a.md",
            "mailto:test@example.com",
            "#a.md",
            "./image.png",
            "a.md?x",
        ] {
            assert!(matches!(resolve_href(&cfg(), url), Cow::Borrowed(_)));
            assert_eq!(resolve("a/b.md", &[], url), url);
        }
    }

    #[test]
    fn markdown_links_need_a_resolver_and_a_source_path() {
        let mut c = cfg();
        c.source_path = Some("a/b.md".to_owned());
        assert_eq!(resolve_href(&c, "c.md"), "c.md");
        let mut c = cfg();
        c.link_resolver = Some(Box::new(Pages(&[])));
        assert_eq!(resolve_href(&c, "c.md"), "c.md");
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
            ("/billing/c%23/intro", "billing/c#/intro", "c%23/intro"),
            ("/%D0%B3%D0%B0%D0%B9%D0%B4/a", "гайд/a", "a"),
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
