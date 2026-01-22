//! Code block processor trait for extensible code block handling.
//!
//! This module provides a generic mechanism for processing special code blocks
//! (diagrams, YAML tables, embeds, etc.) without coupling the renderer to
//! specific implementations.
//!
//! # Architecture
//!
//! Processors are registered with the renderer and checked in order when a code
//! block is encountered. The first processor returning a non-`PassThrough` result
//! wins.
//!
//! # Example
//!
//! ```
//! use std::collections::HashMap;
//! use docstage_renderer::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
//!
//! struct DiagramProcessor {
//!     extracted: Vec<ExtractedCodeBlock>,
//! }
//!
//! impl CodeBlockProcessor for DiagramProcessor {
//!     fn process(
//!         &mut self,
//!         language: &str,
//!         attrs: &HashMap<String, String>,
//!         source: &str,
//!         index: usize,
//!     ) -> ProcessResult {
//!         if language == "plantuml" || language == "mermaid" {
//!             self.extracted.push(ExtractedCodeBlock {
//!                 index,
//!                 language: language.to_string(),
//!                 source: source.to_string(),
//!                 attrs: attrs.clone(),
//!             });
//!             ProcessResult::Placeholder(format!("{{{{DIAGRAM_{index}}}}}"))
//!         } else {
//!             ProcessResult::PassThrough
//!         }
//!     }
//!
//!     fn extracted(&self) -> &[ExtractedCodeBlock] {
//!         &self.extracted
//!     }
//! }
//! ```

use std::collections::HashMap;

/// Result of processing a code block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessResult {
    /// Replace code block with placeholder for deferred processing.
    ///
    /// Use when processing requires external resources (HTTP calls, file I/O).
    /// The caller is responsible for replacing placeholders after rendering.
    Placeholder(String),

    /// Replace code block with inline HTML immediately.
    ///
    /// Use when processing is fast and self-contained (YAML parsing, math rendering).
    Inline(String),

    /// Pass through as regular code block with syntax highlighting.
    ///
    /// Use when the language is not handled by this processor.
    PassThrough,
}

/// Metadata extracted from code block for deferred processing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedCodeBlock {
    /// Zero-based index of this code block in the document.
    pub index: usize,
    /// Language identifier from fence (e.g., "plantuml", "table-yaml").
    pub language: String,
    /// Raw source content of the code block.
    pub source: String,
    /// Attributes parsed from fence (e.g., `format=png` → {"format": "png"}).
    pub attrs: HashMap<String, String>,
}

/// Trait for processing special code blocks.
///
/// Implementations can handle one or more code block languages, transforming
/// them into placeholders (for deferred processing) or inline HTML.
///
/// # Post-Processing
///
/// Processors that use placeholders can implement [`post_process`](Self::post_process)
/// to replace them after rendering. Call [`MarkdownRenderer::finalize`] to trigger
/// post-processing on all registered processors.
pub trait CodeBlockProcessor {
    /// Process a code block and return the result.
    ///
    /// # Arguments
    ///
    /// * `language` - Language identifier from fence info string
    /// * `attrs` - Attributes parsed from fence (key=value pairs)
    /// * `source` - Raw content of the code block
    /// * `index` - Zero-based index for placeholder generation
    ///
    /// # Returns
    ///
    /// - `ProcessResult::Placeholder` - Replace with placeholder string
    /// - `ProcessResult::Inline` - Replace with HTML string
    /// - `ProcessResult::PassThrough` - Render as normal code block
    fn process(
        &mut self,
        language: &str,
        attrs: &HashMap<String, String>,
        source: &str,
        index: usize,
    ) -> ProcessResult;

    /// Post-process rendered HTML to replace placeholders.
    ///
    /// Called by [`MarkdownRenderer::finalize`] after rendering completes.
    /// Use this to replace placeholders with actual content (e.g., rendered diagrams).
    ///
    /// Default implementation is a no-op.
    fn post_process(&mut self, _html: &mut String) {}

    /// Get all extracted code blocks after rendering.
    ///
    /// Returns blocks that were processed with `ProcessResult::Placeholder`.
    /// Default implementation returns empty slice (for inline-only processors).
    fn extracted(&self) -> &[ExtractedCodeBlock] {
        &[]
    }

    /// Get warnings generated during processing.
    ///
    /// Default implementation returns empty slice.
    fn warnings(&self) -> &[String] {
        &[]
    }
}

