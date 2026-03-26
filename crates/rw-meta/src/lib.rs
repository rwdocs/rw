mod fields;
mod head;

use std::collections::HashMap;

use fields::MetaFields;
use head::Head;

/// Resolved page metadata from all sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    /// Page kind (e.g., "domain", "guide").
    pub kind: Option<String>,
    /// Page title (always resolved — falls back to titlecase of filename).
    pub title: String,
    /// Page description.
    pub description: Option<String>,
    /// Custom variables (key-level merge from meta.yaml and frontmatter).
    pub vars: HashMap<String, serde_json::Value>,
}

impl Meta {
    /// Extract and merge metadata from markdown content and meta.yaml.
    ///
    /// Internally:
    /// 1. Parses meta.yaml into base fields
    /// 2. Extracts frontmatter and first H1 from markdown via pulldown-cmark
    /// 3. Merges frontmatter over meta.yaml (frontmatter wins per field, vars key-level merge)
    /// 4. Resolves title: frontmatter.title > meta.title > H1 > titlecase(filename)
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
            .unwrap_or_else(|| {
                let name = filename.strip_suffix(".md").unwrap_or(filename);
                titlecase_from_slug(name)
            });

        Self {
            kind: merged.kind,
            title,
            description: merged.description,
            vars: merged.vars,
        }
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

    #[test]
    fn resolve_vars_merged() {
        let md = "---\nvars:\n  a: frontmatter-a\n  c: frontmatter-c\n---\n";
        let meta_yaml = "vars:\n  a: meta-a\n  b: meta-b";
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(
            meta.vars.get("a").and_then(|v| v.as_str()),
            Some("frontmatter-a"),
            "frontmatter key wins"
        );
        assert_eq!(
            meta.vars.get("b").and_then(|v| v.as_str()),
            Some("meta-b"),
            "meta-only key preserved"
        );
        assert_eq!(
            meta.vars.get("c").and_then(|v| v.as_str()),
            Some("frontmatter-c"),
            "frontmatter-only key present"
        );
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
    fn resolve_no_sources() {
        let meta = Meta::resolve(None, None, "some-page.md");
        assert_eq!(meta.title, "Some Page");
        assert!(meta.description.is_none());
        assert!(meta.kind.is_none());
        assert!(meta.vars.is_empty());
    }
}
