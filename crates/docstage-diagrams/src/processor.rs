//! Code block processor for diagram languages.
//!
//! This module provides [`DiagramProcessor`], which implements the
//! [`CodeBlockProcessor`] trait for extracting diagram code blocks during rendering.

use std::collections::HashMap;

use docstage_renderer::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};

use crate::language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};

/// Code block processor for diagram languages.
///
/// Extracts diagram code blocks (PlantUML, Mermaid, GraphViz, etc.) and replaces
/// them with placeholders during rendering. After rendering, use
/// [`MarkdownRenderer::extracted_code_blocks`] and
/// [`MarkdownRenderer::processor_warnings`] to retrieve the data.
///
/// # Example
///
/// ```ignore
/// use pulldown_cmark::Parser;
/// use docstage_diagrams::{DiagramProcessor, to_extracted_diagrams};
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
///
/// let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
/// let parser = Parser::new(markdown);
/// let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_processor(DiagramProcessor::new());
///
/// let result = renderer.render(parser);
/// let diagrams = to_extracted_diagrams(&renderer.extracted_code_blocks());
/// let warnings = renderer.processor_warnings();
/// ```
#[derive(Default)]
pub struct DiagramProcessor {
    extracted: Vec<ExtractedCodeBlock>,
    warnings: Vec<String>,
}

impl DiagramProcessor {
    /// Create a new diagram processor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CodeBlockProcessor for DiagramProcessor {
    fn process(
        &mut self,
        language: &str,
        attrs: &HashMap<String, String>,
        source: &str,
        index: usize,
    ) -> ProcessResult {
        let Some(diagram_language) = DiagramLanguage::parse(language) else {
            return ProcessResult::PassThrough;
        };

        // Parse format attribute with validation
        let format = attrs.get("format").map_or(DiagramFormat::default(), |value| {
            DiagramFormat::parse(value).unwrap_or_else(|| {
                self.warnings.push(format!(
                    "diagram {index}: unknown format value '{value}', using default 'svg' (valid: svg, png)"
                ));
                DiagramFormat::default()
            })
        });

        // Warn about unknown attributes
        for key in attrs.keys().filter(|k| *k != "format") {
            self.warnings.push(format!(
                "diagram {index}: unknown attribute '{key}' ignored (valid: format)"
            ));
        }

        // Store the format in attrs for later use
        let mut stored_attrs = attrs.clone();
        stored_attrs.insert("format".to_string(), format.as_str().to_string());
        stored_attrs.insert(
            "endpoint".to_string(),
            diagram_language.kroki_endpoint().to_string(),
        );

        // Extract the code block
        self.extracted.push(ExtractedCodeBlock {
            index,
            language: language.to_string(),
            source: source.to_string(),
            attrs: stored_attrs,
        });

        ProcessResult::Placeholder(format!("{{{{DIAGRAM_{index}}}}}"))
    }

    fn extracted(&self) -> Vec<ExtractedCodeBlock> {
        self.extracted.clone()
    }

    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

/// Convert an [`ExtractedCodeBlock`] to an [`ExtractedDiagram`].
///
/// Returns `None` if the code block is not a diagram type.
///
/// # Example
///
/// ```ignore
/// use docstage_diagrams::{DiagramProcessor, to_extracted_diagram};
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
///
/// // After rendering with DiagramProcessor...
/// let blocks = renderer.extracted_code_blocks();
/// for block in &blocks {
///     if let Some(diagram) = to_extracted_diagram(block) {
///         println!("Diagram: {:?}", diagram.language);
///     }
/// }
/// ```
#[must_use]
pub fn to_extracted_diagram(block: &ExtractedCodeBlock) -> Option<ExtractedDiagram> {
    let language = DiagramLanguage::parse(&block.language)?;
    let format = block
        .attrs
        .get("format")
        .and_then(|s| DiagramFormat::parse(s))
        .unwrap_or_default();

    Some(ExtractedDiagram {
        source: block.source.clone(),
        index: block.index,
        language,
        format,
    })
}

/// Convert multiple [`ExtractedCodeBlock`]s to [`ExtractedDiagram`]s.
///
/// Filters out non-diagram blocks automatically.
///
/// # Example
///
/// ```ignore
/// use docstage_diagrams::{DiagramProcessor, to_extracted_diagrams};
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
///
/// // After rendering with DiagramProcessor...
/// let blocks = renderer.extracted_code_blocks();
/// let diagrams = to_extracted_diagrams(&blocks);
/// ```
#[must_use]
pub fn to_extracted_diagrams(blocks: &[ExtractedCodeBlock]) -> Vec<ExtractedDiagram> {
    blocks.iter().filter_map(to_extracted_diagram).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_plantuml() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();
        let source = "@startuml\nA -> B\n@enduml";

        let result = processor.process("plantuml", &attrs, source, 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
        );
        assert_eq!(processor.extracted().len(), 1);
        assert_eq!(processor.extracted()[0].language, "plantuml");
        assert_eq!(processor.extracted()[0].source, source);
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_mermaid() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("mermaid", &attrs, "graph TD\n  A --> B", 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
        );
        assert_eq!(processor.extracted()[0].language, "mermaid");
    }

