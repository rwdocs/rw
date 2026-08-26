mod fields;
mod head;

use fields::MetaFields;
use head::Head;

/// Resolved page metadata from all sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// Page kind (e.g., "domain", "guide").
    pub kind: Option<String>,
    /// Section namespace declared by this page's metadata.
    pub namespace: Option<String>,
    /// Page title (always resolved and never empty): frontmatter `title`, else
    /// `meta.yaml` title, else the first H1, else the titlecased filename stem,
    /// else the stem verbatim, else `"Untitled"`.
    pub title: String,
    /// Page description.
    pub description: Option<String>,
    /// Ordered list of child page slugs for navigation ordering.
    pub pages: Option<Vec<String>>,
}

impl Meta {
    /// Extract and merge metadata from markdown content and meta.yaml.
    ///
    /// Internally:
    /// 1. Parses meta.yaml into base fields
    /// 2. Extracts frontmatter and first H1 from markdown via pulldown-cmark
    /// 3. Merges frontmatter over meta.yaml (frontmatter wins per field)
    /// 4. Resolves title: frontmatter title, else `meta.yaml` title, else H1,
    ///    else titlecased filename stem, else stem verbatim, else `"Untitled"`
    #[must_use]
    pub fn resolve(markdown: Option<&str>, meta_yaml: Option<&str>, filename: &str) -> Self {
        let base = meta_yaml.map(MetaFields::from_yaml).unwrap_or_default();

        let (frontmatter, h1_title) = markdown
            .map(Head::parse)
            .map_or((None, None), |h| (h.frontmatter, h.title));

        let overlay = frontmatter
            .as_deref()
            .map(MetaFields::from_yaml)
            .unwrap_or_default();
        let merged = base.merge(overlay);

        let title = merged
            .title
            .or(h1_title)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| resolve_filename_title(filename));

        Self {
            kind: merged.kind,
            namespace: merged.namespace,
            title,
            description: merged.description,
            pages: merged.pages,
        }
    }
}

/// Fallback title when no frontmatter, `meta.yaml`, or H1 supplies one.
///
/// Titlecases the filename stem. `titlecase_from_slug` is empty-in/empty-out,
/// so a stem made only of `-`/`_` (or an empty stem) would otherwise resolve
/// to an empty title; this falls back to the stem verbatim, and then to
/// `"Untitled"` if the stem itself is empty, so the result is never empty.
fn resolve_filename_title(filename: &str) -> String {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    let titlecased = titlecase_from_slug(stem);
    if !titlecased.is_empty() {
        titlecased
    } else if !stem.is_empty() {
        stem.to_owned()
    } else {
        "Untitled".to_owned()
    }
}

