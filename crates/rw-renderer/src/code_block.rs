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
//! use rw_renderer::{CodeBlockProcessor, ExtractedCodeBlock, FenceAttrs, ProcessResult};
//!
//! struct DiagramProcessor {
//!     extracted: Vec<ExtractedCodeBlock>,
//! }
//!
//! impl CodeBlockProcessor for DiagramProcessor {
//!     fn process(
//!         &mut self,
//!         language: &str,
//!         attrs: &FenceAttrs,
//!         source: &str,
//!         index: usize,
//!     ) -> ProcessResult {
//!         if language == "plantuml" || language == "mermaid" {
//!             self.extracted.push(ExtractedCodeBlock::new(
//!                 index,
//!                 language.to_string(),
//!                 source.to_string(),
//!                 attrs.id.clone(),
//!                 attrs.map.clone(),
//!             ));
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

use std::collections::{BTreeSet, HashMap};

/// Result of processing a code block.
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
pub struct ExtractedCodeBlock {
    /// Zero-based index of this code block in the document.
    pub index: usize,
    /// Language identifier from fence (e.g., "plantuml", "table-yaml").
    pub language: String,
    /// Raw source content of the code block.
    pub source: String,
    /// Writer-set id from `{#id}`, kept as a typed field rather than an `attrs`
    /// map entry so a bare `id=foo` token can't populate it and a consumer reads
    /// it without stringly-typed lookups.
    id: Option<String>,
    /// Attributes parsed from fence (e.g., `format=png` → {"format": "png"}).
    attrs: HashMap<String, String>,
}

impl ExtractedCodeBlock {
    /// Create a new extracted code block.
    #[must_use]
    pub fn new(
        index: usize,
        language: String,
        source: String,
        id: Option<String>,
        attrs: HashMap<String, String>,
    ) -> Self {
        Self {
            index,
            language,
            source,
            id,
            attrs,
        }
    }

    /// Writer-set id from the fence `{#id}` block, if any.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Attributes parsed from the fence info string.
    #[must_use]
    pub fn attrs(&self) -> &HashMap<String, String> {
        &self.attrs
    }
}

/// Intercepts fenced code blocks during rendering.
///
/// Implementations handle one or more code block languages, transforming
/// them into placeholders (for deferred processing like Kroki HTTP calls)
/// or inline HTML (for fast, self-contained transforms like YAML tables).
///
/// Register processors with
/// [`Pipeline::with_processor`](crate::Pipeline::with_processor).
/// They are checked in registration order; the first returning a
/// non-[`PassThrough`](ProcessResult::PassThrough) result wins.
///
/// Processors that use placeholders can implement
/// [`post_process`](Self::post_process) to replace them after rendering
/// completes.
/// [`MarkdownRenderer::render`](crate::MarkdownRenderer::render) calls
/// `post_process` automatically.
pub trait CodeBlockProcessor: Send + Sync {
    /// Inspects a code block and decides how to handle it.
    ///
    /// `language` is the identifier from the fence info string (e.g.,
    /// `"plantuml"`). `attrs` is the parsed `{ … }` attribute block from the
    /// remainder of the info string (e.g., `{#id format=png}`). `index` is a
    /// zero-based counter useful for generating unique placeholder tokens.
    fn process(
        &mut self,
        language: &str,
        attrs: &FenceAttrs,
        source: &str,
        index: usize,
    ) -> ProcessResult;

    /// Post-process rendered HTML to replace placeholders.
    ///
    /// Called by [`MarkdownRenderer::render`](crate::MarkdownRenderer::render)
    /// after rendering completes.
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

    /// Whether this processor encountered a transient failure during processing
    /// (e.g. a remote render service was unreachable). A `true` value signals
    /// callers not to persist the rendered output to a durable cache, so the
    /// render is retried once the condition clears.
    ///
    /// Default implementation returns `false`.
    fn has_transient_error(&self) -> bool {
        false
    }

    /// Canonical section refs (`"kind:namespace/name"`) this processor's output
    /// referenced during `post_process` (e.g. diagram `$link`s resolved to
    /// sections). Collected by the renderer into
    /// [`RenderResult::section_refs`](crate::RenderResult::section_refs).
    ///
    /// Default implementation returns an empty set.
    fn section_refs(&self) -> &BTreeSet<String> {
        // A `static` is needed to hand out a `'static` empty set by reference;
        // unlike the `&[]` slice-literal defaults above, `&BTreeSet::new()`
        // alone would not outlive the call.
        static EMPTY: BTreeSet<String> = BTreeSet::new();
        &EMPTY
    }