    #[test]
    fn test_process_kroki_prefix() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("kroki-mermaid", &attrs, "graph TD", 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
        );
        assert_eq!(processor.extracted()[0].language, "kroki-mermaid");
    }

    #[test]
    fn test_process_non_diagram() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);

        assert_eq!(result, ProcessResult::PassThrough);
        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_process_with_format_png() {
        let mut processor = DiagramProcessor::new();
        let mut attrs = HashMap::new();
        attrs.insert("format".to_string(), "png".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"png".to_string())
        );
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_with_invalid_format() {
        let mut processor = DiagramProcessor::new();
        let mut attrs = HashMap::new();
        attrs.insert("format".to_string(), "jpeg".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        // Should default to svg
        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"svg".to_string())
        );
        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown format value 'jpeg'"));
    }

    #[test]
    fn test_process_with_unknown_attribute() {
        let mut processor = DiagramProcessor::new();
        let mut attrs = HashMap::new();
        attrs.insert("size".to_string(), "large".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_process_multiple_diagrams() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        processor.process("plantuml", &attrs, "source1", 0);
        processor.process("mermaid", &attrs, "source2", 1);

        assert_eq!(processor.extracted().len(), 2);
        assert_eq!(processor.extracted()[0].index, 0);
        assert_eq!(processor.extracted()[1].index, 1);
    }

    #[test]
    fn test_to_extracted_diagram() {
        let block = ExtractedCodeBlock {
            index: 0,
            language: "plantuml".to_string(),
            source: "@startuml\nA -> B\n@enduml".to_string(),
            attrs: HashMap::from([("format".to_string(), "png".to_string())]),
        };

        let diagram = to_extracted_diagram(&block).unwrap();

        assert_eq!(diagram.index, 0);
        assert_eq!(diagram.language, DiagramLanguage::PlantUml);
        assert_eq!(diagram.format, DiagramFormat::Png);
        assert!(diagram.source.contains("A -> B"));
    }

    #[test]
    fn test_to_extracted_diagram_non_diagram() {
        let block = ExtractedCodeBlock {
            index: 0,
            language: "rust".to_string(),
            source: "fn main() {}".to_string(),
            attrs: HashMap::new(),
        };

        assert!(to_extracted_diagram(&block).is_none());
    }

    #[test]
    fn test_to_extracted_diagrams() {
        let blocks = vec![
            ExtractedCodeBlock {
                index: 0,
                language: "plantuml".to_string(),
                source: "source1".to_string(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 1,
                language: "rust".to_string(), // Not a diagram
                source: "source2".to_string(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 2,
                language: "mermaid".to_string(),
                source: "source3".to_string(),
                attrs: HashMap::new(),
            },
        ];

        let diagrams = to_extracted_diagrams(&blocks);

        assert_eq!(diagrams.len(), 2);
        assert_eq!(diagrams[0].index, 0);
        assert_eq!(diagrams[0].language, DiagramLanguage::PlantUml);
        assert_eq!(diagrams[1].index, 2);
        assert_eq!(diagrams[1].language, DiagramLanguage::Mermaid);
    }

    #[test]
    fn test_stores_endpoint_in_attrs() {
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs.get("endpoint"),
            Some(&"plantuml".to_string())
        );
    }

    #[test]
    fn test_all_diagram_languages() {
        let languages = [
            "plantuml",
            "c4plantuml",
            "mermaid",
            "graphviz",
            "dot",
            "ditaa",
            "blockdiag",
            "seqdiag",
            "actdiag",
            "nwdiag",
            "packetdiag",
            "rackdiag",
            "erd",
            "nomnoml",
            "svgbob",
            "vega",
            "vegalite",
            "wavedrom",
        ];

        for lang in languages {
            let mut processor = DiagramProcessor::new();
            let attrs = HashMap::new();

            let result = processor.process(lang, &attrs, "source", 0);

            assert!(
                matches!(result, ProcessResult::Placeholder(_)),
                "Expected Placeholder for language: {}",
                lang
            );
        }
    }
}
