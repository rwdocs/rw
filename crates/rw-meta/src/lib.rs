use std::collections::BTreeMap;

mod attrs;
mod diagnostic;
mod fields;

pub use diagnostic::{Diagnostic, DiagnosticSource, Severity};
use fields::MetaFields;
use rw_parser::PageHead;

/// Resolved page metadata from all sources.
///
/// Prefer [`Meta::resolve`] when constructing metadata from document sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// Page kind (e.g., "domain", "guide").
    pub kind: Option<String>,
    /// Section namespace declared by this page's metadata.
    pub namespace: Option<String>,
    /// Page title (always resolved and never empty): frontmatter `title`, else
    /// `meta.yaml` title, else the opening H1, else the titlecased filename stem,
    /// else the stem verbatim, else `"Untitled"`.
    pub title: String,
    /// Page description.
    pub description: Option<String>,
    /// Ordered list of child page slugs for navigation ordering.
    pub pages: Option<Vec<String>>,
    /// Declared page-local name; never inherited. Overrides section identity only
    /// when this page declares `kind`; otherwise retained but not effective.
    pub name: Option<String>,
    /// Page-local JSON data; never inherited.
    ///
    /// Source resolution bounds array/object nesting for manifest/cache transport.
    /// Callers constructing attrs directly must also respect the wire readers'
    /// nesting limits.
    pub attrs: BTreeMap<String, serde_json::Value>,
}

/// Resolution result: canonical fields plus every recoverable problem found
/// on the way. `diagnostics` lists sidecar problems first, then frontmatter,
/// each group in the fixed field order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMeta {
    /// Canonical fields after merging and fallbacks.
    pub meta: Meta,
    /// Problems in sidecar-then-frontmatter order, fixed field order within each.
    pub diagnostics: Vec<Diagnostic>,
}

impl Meta {
    /// Extract and merge metadata from markdown content and meta.yaml.
    ///
    /// Internally:
    /// 1. Parses meta.yaml into base fields
    /// 2. Extracts frontmatter and the opening H1's text from markdown
    /// 3. Overlays valid frontmatter fields onto meta.yaml; attrs merge by
    ///    top-level key, while other supplied fields replace their sidecar values
    /// 4. Resolves title: frontmatter title, else `meta.yaml` title, else H1,
    ///    else titlecased filename stem, else stem verbatim, else `"Untitled"`
    #[must_use]
    pub fn resolve(markdown: Option<&str>, meta_yaml: Option<&str>, filename: &str) -> Self {
        Self::resolve_with_diagnostics(markdown, meta_yaml, filename).meta
    }

