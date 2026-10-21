//! What a provider is asked to render.

use std::path::Path;

use crate::model::SiteModel;
use crate::reads::FileStamp;

/// The canonical spelling of a fence language, with compatibility prefixes
/// removed.
///
/// RW accepts a `kroki-` prefix on any diagram fence, for compatibility with
/// the `MkDocs` Kroki plugin. Every language predicate should ask about the
/// canonical spelling, so a provider that adds an alias adds it once and every
/// path — render, search, publish — accepts it.
#[must_use]
pub fn canonical_language(language: &str) -> &str {
    language.strip_prefix("kroki-").unwrap_or(language)
}

/// Diagram file name suffixes and the fence language each one stands for.
///
/// No `iuml`: by `PlantUML` convention that is a file meant to be `!include`d,
/// not rendered on its own. No `erd`: the ERD Editor's JSON files use it. No
/// bare `json`, which says nothing about what the file holds.
const FILE_LANGUAGES: &[(&str, &str)] = &[
    ("puml", "plantuml"),
    ("plantuml", "plantuml"),
    ("pu", "plantuml"),
    ("wsd", "plantuml"),
    ("mmd", "mermaid"),
    ("mermaid", "mermaid"),
    ("dot", "graphviz"),
    ("gv", "graphviz"),
    ("ditaa", "ditaa"),
    ("bob", "svgbob"),
    ("nomnoml", "nomnoml"),
    ("noml", "nomnoml"),
    ("er", "erd"),
    ("vega", "vega"),
    ("vg", "vega"),
    ("vg.json", "vega"),
    ("vl", "vegalite"),
    ("vl.json", "vegalite"),
];

/// The fence language a diagram file is written in, from the suffix of its
/// name (matched case-insensitively), or `None` when the file is not a
/// diagram.
///
/// A suffix may span dots, as `vg.json` does, but needs a stem before it:
/// `.vg.json` and `vg.json` are not diagrams.
///
/// Whether some provider renders that language is a separate question, for
/// [`DiagramRouter::handles`](crate::DiagramRouter::handles).
///
/// # Example
///
/// ```
/// use rw_diagrams::diagram_language;
///
/// assert_eq!(diagram_language("diagrams/payment.puml"), Some("plantuml"));
/// assert_eq!(diagram_language("charts/sales.vl.json"), Some("vegalite"));
/// assert_eq!(diagram_language("img/logo.png"), None);
/// ```
#[must_use]
pub fn diagram_language(path: &str) -> Option<&'static str> {
    let name = Path::new(path).file_name()?.to_str()?.as_bytes();
    FILE_LANGUAGES
        .iter()
        .filter(|(suffix, _)| {
            name.len().checked_sub(suffix.len()).is_some_and(|split| {
                let (head, tail) = name.split_at(split);
                tail.eq_ignore_ascii_case(suffix.as_bytes())
                    && head.len() > 1
                    && head.ends_with(b".")
            })
        })
        .max_by_key(|(suffix, _)| suffix.len())
        .map(|&(_, language)| language)
}

/// `PlantUML` file extensions, `iuml` among them, unlike [`FILE_LANGUAGES`].
const PLANTUML_EXTENSIONS: &[&str] = &["puml", "iuml", "plantuml", "pu", "wsd"];

/// Whether `path` has a `PlantUML` file extension (`.puml`, `.iuml`, ...),
/// matched case-insensitively: the only files an include may name.
///
/// # Example
///
/// ```
/// use std::path::Path;
///
/// use rw_diagrams::is_plantuml_file;
///
/// assert!(is_plantuml_file(Path::new("shared/parts.IUML")));
/// assert!(!is_plantuml_file(Path::new("flow.mmd")));
/// ```
#[must_use]
pub fn is_plantuml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PLANTUML_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Where a diagram's source text comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagramSource {
    /// Fence body, exactly as authored — a provider does its own preprocessing.
    Inline(String),
    /// A diagram file, as a project-relative, `/`-separated path the
    /// renderer's file resolver placed. The provider reads it.
    File(String),
}

/// One diagram to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramRequest {
    /// Fence language as the author wrote it, e.g. `plantuml` or `kroki-mermaid`;
    /// for a file, its [`diagram_language`].
    pub language: String,
    /// The diagram's source text, or where to read it.
    pub source: DiagramSource,
    /// Fence attributes in author order, e.g. `format=png`. A provider ignores
    /// keys it does not know, and should warn rather than fail on them.
    ///
    /// A key may appear more than once, and **the last occurrence wins** — a
    /// contract on every provider. It is how a caller overrides what the author
    /// wrote (as `rw-confluence` appends `format=png`) while the author's value
    /// stays in the list to be diagnosed.
    pub attrs: Vec<(String, String)>,
    /// The writer's `{#id}`, when they set one.
    pub id: Option<String>,
}