/// Parse fence info string into language and attributes.
///
/// Format: `language [key=value ...]`
#[must_use]
pub(crate) fn parse_fence_info(info: &str) -> (String, HashMap<String, String>) {
    let mut parts = info.split_whitespace();
    let language = parts.next().unwrap_or("").to_string();

    let mut attrs = HashMap::new();
    for part in parts {
        if let Some((key, value)) = part.split_once('=') {
            // Strip quotes if present
            let value = value.trim_matches('"').trim_matches('\'');
            attrs.insert(key.to_string(), value.to_string());
        }
    }

    (language, attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fence_info_language_only() {
        let (lang, attrs) = parse_fence_info("rust");
        assert_eq!(lang, "rust");
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_parse_fence_info_with_attrs() {
        let (lang, attrs) = parse_fence_info("plantuml format=png");
        assert_eq!(lang, "plantuml");
        assert_eq!(attrs.get("format"), Some(&"png".to_string()));
    }

    #[test]
    fn test_parse_fence_info_multiple_attrs() {
        let (lang, attrs) = parse_fence_info("diagram format=svg theme=dark");
        assert_eq!(lang, "diagram");
        assert_eq!(attrs.get("format"), Some(&"svg".to_string()));
        assert_eq!(attrs.get("theme"), Some(&"dark".to_string()));
    }

    #[test]
    fn test_parse_fence_info_quoted_values() {
        let (lang, attrs) = parse_fence_info("table-yaml caption=\"User List\"");
        assert_eq!(lang, "table-yaml");
        // Note: quotes are stripped, but value is truncated at first space within the fence
        // This is expected behavior with split_whitespace
        assert_eq!(attrs.get("caption"), Some(&"User".to_string()));
    }

    #[test]
    fn test_parse_fence_info_single_quoted() {
        let (lang, attrs) = parse_fence_info("chart title='Sales'");
        assert_eq!(lang, "chart");
        assert_eq!(attrs.get("title"), Some(&"Sales".to_string()));
    }

    #[test]
    fn test_parse_fence_info_empty() {
        let (lang, attrs) = parse_fence_info("");
        assert_eq!(lang, "");
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_parse_fence_info_whitespace_only() {
        let (lang, attrs) = parse_fence_info("   ");
        assert_eq!(lang, "");
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_process_result_variants() {
        let placeholder = ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string());
        let inline = ProcessResult::Inline("<table></table>".to_string());
        let passthrough = ProcessResult::PassThrough;

        assert_eq!(
            placeholder,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
        );
        assert_eq!(inline, ProcessResult::Inline("<table></table>".to_string()));
        assert_eq!(passthrough, ProcessResult::PassThrough);
    }

    #[test]
    fn test_extracted_code_block() {
        let block = ExtractedCodeBlock {
            index: 0,
            language: "plantuml".to_string(),
            source: "@startuml\nA -> B\n@enduml".to_string(),
            attrs: HashMap::from([("format".to_string(), "png".to_string())]),
        };

        assert_eq!(block.index, 0);
        assert_eq!(block.language, "plantuml");
        assert_eq!(block.source, "@startuml\nA -> B\n@enduml");
        assert_eq!(block.attrs.get("format"), Some(&"png".to_string()));
    }

    struct TestProcessor {
        extracted: Vec<ExtractedCodeBlock>,
        warnings: Vec<String>,
    }

    impl TestProcessor {
        fn new() -> Self {
            Self {
                extracted: Vec::new(),
                warnings: Vec::new(),
            }
        }
    }

    impl CodeBlockProcessor for TestProcessor {
        fn process(
            &mut self,
            language: &str,
            attrs: &HashMap<String, String>,
            source: &str,
            index: usize,
        ) -> ProcessResult {
            match language {
                "test-placeholder" => {
                    self.extracted.push(ExtractedCodeBlock {
                        index,
                        language: language.to_string(),
                        source: source.to_string(),
                        attrs: attrs.clone(),
                    });
                    ProcessResult::Placeholder(format!("{{{{TEST_{index}}}}}"))
                }
                "test-inline" => ProcessResult::Inline(format!("<div>{source}</div>")),
                "test-warn" => {
                    self.warnings.push("Test warning".to_string());
                    ProcessResult::PassThrough
                }
                _ => ProcessResult::PassThrough,
            }
        }

        fn extracted(&self) -> &[ExtractedCodeBlock] {
            &self.extracted
        }

        fn warnings(&self) -> &[String] {
            &self.warnings
        }
    }

    #[test]
    fn test_processor_placeholder() {
        let mut processor = TestProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("test-placeholder", &attrs, "content", 0);
        assert_eq!(result, ProcessResult::Placeholder("{{TEST_0}}".to_string()));

        let extracted = processor.extracted();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].language, "test-placeholder");
        assert_eq!(extracted[0].source, "content");
    }

    #[test]
    fn test_processor_inline() {
        let mut processor = TestProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("test-inline", &attrs, "hello", 0);
        assert_eq!(
            result,
            ProcessResult::Inline("<div>hello</div>".to_string())
        );

        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_processor_passthrough() {
        let mut processor = TestProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);
        assert_eq!(result, ProcessResult::PassThrough);
    }

    #[test]
    fn test_processor_warnings() {
        let mut processor = TestProcessor::new();
        let attrs = HashMap::new();

        processor.process("test-warn", &attrs, "", 0);
        let warnings = processor.warnings();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0], "Test warning");
    }

    #[test]
    fn test_default_trait_implementations() {
        struct MinimalProcessor;

        impl CodeBlockProcessor for MinimalProcessor {
            fn process(
                &mut self,
                _language: &str,
                _attrs: &HashMap<String, String>,
                _source: &str,
                _index: usize,
            ) -> ProcessResult {
                ProcessResult::PassThrough
            }
        }

        let processor = MinimalProcessor;
        assert!(processor.extracted().is_empty());
        assert!(processor.warnings().is_empty());
    }
}