/// Convert a slug to title case.
///
/// Replaces `-` and `_` with spaces, capitalizes each word.
///
/// `"setup-guide"` → `"Setup Guide"`, `"my_page"` → `"My Page"`
fn titlecase_from_slug(slug: &str) -> String {
    slug.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- titlecase_from_slug ---

    #[test]
    fn titlecase_kebab() {
        assert_eq!(titlecase_from_slug("setup-guide"), "Setup Guide");
    }

    #[test]
    fn titlecase_snake() {
        assert_eq!(titlecase_from_slug("my_page"), "My Page");
    }

    #[test]
    fn titlecase_single_word() {
        assert_eq!(titlecase_from_slug("hello"), "Hello");
    }

    #[test]
    fn titlecase_empty() {
        assert_eq!(titlecase_from_slug(""), "");
    }

    // --- resolve: title priority ---

    #[test]
    fn resolve_frontmatter_title_wins_over_meta_yaml() {
        let md = "---\ntitle: Frontmatter Title\n---\n\n# H1 Title\n";
        let meta_yaml = "title: Meta YAML Title";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(meta.title, "Frontmatter Title");
    }

    #[test]
    fn resolve_meta_yaml_title_wins_over_h1() {
        let md = "# H1 Title\n\nSome content.";
        let meta_yaml = "title: Meta YAML Title";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(meta.title, "Meta YAML Title");
    }

    #[test]
    fn resolve_h1_wins_over_filename() {
        let md = "# H1 Title\n\nSome content.";
        let meta = Meta::resolve(Some(md), None, "page.md");
        assert_eq!(meta.title, "H1 Title");
    }

    #[test]
    fn resolve_filename_fallback() {
        let meta = Meta::resolve(None, None, "setup-guide.md");
        assert_eq!(meta.title, "Setup Guide");
    }

    #[test]
    fn resolve_filename_strips_md_extension() {
        let meta = Meta::resolve(None, None, "my-page.md");
        assert_eq!(meta.title, "My Page");
    }

    #[test]
    fn resolve_no_markdown_with_meta_yaml() {
        let meta_yaml = "title: From Meta\ndescription: A description";
        let meta = Meta::resolve(None, Some(meta_yaml), "page.md");
        assert_eq!(meta.title, "From Meta");
        assert_eq!(meta.description.as_deref(), Some("A description"));
    }

    // --- resolve: field merging ---

    #[test]
    fn resolve_frontmatter_description_wins() {
        let md = "---\ndescription: Frontmatter desc\n---\n\n# Title\n";
        let meta_yaml = "description: Meta YAML desc";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(meta.description.as_deref(), Some("Frontmatter desc"));
    }

    #[test]
    fn resolve_meta_yaml_description_when_no_frontmatter() {
        let md = "# Title\n\nSome content.";
        let meta_yaml = "description: Meta YAML desc";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(meta.description.as_deref(), Some("Meta YAML desc"));
    }

    // --- resolve: error handling ---

    #[test]
    fn resolve_malformed_frontmatter_ignored() {
        let md = "---\n: : invalid: [unclosed\n---\n\n# H1 Title\n";
        let meta = Meta::resolve(Some(md), None, "page.md");
        // Malformed frontmatter is ignored; H1 is still extracted
        assert_eq!(meta.title, "H1 Title");
    }

    #[test]
    fn resolve_malformed_meta_yaml_ignored() {
        let meta_yaml = ": : invalid: [unclosed";
        let md = "# H1 Title\n";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        // Malformed meta.yaml is ignored; H1 is used
        assert_eq!(meta.title, "H1 Title");
    }

    // --- resolve: edge cases ---

    #[test]
    fn resolve_code_block_comment_not_h1() {
        let md = "```\n# comment\n```\n";
        let meta = Meta::resolve(Some(md), None, "my-page.md");
        // Code block # is not an H1; falls back to filename
        assert_eq!(meta.title, "My Page");
    }

    #[test]
    fn resolve_formatted_h1() {
        let md = "# Hello **world** with `code`\n";
        let meta = Meta::resolve(Some(md), None, "page.md");
        assert_eq!(meta.title, "Hello world with code");
    }

    #[test]
    fn resolve_empty_h1_falls_back_to_filename() {
        let md = "# \n\nSome content.";
        let meta = Meta::resolve(Some(md), None, "setup-guide.md");
        assert_eq!(meta.title, "Setup Guide");
    }

    #[test]
    fn resolve_filename_of_only_underscore_falls_back_to_stem_verbatim() {
        // titlecase_from_slug("_") replaces "_" with " " then splits on
        // whitespace, yielding nothing — titlecasing this stem is empty.
        let meta = Meta::resolve(None, None, "_.md");
        assert_eq!(meta.title, "_");
    }

    #[test]
    fn resolve_filename_dot_md_only_falls_back_to_untitled() {
        // Stripping ".md" from ".md" itself leaves an empty stem, so even the
        // stem-verbatim fallback is empty; "Untitled" is the last resort.
        let meta = Meta::resolve(None, None, ".md");
        assert_eq!(meta.title, "Untitled");
    }

    #[test]
    fn resolve_no_sources() {
        let meta = Meta::resolve(None, None, "some-page.md");
        assert_eq!(meta.title, "Some Page");
        assert!(meta.description.is_none());
        assert!(meta.kind.is_none());
    }

    #[test]
    fn resolve_pages_from_meta_yaml() {
        let meta = Meta::resolve(
            None,
            Some("pages:\n  - getting-started\n  - configuration"),
            "index.md",
        );
        assert_eq!(
            meta.pages,
            Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned()
            ])
        );
    }

    #[test]
    fn resolve_pages_frontmatter_overrides_meta_yaml() {
        let markdown = "---\npages:\n  - alpha\n---\n# Title\n";
        let meta_yaml = "pages:\n  - beta\n  - gamma";
        let meta = Meta::resolve(Some(markdown), Some(meta_yaml), "index.md");
        assert_eq!(meta.pages, Some(vec!["alpha".to_owned()]));
    }

    #[test]
    fn resolve_no_pages() {
        let meta = Meta::resolve(Some("# Hello"), None, "page.md");
        assert!(meta.pages.is_none());
    }

    #[test]
    fn resolve_namespace_from_meta_yaml() {
        let meta = Meta::resolve(None, Some("namespace: payments"), "page.md");
        assert_eq!(meta.namespace.as_deref(), Some("payments"));
    }

    #[test]
    fn resolve_namespace_frontmatter_overrides_meta_yaml() {
        let md = "---\nnamespace: front-ns\n---\n# Title\n";
        let meta = Meta::resolve(Some(md), Some("namespace: yaml-ns"), "page.md");
        assert_eq!(meta.namespace.as_deref(), Some("front-ns"));
    }
}
