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

use std::path::PathBuf;

use rw_diagrams::{SiteModel, canonical_language};

/// Whether a fence language carries `PlantUML` source.
///
/// RW accepts a `kroki-` prefix on any diagram fence (`kroki-plantuml` is the
/// `MkDocs` Kroki plugin's spelling), so both spellings answer the same.
#[must_use]
pub fn is_plantuml_fence(language: &str) -> bool {
    matches!(canonical_language(language), "plantuml" | "c4plantuml")
}

/// The human-readable lines a search index wants from a diagram fence.
///
/// `None` for a language that carries no `PlantUML` source: its fence body is
/// already the text to index, so there is nothing to strip. A caller rewriting
/// fences can use `None` as the signal to leave the fence untouched.
///
/// Include warnings are discarded rather than returned: the search document
/// carries only a title and a text body, so no caller has anywhere to put them.
#[must_use]
pub fn search_text(
    language: &str,
    source: &str,
    include_dirs: &[PathBuf],
    model: Option<&dyn SiteModel>,
) -> Option<String> {
    is_plantuml_fence(language).then(|| {
        // Includes are resolved first so entity titles and descriptions reach
        // the index — for a C4 diagram they are often the only prose it has.
        let mut warnings = Vec::new();
        let resolved = resolve_includes(source, include_dirs, model, &mut warnings);
        strip_plantuml_boilerplate(&resolved)
    })
}

#[cfg(test)]
mod tests {
    use super::{is_plantuml_fence, search_text};
    use rw_diagrams::{Entity, SiteModel};

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

    #[test]
    fn search_text_strips_plantuml_boilerplate() {
        let source = "@startuml\nskinparam defaultFontName Roboto\n!include common.puml\nPerson(user, \"User\", \"A user\")\nSystem(sys, \"System\", \"The system\")\n@enduml";
        let text = search_text("plantuml", source, &[], None).expect("plantuml fence has text");

        assert!(!text.contains("@startuml"), "got: {text}");
        assert!(!text.contains("skinparam"), "got: {text}");
        assert!(!text.contains("!include"), "got: {text}");
        assert!(text.contains("Person(user,"), "got: {text}");
        assert!(text.contains("System(sys,"), "got: {text}");
    }

    #[test]
    fn search_text_strips_c4plantuml_boilerplate() {
        let source = "@startuml\n!include C4_Context.puml\nPerson(user, \"User\")\nSystem(sys, \"System\")\n@enduml";
        let text = search_text("c4plantuml", source, &[], None).expect("c4plantuml fence has text");

        assert!(!text.contains("@startuml"), "got: {text}");
        assert!(!text.contains("!include"), "got: {text}");
        assert!(text.contains("Person(user,"), "got: {text}");
        assert!(text.contains("System(sys,"), "got: {text}");
    }

    /// A non-`PlantUML` diagram fence is already the text to index, so
    /// `search_text` returns `None` and the caller leaves the fence untouched.
    #[test]
    fn search_text_declines_a_non_plantuml_diagram() {
        assert_eq!(
            search_text("mermaid", "graph TD\n    A-->B", &[], None),
            None
        );
    }

    #[test]
    fn search_text_declines_a_prose_fence() {
        assert_eq!(search_text("python", "def hello(): pass", &[], None), None);
    }

    /// Meta includes are resolved before stripping: for a C4 diagram the entity
    /// titles and descriptions they pull in are often its only indexable prose.
    #[test]
    fn search_text_resolves_meta_includes() {
        struct TestModel;
        impl SiteModel for TestModel {
            fn entity(&self, _kind: &str, _name: &str) -> Option<Entity> {
                Some(Entity {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: None,
                })
            }
        }

        let source =
            "@startuml\n!include systems/sys_payment_gateway.iuml\nRel(a, b, \"uses\")\n@enduml";
        let text = search_text("plantuml", source, &[], Some(&TestModel))
            .expect("plantuml fence has text");

        assert!(text.contains("Payment Gateway"), "got: {text}");
        assert!(text.contains("Processes payments"), "got: {text}");
        assert!(text.contains("Rel(a, b,"), "got: {text}");
        assert!(!text.contains("@startuml"), "got: {text}");
        assert!(!text.contains("!include"), "got: {text}");
    }
}
