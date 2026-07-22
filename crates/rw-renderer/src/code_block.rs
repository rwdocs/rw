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
//! A processor that can't render immediately (e.g. it needs an external HTTP
//! call) returns [`ProcessResult::Deferred`] to reserve a hole, then supplies
//! the rendered markup afterwards through
//! [`fills`](CodeBlockProcessor::fills):
//!
//! ```
//! use rw_renderer::{CodeBlockProcessor, ExtractedCodeBlock, FenceAttrs, Fills, ProcessResult};
//!
//! #[derive(Default)]
//! struct DiagramProcessor {
//!     extracted: Vec<ExtractedCodeBlock>,
//!     seen: Vec<usize>,
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
//!             self.seen.push(index);
//!             ProcessResult::Deferred
//!         } else {
//!             ProcessResult::PassThrough
//!         }
//!     }
//!
//!     fn fills(&mut self, fills: &mut Fills) {
//!         // Runs after the walk, once every diagram has been rendered.
//!         for index in &self.seen {
//!             let key = u32::try_from(*index).expect("code block index exceeds hole key width");
//!             fills.set(key, format!(r#"<img src="diagram-{index}.svg">"#));
//!         }
//!     }
//!
//!     fn extracted(&self) -> &[ExtractedCodeBlock] {
//!         &self.extracted
//!     }
//! }
//! ```

use std::collections::BTreeSet;

use crate::directive::Fills;
use rw_parser::FenceAttrs;

/// Result of processing a code block.
#[derive(Debug, PartialEq, Eq)]
pub enum ProcessResult {
    /// Replace code block with inline HTML immediately.
    ///
    /// Use when processing is fast and self-contained (YAML parsing, math rendering).
    Inline(String),

    /// Pass through as regular code block with syntax highlighting.
    ///
    /// Use when the language is not handled by this processor.
    PassThrough,

    /// The block's content is not knowable during the walk. The walker reserves
    /// a hole at this position keyed by the block's index, and the processor
    /// supplies content afterwards through [`CodeBlockProcessor::fills`].
    ///
    /// Carries no payload: nothing about the final markup is known yet.
    Deferred,
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
    /// Attributes parsed from fence (e.g., `format=png` → `[("format", "png")]`).
    attrs: Vec<(String, String)>,
}