/// Per-render inputs, identical for every fence on one page.
///
/// Passed to [`resolve`](crate::DiagramProvider::resolve) rather than held by
/// the provider, so a provider's own state — an HTTP agent, a parsed model —
/// can live as long as the process while these change per page.
#[derive(Default)]
pub struct ResolveContext<'a> {
    /// The site's entities, for providers whose syntax references them. `None`
    /// when the caller has no model to offer; a provider that needs one should
    /// report an error for the requests that depended on it.
    pub model: Option<&'a dyn SiteModel>,
    /// Called with every file a provider read beyond the request, such as a
    /// `PlantUML` include. `None` when the caller does not cache output.
    pub record_read: Option<&'a (dyn Fn(FileStamp) + Sync)>,
}

#[cfg(test)]
mod tests {
    use super::{canonical_language, diagram_language};

    #[test]
    fn canonicalization_strips_the_kroki_compatibility_prefix() {
        assert_eq!(canonical_language("kroki-mermaid"), "mermaid");
        assert_eq!(canonical_language("kroki-c4plantuml"), "c4plantuml");
    }

    #[test]
    fn canonicalization_leaves_an_unprefixed_language_alone() {
        assert_eq!(canonical_language("plantuml"), "plantuml");
        assert_eq!(canonical_language("rust"), "rust");
    }

    /// Only the leading prefix goes, and only once: `kroki-kroki-x` is not a
    /// spelling RW accepts, so it must not be canonicalized into one.
    #[test]
    fn canonicalization_strips_at_most_one_leading_prefix() {
        assert_eq!(canonical_language("kroki-kroki-mermaid"), "kroki-mermaid");
        assert_eq!(canonical_language("my-kroki-thing"), "my-kroki-thing");
    }

    #[test]
    fn diagram_extensions_map_to_their_language() {
        for (path, language) in [
            ("a.puml", "plantuml"),
            ("a.plantuml", "plantuml"),
            ("a.pu", "plantuml"),
            ("a.wsd", "plantuml"),
            ("a.mmd", "mermaid"),
            ("a.mermaid", "mermaid"),
            ("a.dot", "graphviz"),
            ("a.gv", "graphviz"),
            ("a.ditaa", "ditaa"),
            ("a.bob", "svgbob"),
            ("a.nomnoml", "nomnoml"),
            ("a.noml", "nomnoml"),
            ("a.er", "erd"),
            ("a.vega", "vega"),
            ("a.vg", "vega"),
            ("a.vg.json", "vega"),
            ("a.vl", "vegalite"),
            ("a.vl.json", "vegalite"),
            ("diagrams/Flow.PUML", "plantuml"),
            ("a.Mmd", "mermaid"),
            ("A.DITAA", "ditaa"),
            ("Art.Bob", "svgbob"),
            ("a.NOML", "nomnoml"),
            ("Schema.ER", "erd"),
            ("x.VG.JSON", "vega"),
            ("x.Vl.Json", "vegalite"),
            (".github/x.puml", "plantuml"),
        ] {
            assert_eq!(diagram_language(path), Some(language), "{path}");
        }
    }

    #[test]
    fn a_compound_suffix_follows_any_stem() {
        for (path, language) in [
            ("a.b.vl.json", "vegalite"),
            ("charts/2024.sales.vg.json", "vega"),
            ("..vg.json", "vega"),
            ("a.json.vg", "vega"),
        ] {
            assert_eq!(diagram_language(path), Some(language), "{path}");
        }
    }

    #[test]
    fn other_files_are_not_diagrams() {
        for path in [
            "",
            "a.iuml",
            "puml",
            "diagrams/puml",
            ".puml",
            "a.png",
            "a.md",
            "a.puml.bak",
            "dir.puml/readme",
            "a.json",
            "a.json5",
            "a.txt",
            "a.erd",
            "a.diag",
            "a.dsl",
            ".vg.json",
            "vg.json",
            "charts/.vl.json",
            "notvg.json",
            "a.vgjson",
            "a.vg.json.bak",
            "dir.vg.json/readme",
            "a.ver",
        ] {
            assert_eq!(diagram_language(path), None, "{path}");
        }
    }
}
