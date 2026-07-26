//! `PlantUML` source preprocessing for RW.
//!
//! Every item here is a text transform over `PlantUML` source: resolving
//! `!include` directives against the filesystem and the site model, injecting
//! the render configuration a diagram needs, and reducing a diagram to the
//! human-readable lines a search index wants.
//!
//! Nothing here renders a diagram, so nothing here needs an HTTP client or a
//! thread pool — which is why the S3 publish path can resolve includes
//! without linking a diagram renderer.
//!
//! # Example
//!
//! ```
//! use rw_plantuml::{is_plantuml_fence, prepare_diagram_source};
//!
//! assert!(is_plantuml_fence("kroki-c4plantuml"));
//!
//! let prepared = prepare_diagram_source("@startuml\nA -> B\n@enduml", &[], 192, None);
//! assert!(prepared.source.contains("skinparam dpi 192"));
//! assert!(prepared.warnings.is_empty());
//! ```

mod includes;
mod meta_includes;
mod text;

pub use includes::{PrepareResult, bundle_source, prepare_diagram_source, resolve_includes};
pub use text::strip_plantuml_boilerplate;

/// Whether a fence language carries `PlantUML` source.
///
/// RW accepts a `kroki-` prefix on any diagram fence (`kroki-plantuml` is the
/// `MkDocs` Kroki plugin's spelling), so both spellings answer the same.
#[must_use]
pub fn is_plantuml_fence(language: &str) -> bool {
    matches!(
        language.strip_prefix("kroki-").unwrap_or(language),
        "plantuml" | "c4plantuml"
    )
}

#[cfg(test)]
mod tests {
    use super::is_plantuml_fence;

    #[test]
    fn recognizes_the_plantuml_family() {
        assert!(is_plantuml_fence("plantuml"));
        assert!(is_plantuml_fence("c4plantuml"));
    }

    #[test]
    fn recognizes_the_kroki_prefixed_spelling() {
        assert!(is_plantuml_fence("kroki-plantuml"));
        assert!(is_plantuml_fence("kroki-c4plantuml"));
    }

    #[test]
    fn rejects_other_diagram_languages_and_prose_fences() {
        assert!(!is_plantuml_fence("mermaid"));
        assert!(!is_plantuml_fence("graphviz"));
        assert!(!is_plantuml_fence("rust"));
        assert!(!is_plantuml_fence(""));
    }

    #[test]
    fn strips_the_prefix_once_and_matches_the_whole_language() {
        assert!(!is_plantuml_fence("kroki-kroki-plantuml"));
        assert!(!is_plantuml_fence("plantuml-c4"));
    }
}