impl ExtractedCodeBlock {
    /// Create a new extracted code block.
    #[must_use]
    pub fn new(
        index: usize,
        language: String,
        source: String,
        id: Option<String>,
        attrs: Vec<(String, String)>,
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

    /// Look up an attribute value parsed from the fence info string.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Intercepts fenced code blocks during rendering.
///
/// Implementations handle one or more code block languages, transforming
/// them into deferred holes (for processing that needs external resources,
/// like Kroki HTTP calls) or inline HTML (for fast, self-contained transforms
/// like YAML tables).
///
/// Register processors with
/// [`Pipeline::with_processor`](crate::Pipeline::with_processor).
/// They are checked in registration order; the first returning a
/// non-[`PassThrough`](ProcessResult::PassThrough) result wins.
///
/// Processors that return [`ProcessResult::Deferred`] implement
/// [`fills`](Self::fills) to supply the reserved hole's content once the walk
/// completes; [`MarkdownRenderer::render`](crate::MarkdownRenderer::render)
/// calls `fills` automatically.
pub trait CodeBlockProcessor: Send + Sync {
    /// Inspects a code block and decides how to handle it.
    ///
    /// `language` is the identifier from the fence info string (e.g.,
    /// `"plantuml"`). `attrs` is the parsed `{ … }` attribute block from the
    /// remainder of the info string (e.g., `{#id format=png}`). `index` is a
    /// zero-based counter useful for keying a reserved hole (see
    /// [`ProcessResult::Deferred`]).
    fn process(
        &mut self,
        language: &str,
        attrs: &FenceAttrs,
        source: &str,
        index: usize,
    ) -> ProcessResult;

    /// Supply content for holes reserved by [`ProcessResult::Deferred`].
    ///
    /// Called once after the walk, before assembly. Keys are the code-block
    /// `index` values passed to [`process`](Self::process), narrowed to
    /// [`HoleKey`](crate::directive::HoleKey).
    ///
    /// Called on every registered processor whether or not it deferred
    /// anything.
    ///
    /// Every hole reserved by returning [`ProcessResult::Deferred`] must be
    /// filled here. A key left unset is an internal bug, not a recoverable
    /// condition: debug builds panic, release builds silently omit that content
    /// from the page, and it never surfaces through
    /// [`warnings`](Self::warnings).
    fn fills(&mut self, _fills: &mut Fills) {}

    /// Get all extracted code blocks after rendering.
    ///
    /// Returns blocks that were processed with [`ProcessResult::Deferred`].
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
    /// referenced (e.g. diagram `$link`s resolved to sections), collected in
    /// [`fills`](Self::fills). Collected by the renderer into
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HtmlBackend, MarkdownRenderer, Pipeline};

    /// Defers every `demo` block and fills each after the walk, proving the
    /// fill lands at the reserved offset rather than being scanned for.
    #[derive(Default)]
    struct DeferringProcessor {
        seen: Vec<usize>,
    }

    impl CodeBlockProcessor for DeferringProcessor {
        fn process(
            &mut self,
            language: &str,
            _attrs: &FenceAttrs,
            _source: &str,
            index: usize,
        ) -> ProcessResult {
            if language != "demo" {
                return ProcessResult::PassThrough;
            }
            self.seen.push(index);
            ProcessResult::Deferred
        }

        fn fills(&mut self, fills: &mut Fills) {
            // Runs after the walk, so the total is known — the reason to defer.
            let total = self.seen.len();
            for index in &self.seen {
                let key = u32::try_from(*index).expect("code block index exceeds hole key width");
                fills.set(key, format!("<i>{index} of {total}</i>"));
            }
        }
    }

    #[test]
    fn deferred_code_block_fills_at_its_offset() {
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "before\n\n```demo\nx\n```\n\nmiddle\n\n```demo\ny\n```\n\nafter\n",
            Pipeline::new().with_processor(DeferringProcessor::default()),
        );

        // Each fill carries a total only knowable after the walk, and lands
        // between its neighbours rather than appended or scanned into place.
        // `HtmlBackend` emits no separator between block-level tags, so the
        // expected substrings are adjacent with no newline between them.
        assert!(
            result.html.contains("<p>before</p><i>0 of 2</i>"),
            "first fill misplaced: {}",
            result.html
        );
        assert!(
            result.html.contains("<i>1 of 2</i><p>after</p>"),
            "second fill misplaced: {}",
            result.html
        );
    }

    /// Defers one fence language and fills each hole with `<tag>index</tag>`.
    ///
    /// Two of these registered on one pipeline give the walk two *distinct*
    /// deferring processor indices, which is what `Source::CodeBlock(proc_idx)`
    /// exists to keep apart.
    struct LanguageProcessor {
        language: &'static str,
        tag: &'static str,
        seen: Vec<usize>,
    }

    impl LanguageProcessor {
        fn new(language: &'static str, tag: &'static str) -> Self {
            Self {
                language,
                tag,
                seen: Vec::new(),
            }
        }
    }

    impl CodeBlockProcessor for LanguageProcessor {
        fn process(
            &mut self,
            language: &str,
            _attrs: &FenceAttrs,
            _source: &str,
            index: usize,
        ) -> ProcessResult {
            if language != self.language {
                return ProcessResult::PassThrough;
            }
            self.seen.push(index);
            ProcessResult::Deferred
        }

        fn fills(&mut self, fills: &mut Fills) {
            for index in &self.seen {
                let key = u32::try_from(*index).expect("code block index exceeds hole key width");
                fills.set(key, format!("<{0}>{index}</{0}>", self.tag));
            }
        }
    }

    /// Two deferring processors on one pipeline must each get their own hole
    /// namespace.
    ///
    /// The walker reserves under `GlobalKey(Source::CodeBlock(proc_idx), key)`
    /// and `Walker::finish` merges each processor's `Fills` under
    /// `Source::CodeBlock(idx)`; both derive the index from `enumerate()` over
    /// the same slice, so they must agree. Drop `proc_idx` from either side and
    /// the second processor's holes are looked up under the first's source:
    /// the reserved key is missing from `GlobalFills`, so its content never
    /// reaches the page (and `Holes::assemble` trips its debug assert).
    ///
    /// Note the walker derives a hole's local key from the document-wide code
    /// block index, so two processors cannot be handed the same local key by
    /// construction — the failure this pins is the namespace *mismatch* between
    /// the reservation and merge sites, not an overwrite within `GlobalFills`.
    #[test]
    fn two_deferring_processors_do_not_share_a_hole_namespace() {
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "start\n\n```alpha\na\n```\n\nmiddle\n\n```beta\nb\n```\n\nend\n",
            Pipeline::new()
                .with_processor(LanguageProcessor::new("alpha", "a"))
                .with_processor(LanguageProcessor::new("beta", "b")),
        );

