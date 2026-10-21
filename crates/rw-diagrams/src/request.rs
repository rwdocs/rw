//! What a provider is asked to render.

use std::path::Path;

use crate::model::SiteModel;
use crate::reads::FileStamp;

/// The canonical spelling of a fence language, with compatibility prefixes
/// removed.
///
/// RW accepts a `kroki-` prefix on any diagram fence, for compatibility with
/// the `MkDocs` Kroki plugin. Every language predicate should ask about the
/// canonical spelling, so an alias works on every path.
#[must_use]
pub fn canonical_language(language: &str) -> &str {
    language.strip_prefix("kroki-").unwrap_or(language)
}

/// Diagram file extensions and the fence language each one stands for.
///
/// No `iuml`: by `PlantUML` convention that is a file meant to be `!include`d,
/// not rendered on its own.
const FILE_LANGUAGES: &[(&str, &str)] = &[
    ("puml", "plantuml"),
    ("plantuml", "plantuml"),
    ("pu", "plantuml"),
    ("wsd", "plantuml"),
    ("mmd", "mermaid"),
    ("mermaid", "mermaid"),
    ("dot", "graphviz"),
    ("gv", "graphviz"),
];

/// The fence language a diagram file is written in, from its extension
/// (matched case-insensitively), or `None` when the file is not a diagram.
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
/// assert_eq!(diagram_language("img/logo.png"), None);
/// ```
#[must_use]
pub fn diagram_language(path: &str) -> Option<&'static str> {
    let extension = Path::new(path).extension()?.to_str()?;
    FILE_LANGUAGES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(extension))
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
    /// contract on every provider: a caller overrides the author by appending.
    pub attrs: Vec<(String, String)>,
    /// The writer's `{#id}`, when they set one.
    pub id: Option<String>,
}

/// Per-render inputs, identical for every fence on one page.
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
            ("diagrams/Flow.PUML", "plantuml"),
            ("a.Mmd", "mermaid"),
            (".github/x.puml", "plantuml"),
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
        ] {
            assert_eq!(diagram_language(path), None, "{path}");
        }
    }
}