    /// Like [`Meta::resolve`], also reporting every dropped field or source.
    #[must_use]
    pub fn resolve_with_diagnostics(
        markdown: Option<&str>,
        meta_yaml: Option<&str>,
        filename: &str,
    ) -> ResolvedMeta {
        let mut diagnostics = Vec::new();

        let base = meta_yaml.map_or(MetaFields::default(), |yaml| {
            let (fields, found) =
                MetaFields::from_yaml_with_diagnostics(yaml, DiagnosticSource::Sidecar);
            diagnostics.extend(found);
            fields
        });

        let PageHead {
            frontmatter,
            title: h1_title,
        } = markdown.map(PageHead::from).unwrap_or_default();

        let overlay = frontmatter
            .as_deref()
            .map_or(MetaFields::default(), |yaml| {
                let (fields, found) =
                    MetaFields::from_yaml_with_diagnostics(yaml, DiagnosticSource::Frontmatter);
                diagnostics.extend(found);
                fields
            });

        let merged = base.merge(overlay);

        let title = merged
            .title
            .or(h1_title)
            .unwrap_or_else(|| resolve_filename_title(filename));

        ResolvedMeta {
            meta: Self {
                kind: merged.kind,
                namespace: merged.namespace,
                title,
                description: merged.description,
                pages: merged.pages,
                name: merged.name,
                attrs: merged.attrs,
            },
            diagnostics,
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

    #[test]
    fn attrs_shallow_overlay_preserves_null_and_sidecar_keys() {
        let resolved = Meta::resolve_with_diagnostics(
            Some("---\nattrs:\n  owner: null\n  nested: {new: true}\n  tags: [new]\n---\n# Page"),
            Some("attrs:\n  owner: old\n  kept: 42\n  nested: {old: true}\n  tags: [old]"),
            "page.md",
        );
        assert_eq!(resolved.meta.attrs["owner"], serde_json::Value::Null);
        assert_eq!(resolved.meta.attrs["kept"], serde_json::json!(42));
        assert_eq!(
            resolved.meta.attrs["nested"],
            serde_json::json!({"new": true})
        );
        assert_eq!(resolved.meta.attrs["tags"], serde_json::json!(["new"]));
        assert_eq!(resolved.diagnostics, [] as [diagnostic::Diagnostic; 0]);
    }

    #[test]
    fn attrs_preserve_json_types_and_deterministic_keys() {
        let resolved = Meta::resolve_with_diagnostics(
            None,
            Some(
                "attrs:\n  z: [null, true, false, text, -9223372036854775808, 18446744073709551615, 1.5, {}, []]\n  a: {z: 1, a: {z: 2, a: 3}}\n  text: '42'",
            ),
            "page.md",
        );
        assert_eq!(resolved.diagnostics, [] as [diagnostic::Diagnostic; 0]);
        assert_eq!(
            resolved
                .meta
                .attrs
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["a", "text", "z"]
        );
        assert_eq!(resolved.meta.attrs["text"], serde_json::json!("42"));
        assert_eq!(
            resolved.meta.attrs["z"],
            serde_json::json!([null, true, false, "text", i64::MIN, u64::MAX, 1.5, {}, []])
        );
        assert_eq!(
            serde_json::to_string(&resolved.meta.attrs["a"]).unwrap(),
            r#"{"a":{"a":3,"z":2},"z":1}"#
        );
    }

    #[test]
    fn attrs_absent_and_empty_add_no_overrides() {
        for yaml in ["title: Kept", "attrs: {}"] {
            let markdown = format!("---\n{yaml}\n---");
            let empty = Meta::resolve_with_diagnostics(Some(&markdown), None, "p");
            assert!(empty.meta.attrs.is_empty());
            assert_eq!(empty.diagnostics, [] as [diagnostic::Diagnostic; 0]);
            let base =
                Meta::resolve_with_diagnostics(Some(&markdown), Some("attrs: {kept: true}"), "p");
            assert_eq!(base.meta.attrs["kept"], serde_json::json!(true));
            assert_eq!(base.diagnostics, [] as [diagnostic::Diagnostic; 0]);
            let sidecar = Meta::resolve_with_diagnostics(None, Some(yaml), "p");
            assert!(sidecar.meta.attrs.is_empty());
            assert_eq!(sidecar.diagnostics, [] as [diagnostic::Diagnostic; 0]);
        }
        assert!(Meta::resolve(None, None, "p").attrs.is_empty());
    }

    #[test]
    fn attrs_transport_depth_preserves_field_and_source_recovery() {
        // Both container kinds count, even when the deepest container is empty.
        for shape in ["array", "object", "mixed"] {
            for (leaf, leaf_value) in [
                ("null", serde_json::Value::Null),
                ("[]", serde_json::json!([])),
                ("{}", serde_json::json!({})),
            ] {
                for depth in [123, 124, 125, 126, 127] {
                    let mut expected = leaf_value.clone();
                    let leaf_depth = usize::from(leaf != "null");
                    for level in leaf_depth..depth {
                        expected = if shape == "array" || (shape == "mixed" && level % 2 == 0) {
                            serde_json::Value::Array(vec![expected])
                        } else {
                            serde_json::json!({"child": expected})
                        };
                    }
                    let value = serde_json::to_string(&expected).unwrap();
                    for source in [DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter] {
                        let fields =
                            format!("title: Kept\nattrs: {{kept: changed, value: {value}}}");
                        let markdown = format!("---\n{fields}\n---");
                        let fallback = "attrs: {kept: fallback}\ntitle: Fallback";
                        let fallback_markdown = format!("---\n{fallback}\n---");
                        let result = match source {
                            DiagnosticSource::Sidecar => Meta::resolve_with_diagnostics(
                                Some(&fallback_markdown),
                                Some(&fields),
                                "p",
                            ),
                            DiagnosticSource::Frontmatter => {
                                Meta::resolve_with_diagnostics(Some(&markdown), Some(fallback), "p")
                            }
                        };
                        let context = format!("{shape}/{leaf}/{depth}/{source:?}");
                        if depth == 123 {
                            assert!(result.diagnostics.is_empty(), "{context}");
                            assert_eq!(result.meta.attrs["value"], expected, "{context}");
                        } else {
                            assert_eq!(result.meta.attrs.len(), 1, "{context}");
                            assert_eq!(
                                result.meta.attrs["kept"],
                                serde_json::json!("fallback"),
                                "{context}"
                            );
                            assert_eq!(result.diagnostics.len(), 1, "{context}");
                            let diagnostic = &result.diagnostics[0];
                            assert_eq!(diagnostic.source, source, "{context}");
                            if depth == 127 {
                                // The existing YAML parser rejects the entire source first.
                                assert_eq!(diagnostic.field, None, "{context}");
                                assert_eq!(diagnostic.severity, Severity::Error, "{context}");
                                assert_eq!(result.meta.title, "Fallback", "{context}");
                            } else {
                                assert_eq!(diagnostic.field.as_deref(), Some("attrs"), "{context}");
                                assert_eq!(diagnostic.severity, Severity::Warning, "{context}");
                            }
                        }
                        if depth < 127 && source == DiagnosticSource::Frontmatter {
                            assert_eq!(result.meta.title, "Kept", "{context}");
                        }
                    }
                    // With no overlay, a valid sibling in the sidecar also survives.
                    if (124..=126).contains(&depth) {
                        let fields = format!("title: Kept\nattrs: {{value: {value}}}");
                        let result = Meta::resolve_with_diagnostics(None, Some(&fields), "p");
                        assert_eq!(result.meta.title, "Kept");
                        assert!(result.meta.attrs.is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn attrs_invalid_values_drop_whole_field_per_source() {
        for yaml in [
            "text",
            "42",
            "true",
            "[]",
            "null",
            "{kept: changed, 1: value}",
            "{kept: changed, !custom key: value}",
            "{kept: changed, bad: {1: value}}",
            "{kept: changed, bad: {true: value}}",
            "{kept: changed, bad: {null: value}}",
            "{kept: changed, bad: {[a]: value}}",
            "{kept: changed, bad: { !custom key: value }}",
            "{kept: changed, bad: .nan}",
            "{kept: changed, bad: .inf}",
            "{kept: changed, bad: -.inf}",
            "!custom scalar",
            "!custom {key: value}",
            "{kept: changed, bad: !custom scalar}",
            "{kept: changed, bad: !custom {key: value}}",
            "{kept: changed, bad: [!custom scalar]}",
            "{kept: changed, bad: [1, {2: value}]}",
        ] {
            for source in [DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter] {
                let fields = format!("attrs: {yaml}\ntitle: Kept");
                let markdown = format!("---\n{fields}\n---");
                let result = match source {
                    DiagnosticSource::Sidecar => {
                        Meta::resolve_with_diagnostics(None, Some(&fields), "p")
                    }
                    DiagnosticSource::Frontmatter => Meta::resolve_with_diagnostics(
                        Some(&markdown),
                        Some("attrs: {kept: fallback}"),
                        "p",
                    ),
                };
                assert_eq!(result.meta.title, "Kept", "{source:?}: {yaml}");
                if source == DiagnosticSource::Sidecar {
                    assert!(result.meta.attrs.is_empty(), "{yaml}");
                } else {
                    assert_eq!(result.meta.attrs.len(), 1, "{yaml}");
                    assert_eq!(
                        result.meta.attrs["kept"],
                        serde_json::json!("fallback"),
                        "{yaml}"
                    );
                }
                assert_eq!(result.diagnostics.len(), 1, "{source:?}: {yaml}");
                assert_eq!(
                    result.diagnostics[0].field.as_deref(),
                    Some("attrs"),
                    "{source:?}: {yaml}"
                );
                assert_eq!(result.diagnostics[0].source, source, "{yaml}");
                assert_eq!(result.diagnostics[0].severity, Severity::Warning, "{yaml}");
            }
        }
    }

    #[test]
    fn attrs_tagged_known_key_and_duplicate_source_recovery() {
        let result = Meta::resolve_with_diagnostics(
            None,
            Some("? !custom attrs\n: {kept: true}\ntitle: !custom 42"),
            "p",
        );
        assert_eq!(result.meta.attrs["kept"], serde_json::json!(true));
        assert_eq!(result.meta.title, "42");
        assert_eq!(result.diagnostics, [] as [diagnostic::Diagnostic; 0]);
        for yaml in [
            "attrs: {one: 1}\nattrs: {two: 2}\ntitle: Dropped",
            "attrs: {one: 1}\n? !custom attrs\n: {two: 2}\ntitle: Dropped",
        ] {
            for source in [DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter] {
                let markdown = format!("---\n{yaml}\n---");
                let result = match source {
                    DiagnosticSource::Sidecar => {
                        Meta::resolve_with_diagnostics(None, Some(yaml), "p")
                    }
                    DiagnosticSource::Frontmatter => Meta::resolve_with_diagnostics(
                        Some(&markdown),
                        Some("attrs: {kept: true}\ntitle: Fallback"),
                        "p",
                    ),
                };
                if source == DiagnosticSource::Sidecar {
                    assert!(result.meta.attrs.is_empty());
                    assert_eq!(result.meta.title, "P");
                } else {
                    assert_eq!(result.meta.attrs.len(), 1);
                    assert_eq!(result.meta.attrs["kept"], serde_json::json!(true));
                    assert_eq!(result.meta.title, "Fallback");
                }
                assert_eq!(result.diagnostics.len(), 1);
                assert_eq!(result.diagnostics[0].field, None);
                assert_eq!(result.diagnostics[0].source, source);
                assert_eq!(result.diagnostics[0].severity, Severity::Error);
            }
        }
    }

    #[test]
    fn attrs_diagnostics_follow_name_and_source_order() {
        let result = Meta::resolve_with_diagnostics(
            Some("---\nattrs: []\nname: []\n---"),
            Some("attrs: []\nname: []"),
            "p",
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|d| (d.source, d.field.as_deref()))
                .collect::<Vec<_>>(),
            [
                (DiagnosticSource::Sidecar, Some("name")),
                (DiagnosticSource::Sidecar, Some("attrs")),
                (DiagnosticSource::Frontmatter, Some("name")),
                (DiagnosticSource::Frontmatter, Some("attrs")),
            ]
        );
    }

    #[test]
    fn declared_name_scalar_and_identifier_boundaries() {
        for (yaml, expected) in [
            ("A", "A"),
            ("9", "9"),
            ("Pay-ments_API.v2", "Pay-ments_API.v2"),
            ("42", "42"),
            ("true", "true"),
            ("3.14", "3.14"),
            ("!custom Payments-API", "Payments-API"),
        ] {
            let result = Meta::resolve_with_diagnostics(
                None,
                Some(&format!("name: {yaml}\ntitle: Kept")),
                "page.md",
            );
            assert_eq!(result.meta.name.as_deref(), Some(expected), "{yaml}");
            assert_eq!(result.meta.title, "Kept");
            assert!(result.diagnostics.is_empty(), "{yaml}");
        }
        let longest = "a".repeat(63);
        assert_eq!(
            Meta::resolve(None, Some(&format!("name: {longest}")), "p").name,
            Some(longest)
        );
        assert_eq!(Meta::resolve(None, None, "path-derived").name, None);
    }

    #[test]
    fn declared_name_invalid_values_recover_per_source() {
        let too_long = "a".repeat(64);
        for yaml in [
            "''",
            "' '",
            "' padded'",
            "'padded '",
            "a/b",
            "a:b",
            "-start",
            "end-",
            "_start",
            "end_",
            ".start",
            "end.",
            "café",
            "a+b",
            "[a]",
            "{a: b}",
            "null",
            "~",
            "!custom [a]",
            &too_long,
        ] {
            for source in [DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter] {
                let fields = format!("name: {yaml}\ntitle: Kept");
                let markdown = format!("---\n{fields}\n---\n");
                let result = match source {
                    DiagnosticSource::Sidecar => {
                        Meta::resolve_with_diagnostics(None, Some(&fields), "p")
                    }
                    DiagnosticSource::Frontmatter => {
                        Meta::resolve_with_diagnostics(Some(&markdown), Some("name: fallback"), "p")
                    }
                };
                assert_eq!(
                    result.meta.name.as_deref(),
                    if source == DiagnosticSource::Sidecar {
                        None
                    } else {
                        Some("fallback")
                    },
                    "{source:?}: {yaml}"
                );
                assert_eq!(result.meta.title, "Kept", "{source:?}: {yaml}");
                assert_eq!(result.diagnostics.len(), 1, "{source:?}: {yaml}");
                let diagnostic = &result.diagnostics[0];
                assert_eq!(
                    diagnostic.field.as_deref(),
                    Some("name"),
                    "{source:?}: {yaml}"
                );
                assert_eq!(diagnostic.source, source, "{source:?}: {yaml}");
                assert_eq!(diagnostic.severity, Severity::Warning, "{source:?}: {yaml}");
            }
        }
    }

    #[test]
    fn declared_name_diagnostic_order() {
        let result =
            Meta::resolve_with_diagnostics(Some("---\nname: Valid\n---"), Some("name: []"), "p");
        assert_eq!(result.meta.name.as_deref(), Some("Valid"));
        assert_eq!(result.diagnostics[0].source, DiagnosticSource::Sidecar);
        let result = Meta::resolve_with_diagnostics(
            Some("---\nname: []\n---"),
            Some("name: []\npages: null\ndescription: []\ntitle: []\nnamespace: []\nkind: []"),
            "p",
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|d| d.field.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("kind"),
                Some("namespace"),
                Some("title"),
                Some("description"),
                Some("pages"),
                Some("name"),
                Some("name")
            ]
        );
        assert_eq!(result.diagnostics[5].source, DiagnosticSource::Sidecar);
        assert_eq!(result.diagnostics[6].source, DiagnosticSource::Frontmatter);
    }

    #[test]
    fn declared_name_tagged_key_and_duplicate_recovery() {
        let result = Meta::resolve_with_diagnostics(
            None,
            Some("? !custom name\n: Payments-API\ntitle: Kept"),
            "p",
        );
        assert_eq!(result.meta.name.as_deref(), Some("Payments-API"));
        assert_eq!(result.diagnostics, [] as [diagnostic::Diagnostic; 0]);
        for yaml in [
            "name: One\n? !custom name\n: Two\ntitle: Dropped",
            "name: One\nname: Two\ntitle: Dropped",
        ] {
            let result = Meta::resolve_with_diagnostics(None, Some(yaml), "p");
            assert_eq!(result.meta.name, None);
            assert_eq!(result.meta.title, "P");
            assert_eq!(result.diagnostics.len(), 1);
            assert_eq!(result.diagnostics[0].field, None);
            assert_eq!(result.diagnostics[0].severity, Severity::Error);
        }
    }

    // --- titlecase_from_slug ---

    #[test]
    fn titlecase_from_slug_cases() {
        for (slug, expected) in [
            ("setup-guide", "Setup Guide"),
            ("my_page", "My Page"),
            ("hello", "Hello"),
            ("", ""),
        ] {
            assert_eq!(titlecase_from_slug(slug), expected, "{slug:?}");
        }
    }

    #[test]
    fn frontmatter_overrides_sidecar_per_field() {
        type Getter = fn(&Meta) -> Option<String>;
        let fields: [(&str, Getter); 6] = [
            ("kind", |meta| meta.kind.clone()),
            ("namespace", |meta| meta.namespace.clone()),
            ("title", |meta| Some(meta.title.clone())),
            ("description", |meta| meta.description.clone()),
            ("pages", |meta| {
                meta.pages.as_ref().map(|pages| pages.join(","))
            }),
            ("name", |meta| meta.name.clone()),
        ];
        for (field, get) in fields {
            let yaml = |value| match field {
                "pages" => format!("pages: [{value}]"),
                _ => format!("{field}: {value}"),
            };
            let sidecar = yaml("sidecar");
            let frontmatter = format!("---\n{}\n---\n# H1", yaml("frontmatter"));
            for (markdown, meta_yaml, expected) in [
                ("# H1", Some(sidecar.as_str()), "sidecar"),
                (frontmatter.as_str(), None, "frontmatter"),
                (frontmatter.as_str(), Some(sidecar.as_str()), "frontmatter"),
            ] {
                let resolved = Meta::resolve_with_diagnostics(Some(markdown), meta_yaml, "p");
                let context = format!("{field}: {markdown:?} over {meta_yaml:?}");
                assert_eq!(get(&resolved.meta).as_deref(), Some(expected), "{context}");
                assert!(resolved.diagnostics.is_empty(), "{context}");
            }
        }
    }

    #[test]
    fn resolve_h1_wins_over_filename() {
        let md = "# H1 Title\n\nSome content.";
        let meta = Meta::resolve(Some(md), None, "page.md");
        assert_eq!(meta.title, "H1 Title");
    }

    #[test]
    fn resolve_h1_title_is_the_text_the_heading_shows() {
        for (md, expected) in [
            ("# Done :status[OK]{color=green}\n", "Done OK"),
            ("# See [[domain:x::y]]\n", "See domain:x::y"),
            ("# ![logo](logo.png)\n", "Setup Guide"),
        ] {
            let meta = Meta::resolve(Some(md), None, "setup-guide.md");
            assert_eq!(meta.title, expected, "{md:?}");
        }
    }

    #[test]
    fn resolve_meta_yaml_type_is_ignored_without_dropping_known_fields() {
        let meta = Meta::resolve(None, Some("title: Sidecar\ntype: domain"), "page.md");
        assert_eq!(meta.title, "Sidecar");
        assert!(meta.kind.is_none());
    }

    #[test]
    fn resolve_frontmatter_type_does_not_override_meta_yaml_kind() {
        let markdown = "---\ntitle: Frontmatter Title\ntype: service\n---\n# Page\n";
        let meta = Meta::resolve(Some(markdown), Some("kind: domain"), "page.md");
        assert_eq!(meta.title, "Frontmatter Title");
        assert_eq!(meta.kind.as_deref(), Some("domain"));
    }

    #[test]
    fn resolve_blank_declared_title_falls_through_to_next_source() {
        for blank in ["''", "' '", "\"\\t\"", "null", "~", ""] {
            let title = format!("title: {blank}");
            let frontmatter = |body: &str| format!("---\n{title}\n---\n{body}");
            for (markdown, meta_yaml, expected) in [
                (frontmatter("# Heading"), None, "Heading"),
                ("# Heading".to_owned(), Some(title.as_str()), "Heading"),
                (frontmatter("# Heading"), Some("title: Sidecar"), "Sidecar"),
                (frontmatter("Body"), Some(title.as_str()), "My Page"),
            ] {
                let resolved =
                    Meta::resolve_with_diagnostics(Some(&markdown), meta_yaml, "my-page.md");
                let context = format!("{markdown:?} over {meta_yaml:?}");
                assert_eq!(resolved.meta.title, expected, "{context}");
                assert!(resolved.diagnostics.is_empty(), "{context}");
            }
        }
    }

    #[test]
    fn resolve_blank_declared_description_falls_through_to_next_source() {
        for blank in ["''", "' '", "null", "~", ""] {
            let description = format!("description: {blank}");
            let markdown = format!("---\n{description}\n---\n# H1");
            for (meta_yaml, expected) in [
                (Some("description: Sidecar"), Some("Sidecar")),
                (None, None),
            ] {
                let resolved = Meta::resolve_with_diagnostics(Some(&markdown), meta_yaml, "p.md");
                let context = format!("{markdown:?} over {meta_yaml:?}");
                assert_eq!(resolved.meta.description.as_deref(), expected, "{context}");
                assert!(resolved.diagnostics.is_empty(), "{context}");
            }
            let resolved = Meta::resolve_with_diagnostics(None, Some(&description), "p.md");
            assert_eq!(resolved.meta.description, None, "{description}");
            assert!(resolved.diagnostics.is_empty(), "{description}");
        }
    }

    #[test]
    fn resolve_padded_title_and_description_stay_as_written() {
        let meta = Meta::resolve(
            Some("---\ntitle: '  Foo  '\ndescription: ' Bar '\n---\n# Heading"),
            None,
            "p.md",
        );
        assert_eq!(meta.title, "  Foo  ");
        assert_eq!(meta.description.as_deref(), Some(" Bar "));
    }

    #[test]
    fn resolve_blank_h1_falls_back_to_filename() {
        for h1 in ["# \n", "#   \n", "# &#32;\n", "# &nbsp;\n", "# **&#32;**\n"] {
            let markdown = format!("{h1}\nSome content.");
            assert_eq!(
                Meta::resolve(Some(&markdown), None, "setup-guide.md").title,
                "Setup Guide",
                "{h1:?}"
            );
        }
    }

    #[test]
    fn resolve_empty_h1_falls_back_to_filename() {
        let md = "# \n\nSome content.";
        let meta = Meta::resolve(Some(md), None, "setup-guide.md");
        assert_eq!(meta.title, "Setup Guide");
    }

    #[test]
    fn resolve_filename_of_only_underscore_falls_back_to_stem_verbatim() {
        let meta = Meta::resolve(None, None, "_.md");
        assert_eq!(meta.title, "_");
    }

    #[test]
    fn resolve_filename_dot_md_only_falls_back_to_untitled() {
        let meta = Meta::resolve(None, None, ".md");
        assert_eq!(meta.title, "Untitled");
    }

    #[test]
    fn resolve_no_sources() {
        assert_eq!(
            Meta::resolve(None, None, "some-page.md"),
            Meta {
                kind: None,
                namespace: None,
                title: "Some Page".to_owned(),
                description: None,
                pages: None,
                name: None,
                attrs: BTreeMap::new(),
            }
        );
    }

    // --- resolve_with_diagnostics ---

    #[test]
    fn resolve_with_diagnostics_lists_sidecar_before_frontmatter() {
        let md = "---\ntitle: [a, b]\n---\n# H\n";
        let resolved = Meta::resolve_with_diagnostics(Some(md), Some("kind: [x]"), "page.md");
        let sources: Vec<_> = resolved.diagnostics.iter().map(|d| d.source).collect();
        assert_eq!(
            sources,
            vec![DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter]
        );
    }

    #[test]
    fn invalid_frontmatter_field_recovers_to_sidecar_value() {
        let md = "---\ntitle: [a, b]\nkind: guide\n---\n# H1\n";
        let resolved = Meta::resolve_with_diagnostics(Some(md), Some("title: Sidecar"), "page.md");
        assert_eq!(resolved.meta.title, "Sidecar");
        assert_eq!(resolved.meta.kind.as_deref(), Some("guide"));
    }

    #[test]
    fn resolve_drops_diagnostics_and_keeps_meta() {
        let md = "---\ndescription: [x]\n---\n# H1\n";
        let meta_yaml = "title: Sidecar\ndescription: Valid";
        let resolved = Meta::resolve_with_diagnostics(Some(md), Some(meta_yaml), "page.md");
        assert_ne!(resolved.diagnostics, [] as [diagnostic::Diagnostic; 0]);
        let meta = Meta::resolve(Some(md), Some(meta_yaml), "page.md");
        assert_eq!(meta, resolved.meta);
    }
}
