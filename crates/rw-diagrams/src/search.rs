//! Code block processor for search document rendering.
//!
//! [`SearchDiagramProcessor`] strips diagram boilerplate (`PlantUML` directives,
//! skinparam, includes) and returns human-readable text for search indexing.
//! Non-diagram code blocks pass through unchanged.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rw_renderer::{CodeBlockProcessor, ProcessResult};

use crate::language::DiagramLanguage;
use crate::meta_includes::MetaIncludeSource;
use crate::plantuml::resolve_includes;

/// Lines starting with these prefixes are stripped from `PlantUML` source
/// during search document generation.
const PLANTUML_BOILERPLATE_PREFIXES: &[&str] = &[
    "@start",
    "@end",
    "skinparam",
    "!include",
    "!define",
    "!$",
    "hide ",
    "show ",
];

/// Code block processor that strips diagram boilerplate for search indexing.
///
/// PlantUML/C4 diagrams have their boilerplate lines removed, keeping only
/// human-readable content (system names, descriptions, relationships).
/// Other diagram languages (Mermaid, `GraphViz`, etc.) pass through with raw
/// source. Non-diagram code blocks pass through to the backend's `code_block()`.
pub struct SearchDiagramProcessor {
    include_dirs: Vec<PathBuf>,
    meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    warnings: Vec<String>,
}

impl SearchDiagramProcessor {
    /// Create a new search diagram processor.
    ///
    /// `include_dirs` are used for resolving `PlantUML` `!include` directives
    /// during bundling. On S3 storage, includes are already resolved at publish
    /// time, so pass an empty vec.
    #[must_use]
    pub fn new(include_dirs: Vec<PathBuf>) -> Self {
        Self {
            include_dirs,
            meta_include_source: None,
            warnings: Vec::new(),
        }
    }

    /// Set a meta include source for resolving entity-based `!include` directives.
    ///
    /// When set, `PlantUML` `!include` paths matching the meta pattern
    /// (e.g., `systems/sys_payment_gateway.iuml`) are resolved to C4 macros
    /// containing system titles and descriptions — valuable for search indexing.
    #[must_use]
    pub fn with_meta_include_source(mut self, source: Arc<dyn MetaIncludeSource>) -> Self {
        self.meta_include_source = Some(source);
        self
    }
}

/// Check whether a line is `PlantUML` boilerplate.
fn is_plantuml_boilerplate(line: &str) -> bool {
    let trimmed = line.trim();
    PLANTUML_BOILERPLATE_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Strip `PlantUML` boilerplate lines, keeping human-readable content.
fn strip_plantuml_boilerplate(source: &str) -> String {
    source
        .lines()
        .filter(|line| !is_plantuml_boilerplate(line))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

impl CodeBlockProcessor for SearchDiagramProcessor {
    fn process(
        &mut self,
        language: &str,
        _attrs: &HashMap<String, String>,
        source: &str,
        _index: usize,
    ) -> ProcessResult {
        let Some(lang) = DiagramLanguage::parse(language) else {
            return ProcessResult::PassThrough;
        };

        let text = if lang.needs_plantuml_preprocessing() {
            // Resolve meta includes so entity titles/descriptions appear in search text.
            let resolved = resolve_includes(
                source,
                &self.include_dirs,
                self.meta_include_source.as_deref(),
                0,
                &mut self.warnings,
            );
            strip_plantuml_boilerplate(&resolved)
        } else {
            source.to_owned()
        };

        ProcessResult::Inline(text)
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rw_renderer::ProcessResult;

    use super::*;

    #[test]
    fn strips_plantuml_boilerplate() {
        let mut processor = SearchDiagramProcessor::new(vec![]);
        let source = "@startuml\nskinparam defaultFontName Roboto\n!include common.puml\nPerson(user, \"User\", \"A user\")\nSystem(sys, \"System\", \"The system\")\n@enduml";
        let result = processor.process("plantuml", &HashMap::new(), source, 0);

        match result {
            ProcessResult::Inline(text) => {
                assert!(!text.contains("@startuml"));
                assert!(!text.contains("skinparam"));
                assert!(!text.contains("!include"));
                assert!(text.contains("Person(user,"));
                assert!(text.contains("System(sys,"));
            }
            other => panic!("Expected Inline, got {other:?}"),
        }
    }

    #[test]
    fn non_diagram_passes_through() {
        let mut processor = SearchDiagramProcessor::new(vec![]);
        let result = processor.process("python", &HashMap::new(), "def hello(): pass", 0);
        assert!(matches!(result, ProcessResult::PassThrough));
    }

    #[test]
    fn strips_c4plantuml_boilerplate() {
        let mut processor = SearchDiagramProcessor::new(vec![]);
        let source = "@startuml\n!include C4_Context.puml\nPerson(user, \"User\")\nSystem(sys, \"System\")\n@enduml";
        let result = processor.process("c4plantuml", &HashMap::new(), source, 0);

        match result {
            ProcessResult::Inline(text) => {
                assert!(!text.contains("@startuml"));
                assert!(!text.contains("!include"));
                assert!(text.contains("Person(user,"));
                assert!(text.contains("System(sys,"));
            }
            other => panic!("Expected Inline, got {other:?}"),
        }
    }

    #[test]
    fn mermaid_passes_raw_source() {
        let mut processor = SearchDiagramProcessor::new(vec![]);
        let source = "graph TD\n    A-->B\n    B-->C";
        let result = processor.process("mermaid", &HashMap::new(), source, 0);

        match result {
            ProcessResult::Inline(text) => {
                assert!(text.contains("A-->B"));
            }
            other => panic!("Expected Inline, got {other:?}"),
        }
    }

    #[test]
    fn resolves_meta_includes() {
        use crate::meta_includes::{EntityInfo, MetaIncludeSource};

        struct TestSource;
        impl MetaIncludeSource for TestSource {
            fn get_entity(&self, _entity_type: &str, _name: &str) -> Option<EntityInfo> {
                Some(EntityInfo {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: None,
                })
            }
        }

        let mut processor =
            SearchDiagramProcessor::new(vec![]).with_meta_include_source(Arc::new(TestSource));
        let source =
            "@startuml\n!include systems/sys_payment_gateway.iuml\nRel(a, b, \"uses\")\n@enduml";
        let result = processor.process("plantuml", &HashMap::new(), source, 0);

        match result {
            ProcessResult::Inline(text) => {
                assert!(text.contains("Payment Gateway"));
                assert!(text.contains("Processes payments"));
                assert!(text.contains("Rel(a, b,"));
                assert!(!text.contains("@startuml"));
                assert!(!text.contains("!include"));
            }
            other => panic!("Expected Inline, got {other:?}"),
        }
    }
}
