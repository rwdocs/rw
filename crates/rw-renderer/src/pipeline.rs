//! Per-render extensions for [`MarkdownRenderer`](crate::MarkdownRenderer):
//! code-block processors. See [`Pipeline`] for the API.
//!
//! Settings that live for the lifetime of the renderer (base path, wikilinks,
//! sections, title resolver) stay on
//! [`MarkdownRenderer`](crate::MarkdownRenderer) and are configured via
//! its builder methods.

use crate::code_block::CodeBlockProcessor;

/// Per-render extensions: code-block processors.
///
/// Construct one with [`Pipeline::new`] (or [`Pipeline::default`]),
/// register extensions via [`Pipeline::with_processor`], then pass it into
/// [`MarkdownRenderer::render`](crate::MarkdownRenderer::render). The
/// render call consumes the pipeline.
///
/// # Examples
///
/// Empty pipeline — used when no code-block processors are needed:
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
/// let result = renderer.render(":status[Done]{color=green}", Pipeline::new());
/// assert!(result.html.contains("status-green"));
/// ```
pub struct Pipeline {
    pub(crate) processors: Vec<Box<dyn CodeBlockProcessor>>,
}

impl Pipeline {
    /// Construct an empty pipeline: no code-block processors.
    #[must_use]
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
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
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_block::ProcessResult;
    use rw_parser::FenceAttrs;

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
    fn new_pipeline_has_no_processors() {
        let p = Pipeline::new();
        assert!(p.processors.is_empty());
    }

    #[test]
    fn with_processor_appends() {
        let p = Pipeline::new()
            .with_processor(DummyProcessor)
            .with_processor(DummyProcessor);
        assert_eq!(p.processors.len(), 2);
    }

    #[test]
    fn debug_format_does_not_panic() {
        let p = Pipeline::new().with_processor(DummyProcessor);
        let s = format!("{p:?}");
        assert!(s.contains("Pipeline"));
        assert!(s.contains("processors"));
    }
}
