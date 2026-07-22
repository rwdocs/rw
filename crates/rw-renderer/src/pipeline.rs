//! Per-render extensions for [`MarkdownRenderer`]: code-block processors
//! and the directive processor. See [`Pipeline`] for the API.
//!
//! Settings that live for the lifetime of the renderer (base path, wikilinks,
//! sections, title resolver) stay on
//! [`MarkdownRenderer`](crate::MarkdownRenderer) and are configured via
//! its builder methods.

use crate::code_block::CodeBlockProcessor;
use crate::directive::DirectiveProcessor;

/// Per-render extensions: code-block processors and an optional
/// [`DirectiveProcessor`].
///
/// Construct one with [`Pipeline::new`] (or [`Pipeline::default`]),
/// register extensions via [`Pipeline::with_processor`] /
/// [`Pipeline::with_directives`], then pass it into
/// [`MarkdownRenderer::render`](crate::MarkdownRenderer::render). The
/// render call consumes the pipeline.
///
/// `Pipeline` is `Send` but not `Sync`. Directive handlers
/// ([`InlineDirective`](crate::directive::InlineDirective),
/// [`LeafDirective`](crate::directive::LeafDirective),
/// [`ContainerDirective`](crate::directive::ContainerDirective)) are
/// `Send`-only by contract — each document gets its own handler — so
/// sharing a `Pipeline` across threads is intentionally not possible.
/// Build a fresh `Pipeline` per render.
///
/// # Examples
///
/// Empty pipeline — used when no directives or code-block processors are
/// needed:
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
/// let result = renderer.render("Hello.", Pipeline::new());
/// assert!(result.html.contains("Hello"));
/// ```
///
/// Pipeline with a directive processor:
///
/// ```
/// use rw_renderer::{
///     HtmlBackend, MarkdownRenderer, Pipeline, StatusDirective,
/// };
/// use rw_renderer::directive::DirectiveProcessor;
///
/// let directives = DirectiveProcessor::new().with_inline(StatusDirective::new());
/// let pipeline = Pipeline::new().with_directives(directives);
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
/// let result = renderer.render(":status[Done]{color=green}", pipeline);
/// assert!(result.html.contains("status-green"));
/// ```
pub struct Pipeline {
    pub(crate) processors: Vec<Box<dyn CodeBlockProcessor>>,
    pub(crate) directives: Option<DirectiveProcessor>,
}

impl Pipeline {
    /// Construct an empty pipeline: no code-block processors, no directive
    /// processor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
            directives: None,
        }
    }

    /// Add a code-block processor.
    ///
    /// Processors are checked in registration order; the first returning
    /// a non-[`PassThrough`](crate::ProcessResult::PassThrough) result
    /// wins for a given code block.
    #[must_use]
    pub fn with_processor<P: CodeBlockProcessor + 'static>(mut self, processor: P) -> Self {
        self.processors.push(Box::new(processor));
        self
    }

    /// Install the directive processor.
    ///
    /// At most one directive processor per pipeline; calling
    /// `with_directives` twice replaces the previous one.
    #[must_use]
    pub fn with_directives(mut self, directives: DirectiveProcessor) -> Self {
        self.directives = Some(directives);
        self
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline")
            .field(
                "processors",
                &format_args!("[<{} processors>]", self.processors.len()),
            )
            .field(
                "directives",
                &if self.directives.is_some() {
                    "Some(<DirectiveProcessor>)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_block::{FenceAttrs, ProcessResult};

    struct DummyProcessor;
    impl CodeBlockProcessor for DummyProcessor {
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

    #[test]
    fn new_pipeline_has_no_processors_or_directives() {
        let p = Pipeline::new();
        assert!(p.processors.is_empty());
        assert!(p.directives.is_none());
    }

    #[test]
    fn with_processor_appends() {
        let p = Pipeline::new()
            .with_processor(DummyProcessor)
            .with_processor(DummyProcessor);
        assert_eq!(p.processors.len(), 2);
    }

    #[test]
    fn with_directives_sets_directive_processor() {
        let p = Pipeline::new().with_directives(DirectiveProcessor::new());
        assert!(p.directives.is_some());
    }

    #[test]
    fn with_directives_replaces_existing() {
        // Verify the second processor's directive handler actually wins
        // observably — not just that `directives` is `Some` after replacement.
        use crate::HtmlBackend;
        use crate::MarkdownRenderer;
        use crate::directive::{DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective};

        struct AlphaTag;
        impl InlineDirective for AlphaTag {
            fn name(&self) -> &'static str {
                "tag"
            }
            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                DirectiveOutput::html("<ALPHA>")
            }
        }
        struct BetaTag;
        impl InlineDirective for BetaTag {
            fn name(&self) -> &'static str {
                "tag"
            }
            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                DirectiveOutput::html("<BETA>")
            }
        }

        let pipeline = Pipeline::new()
            .with_directives(DirectiveProcessor::new().with_inline(AlphaTag))
            .with_directives(DirectiveProcessor::new().with_inline(BetaTag));

        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(":tag[x]", pipeline);

        assert!(
            result.html.contains("<BETA>"),
            "second processor should win, got: {}",
            result.html
        );
        assert!(
            !result.html.contains("<ALPHA>"),
            "first processor should be replaced, got: {}",
            result.html
        );
    }

    #[test]
    fn debug_format_does_not_panic() {
        let p = Pipeline::new()
            .with_processor(DummyProcessor)
            .with_directives(DirectiveProcessor::new());
        let s = format!("{p:?}");
        assert!(s.contains("Pipeline"));
        assert!(s.contains("processors"));
        assert!(s.contains("directives"));
    }
}