    /// Bundle code block source before rendering.
    ///
    /// Called by [`bundle_markdown`](crate::bundle_markdown) to resolve
    /// external references (e.g., `PlantUML` `!include` directives).
    ///
    /// Return `Some(resolved_source)` to replace the code block content,
    /// or `None` if this processor doesn't handle the language.
    ///
    /// # Arguments
    ///
    /// * `language` - Language identifier from fence info string
    /// * `source` - Raw content of the code block
    fn bundle(&mut self, _language: &str, _source: &str) -> Option<String> {
        None
    }
}

/// Parsed fence info string: the language plus an optional `{ … }`
/// attribute block.
///
/// Only the brace block populates attributes. Outside the braces, the first
/// whitespace token is the language and every other bare token is ignored.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FenceAttrs {
    /// Explicit id from `{#id}` (last one wins). `None` when absent.
    pub id: Option<String>,
    /// Classes from `{.class}`, in source order.
    pub classes: Vec<String>,
    /// `key=value` attributes (and valueless flags, value `""`) from the block.
    pub map: HashMap<String, String>,
}

/// Parse a fence info string into its language and attribute block.
///
/// Grammar inside a single `{ … }` span: whitespace-separated tokens, each
/// classified by its first byte — `#id`, `.class`, `key=value`, or a bare flag.
/// Tokens of length ≤ 1 are ignored. This is an original implementation modeled
/// on the documented Pandoc/heading-attribute behavior; no third-party parser
/// code is reused.
#[must_use]
pub(crate) fn parse_fence_info(info: &str) -> (String, FenceAttrs) {
    let (before_brace, inner) = split_brace_block(info);
    let language = before_brace
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned();

    let mut attrs = FenceAttrs::default();
    if let Some(inner) = inner {
        parse_attr_block(inner, &mut attrs);
    }
    (language, attrs)
}

/// Split off a single `{ … }` block: the substring before the first `{`, and
/// the content between the first `{` and the *first* `}` after it. Closing on
/// the first `}` (not the last) keeps two adjacent groups like `{#a}{#b}` from
/// merging into one corrupted block; only the first group is honored.
fn split_brace_block(info: &str) -> (&str, Option<&str>) {
    if let Some(open) = info.find('{')
        && let Some(close_rel) = info[open + 1..].find('}')
    {
        let close = open + 1 + close_rel;
        return (&info[..open], Some(&info[open + 1..close]));
    }
    (info, None)
}