        // Both processors' content lands, each at its own fence's position.
        assert!(
            result.html.contains("<p>start</p><a>0</a><p>middle</p>"),
            "first processor's fill misplaced or missing: {}",
            result.html
        );
        assert!(
            result.html.contains("<p>middle</p><b>1</b><p>end</p>"),
            "second processor's fill misplaced or missing: {}",
            result.html
        );
        // Neither processor's content appears where the other's belongs.
        assert!(
            !result.html.contains("<a>1</a>") && !result.html.contains("<b>0</b>"),
            "a processor's fill leaked into the other's hole: {}",
            result.html
        );
    }

    /// A deferred fence inside a blockquote fills inside the `<blockquote>`.
    ///
    /// Hole offsets index the walk buffer, so a fill's position depends on
    /// nothing but where the buffer stood at reservation time. Blockquotes are
    /// not `Scope`s (they write straight to `output`), which is what lets the
    /// walker assert `self.scopes.is_empty()` when it reserves.
    #[test]
    fn deferred_code_block_inside_blockquote_fills_within_it() {
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "> quoted\n>\n> ```demo\n> x\n> ```\n\nafter\n",
            Pipeline::new().with_processor(DeferringProcessor::default()),
        );

        let fill = result
            .html
            .find("<i>0 of 1</i>")
            .expect("fill missing from output");
        let close = result
            .html
            .find("</blockquote>")
            .expect("blockquote should close");
        assert!(
            fill < close,
            "fill landed outside the blockquote: {}",
            result.html
        );
    }

    /// Same for a list item: the fill lands inside the `<li>`, not after it.
    #[test]
    fn deferred_code_block_inside_list_item_fills_within_it() {
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "- item\n\n  ```demo\n  x\n  ```\n\nafter\n",
            Pipeline::new().with_processor(DeferringProcessor::default()),
        );

        let fill = result
            .html
            .find("<i>0 of 1</i>")
            .expect("fill missing from output");
        let close = result.html.find("</li>").expect("list item should close");
        assert!(
            fill < close,
            "fill landed outside the list item: {}",
            result.html
        );
    }

    /// Returning `Deferred` without implementing `fills` is the likeliest
    /// mistake at this extension point, because `fills` has a no-op default.
    /// The reserved hole is then never filled: debug builds panic, release
    /// builds silently omit the content.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "was reserved but never filled")]
    fn deferring_without_implementing_fills_panics_in_debug() {
        struct ForgetfulProcessor;

        impl CodeBlockProcessor for ForgetfulProcessor {
            fn process(
                &mut self,
                language: &str,
                _attrs: &FenceAttrs,
                _source: &str,
                _index: usize,
            ) -> ProcessResult {
                if language == "demo" {
                    // Reserves a hole, but `fills` is left at its no-op default.
                    ProcessResult::Deferred
                } else {
                    ProcessResult::PassThrough
                }
            }
        }

        let _ = MarkdownRenderer::<HtmlBackend>::new().render(
            "```demo\nx\n```\n",
            Pipeline::new().with_processor(ForgetfulProcessor),
        );
    }

    #[test]
    fn test_extracted_code_block() {
        let block = ExtractedCodeBlock::new(
            0,
            "plantuml".to_owned(),
            "@startuml\nA -> B\n@enduml".to_owned(),
            None,
            Vec::from([("format".to_owned(), "png".to_owned())]),
        );

        assert_eq!(block.index, 0);
        assert_eq!(block.language, "plantuml");
        assert_eq!(block.source, "@startuml\nA -> B\n@enduml");
        assert_eq!(block.id(), None);
        assert_eq!(block.attr("format"), Some("png"));
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
                "test-deferred" => {
                    self.extracted.push(ExtractedCodeBlock::new(
                        index,
                        language.to_owned(),
                        source.to_owned(),
                        attrs.id.clone(),
                        attrs.map.clone(),
                    ));
                    ProcessResult::Deferred
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
    fn test_processor_deferred() {
        let mut processor = TestProcessor::new();
        let attrs = FenceAttrs::default();

        let result = processor.process("test-deferred", &attrs, "content", 0);
        assert_eq!(result, ProcessResult::Deferred);

        let extracted = processor.extracted();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].language, "test-deferred");
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