/// Parse the tokens inside a brace block into `attrs`, dispatching each
/// whitespace-separated token by its first byte (`#`→id, `.`→class, else
/// `key=value`). A later `#id` overwrites an earlier one (last wins); classes
/// accumulate.
fn parse_attr_block(inner: &str, attrs: &mut FenceAttrs) {
    for token in inner.split_whitespace() {
        if token.len() <= 1 {
            // Lone `#`, `.`, or a single-char token — nothing to name.
            continue;
        }
        match token.as_bytes()[0] {
            b'#' => attrs.id = Some(token[1..].to_owned()),
            b'.' => attrs.classes.push(token[1..].to_owned()),
            _ => {
                if let Some((key, value)) = token.split_once('=') {
                    if !key.is_empty() {
                        let value = value.trim_matches('"').trim_matches('\'');
                        attrs.map.insert(key.to_owned(), value.to_owned());
                    }
                } else {
                    attrs.map.insert(token.to_owned(), String::new());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language_only() {
        let (lang, attrs) = parse_fence_info("rust");
        assert_eq!(lang, "rust");
        assert_eq!(attrs, FenceAttrs::default());
    }

    #[test]
    fn parse_brace_id() {
        let (lang, attrs) = parse_fence_info("mermaid {#architecture}");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id.as_deref(), Some("architecture"));
        assert!(attrs.classes.is_empty());
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_brace_id_classes_kv() {
        let (lang, attrs) = parse_fence_info("plantuml {#a .b .c format=png k=v}");
        assert_eq!(lang, "plantuml");
        assert_eq!(attrs.id.as_deref(), Some("a"));
        assert_eq!(attrs.classes, vec!["b".to_owned(), "c".to_owned()]);
        assert_eq!(attrs.map.get("format"), Some(&"png".to_owned()));
        assert_eq!(attrs.map.get("k"), Some(&"v".to_owned()));
    }

    #[test]
    fn parse_brace_last_id_wins() {
        let (_lang, attrs) = parse_fence_info("mermaid {#a #b}");
        assert_eq!(attrs.id.as_deref(), Some("b"));
    }

    #[test]
    fn parse_bare_tokens_ignored() {
        // Outside the braces, bare id=/format= are NOT attributes.
        let (lang, attrs) = parse_fence_info("mermaid id=foo format=png");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id, None);
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_brace_format_only() {
        let (_lang, attrs) = parse_fence_info("mermaid {format=svg}");
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.map.get("format"), Some(&"svg".to_owned()));
    }

    #[test]
    fn parse_degenerate_braces_no_panic() {
        for info in ["mermaid {}", "mermaid {#}", "mermaid {#foo", "mermaid }{"] {
            let (lang, attrs) = parse_fence_info(info);
            assert_eq!(lang, "mermaid");
            assert_eq!(attrs.id, None, "info: {info}");
        }
    }

    #[test]
    fn parse_non_ascii_id_no_panic() {
        let (_lang, attrs) = parse_fence_info("mermaid {#заголовок}");
        assert_eq!(attrs.id.as_deref(), Some("заголовок"));
    }

    #[test]
    fn parse_multiple_brace_groups_takes_first() {
        // Two adjacent groups must not merge into one corrupted block: the
        // block ends at the first `}`, so only the first group is honored.
        let (lang, attrs) = parse_fence_info("mermaid {#hello}{format=png}");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id.as_deref(), Some("hello"));
        assert!(
            attrs.map.is_empty(),
            "second group must be ignored, not merged"
        );

        let (_lang, attrs) = parse_fence_info("mermaid {#a} {b=c}");
        assert_eq!(attrs.id.as_deref(), Some("a"));
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_quoted_kv_value_trimmed() {
        let (_lang, attrs) = parse_fence_info("mermaid {caption=\"User\"}");
        assert_eq!(attrs.map.get("caption"), Some(&"User".to_owned()));
    }

    #[test]
    fn test_process_result_variants() {
        let placeholder = ProcessResult::Placeholder("{{DIAGRAM_0}}".to_owned());
        let inline = ProcessResult::Inline("<table></table>".to_owned());
        let passthrough = ProcessResult::PassThrough;

        assert_eq!(
            placeholder,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_owned())
        );
        assert_eq!(inline, ProcessResult::Inline("<table></table>".to_owned()));
        assert_eq!(passthrough, ProcessResult::PassThrough);
    }

    #[test]
    fn test_extracted_code_block() {
        let block = ExtractedCodeBlock::new(
            0,
            "plantuml".to_owned(),
            "@startuml\nA -> B\n@enduml".to_owned(),
            None,
            HashMap::from([("format".to_owned(), "png".to_owned())]),
        );

        assert_eq!(block.index, 0);
        assert_eq!(block.language, "plantuml");
        assert_eq!(block.source, "@startuml\nA -> B\n@enduml");
        assert_eq!(block.id(), None);
        assert_eq!(block.attrs().get("format"), Some(&"png".to_owned()));
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
            attrs: &FenceAttrs,
            source: &str,
            index: usize,
        ) -> ProcessResult {
            match language {
                "test-placeholder" => {
                    self.extracted.push(ExtractedCodeBlock::new(
                        index,
                        language.to_owned(),
                        source.to_owned(),
                        attrs.id.clone(),
                        attrs.map.clone(),
                    ));
                    ProcessResult::Placeholder(format!("{{{{TEST_{index}}}}}"))
                }
                "test-inline" => ProcessResult::Inline(format!("<div>{source}</div>")),
                "test-warn" => {
                    self.warnings.push("Test warning".to_owned());
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
        let attrs = FenceAttrs::default();

        let result = processor.process("test-placeholder", &attrs, "content", 0);
        assert_eq!(result, ProcessResult::Placeholder("{{TEST_0}}".to_owned()));

        let extracted = processor.extracted();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].language, "test-placeholder");
        assert_eq!(extracted[0].source, "content");
    }

    #[test]
    fn test_processor_inline() {
        let mut processor = TestProcessor::new();
        let attrs = FenceAttrs::default();

        let result = processor.process("test-inline", &attrs, "hello", 0);
        assert_eq!(result, ProcessResult::Inline("<div>hello</div>".to_owned()));

        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_processor_passthrough() {
        let mut processor = TestProcessor::new();
        let attrs = FenceAttrs::default();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);
        assert_eq!(result, ProcessResult::PassThrough);
    }

    #[test]
    fn test_processor_warnings() {
        let mut processor = TestProcessor::new();
        let attrs = FenceAttrs::default();

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
                _attrs: &FenceAttrs,
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
