//! Directive processor for `CommonMark` directives.
//!
//! Registries for inline/leaf/container handlers, dispatched during the render
//! walk, plus collection of the deferred content that fills their reserved
//! holes once the walk completes.

use std::io;
use std::path::{Path, PathBuf};

use super::fills::{GlobalFills, Source};
use super::parser::ParsedDirective;
use super::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Fills, InlineDirective,
    LeafDirective, Marker, Part,
};

/// Type alias for the file reading callback function.
pub type ReadFileFn = dyn Fn(&Path) -> io::Result<String> + Send;

/// Result of dispatching a parsed block directive (leaf or container) for the
/// walker to render. Distinct from [`DirectiveOutput`] because it adds a
/// `PassThrough` variant carrying the byte-exact literal source an unhandled
/// directive reconstructs to, so an unrecognized or declined directive renders
/// as its original text rather than disappearing.
#[derive(Debug)]
pub(crate) enum BlockDispatch {
    /// Emit verbatim via the backend's `raw_html`. An empty string emits nothing
    /// (e.g. a container `end()` that returns `None`).
    Html(String),
    /// A semantic marker — `marker_open + text(body) + marker_close`.
    Marker { marker: Marker, body: String },
    /// Literal HTML interleaved with holes. See [`DirectiveOutput::Deferred`].
    ///
    /// `source` identifies the handler that produced the parts: its hole keys
    /// are handler-local, and the walker pairs each with this source to get the
    /// global key it records.
    Deferred { parts: Vec<Part>, source: Source },
    /// Literal text the walker renders as an ordinary paragraph (`<p>…</p>`).
    PassThrough(String),
}

/// One entry per open container scope, recording how the matching closing
/// `:::` should be rendered. Pushed for every `:::name` opener the user must
/// close with its own `:::` — including unregistered or `Skip`-ing openers, so
/// their close does not pop an enclosing registered container.
///
/// Every frame carries the walker's enclosing block-nesting depth (blockquote
/// and list levels) at the moment the opener was seen. A container left open
/// must be balanced when *that* block ends, not at end of input — see
/// [`DirectiveProcessor::close_containers_nested_in`].
enum ContainerFrame {
    /// A registered handler opened a scope; call `container_handlers[idx].end()`
    /// when the closing `:::` is reached.
    Handled { idx: usize, depth: usize },
    /// The opening delimiter rendered literally (unregistered name, or the
    /// handler returned `Skip`); render the closing `:::` literally too.
    Literal { depth: usize },
}

impl ContainerFrame {
    /// Enclosing block-nesting depth at the opener.
    fn depth(&self) -> usize {
        match *self {
            Self::Handled { depth, .. } | Self::Literal { depth } => depth,
        }
    }
}

/// Configuration for the directive processor.
pub struct DirectiveProcessorConfig {
    /// Base directory for resolving relative paths (e.g., for `::include`).
    pub base_dir: PathBuf,
    /// Path to the source file being rendered (if known).
    pub source_path: Option<PathBuf>,
    /// Callback to read files from the file system.
    ///
    /// Default: `std::fs::read_to_string`
    pub read_file: Option<Box<ReadFileFn>>,
}

impl Default for DirectiveProcessorConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectiveProcessorConfig {
    /// Create a new configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            base_dir: PathBuf::from("."),
            source_path: None,
            read_file: None,
        }
    }

    /// Set the base directory for resolving relative paths.
    #[must_use]
    pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
        self.base_dir = base_dir.into();
        self
    }

    /// Set the source file path.
    #[must_use]
    pub fn with_source_path(mut self, source_path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    /// Set the file reading callback.
    #[must_use]
    pub fn with_read_file<F>(mut self, read_file: F) -> Self
    where
        F: Fn(&Path) -> io::Result<String> + Send + 'static,
    {
        self.read_file = Some(Box::new(read_file));
        self
    }

    fn create_context(&self, line: usize) -> DirectiveContext<'_> {
        DirectiveContext {
            source_path: self.source_path.as_deref(),
            base_dir: &self.base_dir,
            line,
            read_file: self.read_file.as_ref().map_or_else(
                || &default_read_file as &dyn Fn(&Path) -> io::Result<String>,
                |f| f.as_ref(),
            ),
        }
    }
}

/// Default file reading function.
fn default_read_file(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Processor for `CommonMark` directives.
///
/// Dispatches directive handlers during the render walk and collects the
/// content filling their deferred holes once the walk completes.
///
/// # Example
///
/// Register handlers, then drive them through
/// [`MarkdownRenderer::render`](crate::MarkdownRenderer::render): every
/// directive kind — leaf, container, and inline `:name[…]` — is recognized as
/// the markdown is tokenized and dispatched here, while inline code spans, code
/// blocks, and raw HTML pass through unchanged.
///
/// ```
/// use rw_renderer::directive::{
///     DirectiveProcessor, DirectiveArgs, DirectiveContext, DirectiveOutput, LeafDirective,
/// };
///
/// struct YouTube;
///
/// impl LeafDirective for YouTube {
///     fn name(&self) -> &str { "youtube" }
///     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         DirectiveOutput::html(format!(r#"<iframe src="https://youtu.be/{}"></iframe>"#, args.content()))
///     }
/// }
///
/// // Block directives expand during `MarkdownRenderer::render`.
/// let processor = DirectiveProcessor::new()
///     .with_leaf(YouTube);
/// ```
pub struct DirectiveProcessor {
    config: DirectiveProcessorConfig,
    inline_handlers: Vec<Box<dyn InlineDirective>>,
    leaf_handlers: Vec<Box<dyn LeafDirective>>,
    container_handlers: Vec<Box<dyn ContainerDirective>>,
    /// Stack of open container scopes (one [`ContainerFrame`] per textual
    /// `:::name` … `:::` nesting level) used to pair closing delimiters.
    active_containers: Vec<ContainerFrame>,
    /// Handler indices of containers already balanced early, when their
    /// enclosing blockquote or list item ended. They are off
    /// `active_containers` (so they are neither closed nor popped twice) but
    /// [`finalize`](DirectiveProcessor::finalize) still owes each one warning.
    closed_at_block_end: Vec<usize>,
    warnings: Vec<String>,
}

impl Default for DirectiveProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectiveProcessor {
    /// Create a new directive processor with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(DirectiveProcessorConfig::default())
    }

    /// Create a new directive processor with custom configuration.
    #[must_use]
    pub fn with_config(config: DirectiveProcessorConfig) -> Self {
        Self {
            config,
            inline_handlers: Vec::new(),
            leaf_handlers: Vec::new(),
            container_handlers: Vec::new(),
            active_containers: Vec::new(),
            closed_at_block_end: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Register an inline directive handler.
    ///
    /// Dispatch picks the *first* handler whose `name()` matches, so
    /// registering two handlers under the same name shadows the second
    /// silently. A warning is recorded if that happens (visible via
    /// [`warnings`](Self::warnings)).
    #[must_use]
    pub fn with_inline<D: InlineDirective + 'static>(mut self, handler: D) -> Self {
        let name = handler.name().to_owned();
        if self.inline_handlers.iter().any(|h| h.name() == name) {
            self.warnings.push(format!(
                "inline directive ':{name}' is registered more than once; only the first handler will be dispatched"
            ));
        }
        self.inline_handlers.push(Box::new(handler));
        self
    }

    /// Register a leaf directive handler.
    ///
    /// Dispatch picks the *first* handler whose `name()` matches; a duplicate
    /// registration records a warning rather than overriding the original.
    #[must_use]
    pub fn with_leaf<D: LeafDirective + 'static>(mut self, handler: D) -> Self {
        let name = handler.name().to_owned();
        if self.leaf_handlers.iter().any(|h| h.name() == name) {
            self.warnings.push(format!(
                "leaf directive '::{name}' is registered more than once; only the first handler will be dispatched"
            ));
        }
        self.leaf_handlers.push(Box::new(handler));
        self
    }

    /// Register a container directive handler.
    ///
    /// Dispatch picks the *first* handler whose `name()` matches; a duplicate
    /// registration records a warning rather than overriding the original.
    #[must_use]
    pub fn with_container<D: ContainerDirective + 'static>(mut self, handler: D) -> Self {
        let name = handler.name().to_owned();
        if self.container_handlers.iter().any(|h| h.name() == name) {
            self.warnings.push(format!(
                "container directive ':::{name}' is registered more than once; only the first handler will be dispatched"
            ));
        }
        self.container_handlers.push(Box::new(handler));
        self
    }

    /// Dispatch a parsed block directive (leaf or container): invoke the
    /// registered handler, perform the `active_containers` push/pop and warning
    /// bookkeeping, and return owned [`BlockDispatch`] data for the walker to
    /// render. `ctx.line()` is always `0` — block directives carry no line
    /// number (no shipped handler reads it).
    ///
    /// `depth` is the walker's enclosing block nesting (blockquote and list
    /// levels) at this directive. Container frames remember it so an unclosed
    /// container is balanced when its enclosing block ends — see
    /// [`close_containers_nested_in`](Self::close_containers_nested_in).
    pub(crate) fn dispatch_block(
        &mut self,
        directive: ParsedDirective,
        depth: usize,
    ) -> BlockDispatch {
        match directive {
            ParsedDirective::ContainerStart { name, args, .. } => {
                let Some(idx) = self
                    .container_handlers
                    .iter()
                    .position(|h| h.name() == name)
                else {
                    // Unregistered: render the opener literally and track the
                    // scope so its closing ::: renders literally too, rather
                    // than closing an enclosing registered container.
                    self.active_containers
                        .push(ContainerFrame::Literal { depth });
                    return BlockDispatch::PassThrough(format!(":::{name}{}", args.to_syntax()));
                };
                let syntax = args.to_syntax();
                let ctx = self.config.create_context(0);
                let output = self.container_handlers[idx].start(args, &ctx);
                // Read before the match: do NOT move this into the arms or after
                // any other handler call — it reflects only the latest start().
                let opened = self.container_handlers[idx].opened_scope();
                // Every arm but `Skip` renders through the handler, so an
                // opened scope is the handler's to close. `Skip` is the
                // exception: it pushes a `Literal` frame of its own below.
                if opened && !matches!(output, DirectiveOutput::Skip) {
                    self.active_containers
                        .push(ContainerFrame::Handled { idx, depth });
                }
                match output {
                    DirectiveOutput::Html(html) => BlockDispatch::Html(html),
                    DirectiveOutput::Marker { marker, body } => {
                        BlockDispatch::Marker { marker, body }
                    }
                    DirectiveOutput::Deferred(parts) => BlockDispatch::Deferred {
                        parts,
                        source: Source::Container(idx),
                    },
                    DirectiveOutput::Skip => {
                        // Handler declined: the opener renders literally, so
                        // track a Literal scope for its matching close.
                        self.active_containers
                            .push(ContainerFrame::Literal { depth });
                        BlockDispatch::PassThrough(format!(":::{name}{syntax}"))
                    }
                }
            }
            ParsedDirective::ContainerEnd { colon_count } => match self.active_containers.pop() {
                Some(ContainerFrame::Handled { idx, .. }) => {
                    let html = self.container_handlers[idx].end(0).unwrap_or_default();
                    BlockDispatch::Html(html)
                }
                Some(ContainerFrame::Literal { .. }) => {
                    // Matching close for an unhandled opener — render literally.
                    BlockDispatch::PassThrough(":".repeat(colon_count))
                }
                None => {
                    self.warnings
                        .push("stray ::: with no opening directive".to_owned());
                    BlockDispatch::PassThrough(":".repeat(colon_count))
                }
            },
            ParsedDirective::Leaf { name, args } => {
                let Some(idx) = self.leaf_handlers.iter().position(|h| h.name() == name) else {
                    return BlockDispatch::PassThrough(format!("::{name}{}", args.to_syntax()));
                };
                let syntax = args.to_syntax();
                let ctx = self.config.create_context(0);
                match self.leaf_handlers[idx].process(args, &ctx) {
                    DirectiveOutput::Html(html) => BlockDispatch::Html(html),
                    DirectiveOutput::Marker { marker, body } => {
                        BlockDispatch::Marker { marker, body }
                    }
                    DirectiveOutput::Deferred(parts) => BlockDispatch::Deferred {
                        parts,
                        source: Source::Leaf(idx),
                    },
                    DirectiveOutput::Skip => {
                        BlockDispatch::PassThrough(format!("::{name}{syntax}"))
                    }
                }
            }
            ParsedDirective::Inline { .. } => {
                unreachable!("dispatch_block only handles block (leaf/container) directives")
            }
        }
    }

    /// Dispatch an inline directive by name.
    ///
    /// Returns [`DirectiveOutput::Skip`] when no handler is registered for
    /// `name`. Called by [`MarkdownRenderer`](crate::MarkdownRenderer) when an
    /// inline-directive event reaches the walk.
    ///
    /// Line number is currently not threaded through; `DirectiveContext::line`
    /// returns `0` for inline-directive calls. No existing inline handler
    /// consults it.
    pub(crate) fn dispatch_inline_named(
        &mut self,
        name: &str,
        args: DirectiveArgs,
    ) -> DirectiveOutput {
        let Some(idx) = self.inline_handlers.iter().position(|h| h.name() == name) else {
            return DirectiveOutput::Skip;
        };
        let ctx = self.config.create_context(0);
        self.inline_handlers[idx].process(args, &ctx)
    }

    /// Emit the closing markup for every container still open at end of input,
    /// appending it to `out`.
    ///
    /// A container whose closing `:::` is missing never reaches `end()` through
    /// [`dispatch_block`](Self::dispatch_block), so without this its opening
    /// tags would be left dangling and the rest of the document would nest
    /// inside them — for tabs, inside a `hidden` panel, making it invisible.
    /// Innermost scope first, so the emitted tags close in the right order.
    ///
    /// Frames are read, not drained: [`finalize`](Self::finalize) still needs
    /// them to report one "unclosed container directive" warning per frame.
    /// Only [`ContainerFrame::Handled`] frames have a handler to close; a
    /// `Literal` opener rendered as plain text and has no markup to balance.
    ///
    /// Must run before hole assembly: appending only extends the walk buffer,
    /// so every recorded hole offset stays valid.
    ///
    /// Closing tags are markup, so they reach `out` through `write_html` — the
    /// backend's `raw_html` — exactly as an in-walk `end()` would.
    pub(crate) fn close_unclosed_containers(
        &mut self,
        out: &mut String,
        write_html: impl Fn(&str, &mut String),
    ) {
        // Collect indices first: calling `end()` needs `&mut self`, which would
        // conflict with a live borrow of `self.active_containers`.
        let open: Vec<usize> = self
            .active_containers
            .iter()
            .rev()
            .filter_map(|frame| match frame {
                ContainerFrame::Handled { idx, .. } => Some(*idx),
                ContainerFrame::Literal { .. } => None,
            })
            .collect();

        for idx in open {
            if let Some(html) = self.container_handlers[idx].end(0) {
                write_html(&html, out);
            }
        }
    }

    /// Emit the closing markup for every container opened at block-nesting
    /// depth `depth` or deeper, appending it to `out`.
    ///
    /// Called by the walker as a blockquote or list item is about to close,
    /// *before* the enclosing `</blockquote>` / `</li>` is written. A container
    /// left open inside such a block cannot wait for end of input: by then its
    /// parent's closing tag has already been emitted, and the container's own
    /// closing tags would land outside it, crossing the nesting.
    ///
    /// `active_containers` is a stack, so frames opened deeper than the block
    /// that is ending are exactly the topmost ones — popping until the depth
    /// test fails visits them innermost-first, the order their tags must close
    /// in.
    ///
    /// Unlike [`close_unclosed_containers`](Self::close_unclosed_containers),
    /// frames are *drained*: a closed handler must not be closed again at end of
    /// input, nor be popped by a later stray `:::`. Their warnings are still
    /// owed, so `Handled` frames move to `closed_at_block_end` for
    /// [`finalize`](Self::finalize) to report.
    ///
    /// Closing markup is appended at the walker's current write position, so it
    /// only ever extends the walk buffer — no hole offset recorded earlier
    /// moves.
    pub(crate) fn close_containers_nested_in(
        &mut self,
        depth: usize,
        out: &mut String,
        write_html: impl Fn(&str, &mut String),
    ) {
        while self
            .active_containers
            .last()
            .is_some_and(|frame| frame.depth() >= depth)
        {
            let frame = self
                .active_containers
                .pop()
                .expect("checked above: the stack has a frame at or below `depth`");
            // Literal frames rendered their opener as plain text: nothing to
            // balance, and finalize owes them no warning either.
            if let ContainerFrame::Handled { idx, .. } = frame {
                if let Some(html) = self.container_handlers[idx].end(0) {
                    write_html(&html, out);
                }
                self.closed_at_block_end.push(idx);
            }
        }
    }

    pub(crate) fn finalize(&mut self) {
        // mem::take avoids borrowing self.active_containers while reading
        // self.container_handlers / pushing to self.warnings below.
        let frames = std::mem::take(&mut self.active_containers);
        let closed_early = std::mem::take(&mut self.closed_at_block_end);
        let handled = frames
            .into_iter()
            .filter_map(|frame| match frame {
                // Literal frames: the opener rendered as plain text, so there is
                // no managed container to warn about.
                ContainerFrame::Handled { idx, .. } => Some(idx),
                ContainerFrame::Literal { .. } => None,
            })
            // Containers balanced early at a blockquote/list-item boundary are
            // off the stack but just as unclosed: they still owe one warning.
            .chain(closed_early);
        for idx in handled {
            let name = self.container_handlers[idx].name().to_owned();
            self.warnings.push(format!(
                "unclosed container directive :::{name} (missing closing :::)"
            ));
        }
    }

    /// Collect hole content from every leaf and container handler.
    ///
    /// Inline handlers are absent by design: they emit semantic markers the
    /// backend renders during the walk, so they never defer.
    ///
    /// Each handler fills a fresh [`Fills`] under its own local keys, which are
    /// then merged under the handler's `Source` — the same one paired with its
    /// `Part::Hole` keys at dispatch — so handlers keep choosing simple local
    /// keys without risk of overwriting each other. Both directions of that
    /// pairing live here.
    pub(crate) fn collect_fills(&mut self) -> GlobalFills {
        let mut collected = GlobalFills::default();
        for (idx, handler) in self.leaf_handlers.iter_mut().enumerate() {
            let mut fills = Fills::new();
            handler.fills(&mut fills);
            collected.merge(Source::Leaf(idx), fills);
        }
        for (idx, handler) in self.container_handlers.iter_mut().enumerate() {
            let mut fills = Fills::new();
            handler.fills(&mut fills);
            collected.merge(Source::Container(idx), fills);
        }
        collected
    }

    /// Record a warning. Called by the walker when it dispatches an inline
    /// directive it can't fully honor: an unregistered name, or a handler
    /// returning `DirectiveOutput::Deferred`, whose holes it cannot fill.
    pub(crate) fn push_warning(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Get all warnings generated during processing.
    ///
    /// Includes warnings from the processor itself and from all handlers.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let mut all_warnings = self.warnings.clone();

        for handler in &self.leaf_handlers {
            all_warnings.extend(handler.warnings().iter().cloned());
        }
        for handler in &self.container_handlers {
            all_warnings.extend(handler.warnings().iter().cloned());
        }

        all_warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::DirectiveArgs;
    use crate::{HtmlBackend, MarkdownRenderer, Pipeline};

    // Test inline directive
    struct TestKbd;

    impl InlineDirective for TestKbd {
        fn name(&self) -> &'static str {
            "kbd"
        }

        fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
        }
    }

    // Test leaf directive
    struct TestYoutube;

    impl LeafDirective for TestYoutube {
        fn name(&self) -> &'static str {
            "youtube"
        }

        fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            DirectiveOutput::html(format!(
                r#"<iframe src="https://www.youtube.com/embed/{}"></iframe>"#,
                args.content()
            ))
        }
    }

    // Test container directive
    struct TestNote;

    impl ContainerDirective for TestNote {
        fn name(&self) -> &'static str {
            "note"
        }

        fn start(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            let title = if args.content().is_empty() {
                "Note".to_owned()
            } else {
                args.content().to_owned()
            };
            DirectiveOutput::html(format!(r#"<div class="note" data-title="{title}">"#))
        }

        fn end(&mut self, _line: usize) -> Option<String> {
            Some("</div>".to_owned())
        }
    }

    #[test]
    fn test_inline_directive() {
        // Inline directives are split out by the parser and dispatched by the
        // walker, not by `process`. Drive the full `MarkdownRenderer` pipeline
        // so the wiring runs end-to-end.

        let processor = DirectiveProcessor::new().with_inline(TestKbd);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "Press :kbd[Ctrl+C] to copy.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<kbd>Ctrl+C</kbd>"),
            "got: {}",
            result.html,
        );
    }

    #[test]
    fn test_inline_directive_unknown_marker_degrades_to_body_text() {
        // A backend that doesn't recognize a marker name must write nothing for
        // it, rendering the label unstyled rather than leaking markup it can't
        // translate.
        struct MarkerDirective;

        impl InlineDirective for MarkerDirective {
            fn name(&self) -> &'static str {
                "marker"
            }

            fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::marker(Marker::new("unrecognized"), args.content())
            }
        }

        let processor = DirectiveProcessor::new().with_inline(MarkerDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(":marker[label]", Pipeline::new().with_directives(processor));

        assert!(result.html.contains("label"), "got: {}", result.html);
        assert!(
            !result.html.contains("unrecognized"),
            "marker name leaked into output: {}",
            result.html,
        );
    }

    #[test]
    fn test_multiple_inline_directives() {
        let processor = DirectiveProcessor::new().with_inline(TestKbd);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "Press :kbd[Ctrl+C] then :kbd[Ctrl+V].",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<kbd>Ctrl+C</kbd>"),
            "got: {}",
            result.html,
        );
        assert!(
            result.html.contains("<kbd>Ctrl+V</kbd>"),
            "got: {}",
            result.html,
        );
    }

    #[test]
    fn test_code_fence_skipping() {
        // End-to-end: a fenced code block should preserve inline directive
        // syntax literally, while the same directive on a regular paragraph
        // line should expand. A fence's body is accumulated by the parser and
        // never scanned for directive syntax; `process` does not touch inline
        // syntax at all.

        let processor = DirectiveProcessor::new().with_inline(TestKbd);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "```\n:kbd[inside fence]\n```\n\n:kbd[outside]",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains(":kbd[inside fence]"),
            "directive inside fence should stay literal; got: {}",
            result.html,
        );
        assert!(
            result.html.contains("<kbd>outside</kbd>"),
            "directive outside fence should expand; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_config_builder() {
        let config = DirectiveProcessorConfig::new()
            .with_base_dir("/docs")
            .with_source_path("/docs/guide.md");

        assert_eq!(config.base_dir, PathBuf::from("/docs"));
        assert_eq!(config.source_path, Some(PathBuf::from("/docs/guide.md")));
    }

    #[test]
    fn inline_directive_after_leaf_token_in_paragraph_still_expands() {
        // Regression guard: a `::leaf` token mid-line must not stop the
        // scanner from finding a later `:inline` directive on the same line.
        // Driven through the full pipeline because the scan happens in the
        // parser, not in `process`.

        let processor = DirectiveProcessor::new().with_inline(TestKbd);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "Press ::foo[x] then :kbd[Ctrl+C].",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<kbd>Ctrl+C</kbd>"),
            "inline directive after a `::` token should still expand. got: {}",
            result.html,
        );
        // The mid-line `::foo[x]` is literal text — no leaf expansion mid-paragraph
        assert!(result.html.contains("::foo[x]"), "got: {}", result.html);
    }

    #[test]
    fn dispatch_block_container_start_and_end() {
        use crate::directive::parser::parse_container_line;

        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        let start = parse_container_line(":::note[Important]").unwrap();
        match processor.dispatch_block(start, 0) {
            BlockDispatch::Html(html) => {
                assert!(html.contains(r#"<div class="note" data-title="Important">"#));
            }
            other => panic!("expected Html, got {other:?}"),
        }

        let end = parse_container_line(":::").unwrap();
        match processor.dispatch_block(end, 0) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected Html, got {other:?}"),
        }
    }

    #[test]
    fn unregistered_container_nested_in_registered_does_not_corrupt_stack() {
        use crate::directive::parser::parse_container_line;
        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        // Outer registered container opens its scope.
        match processor.dispatch_block(parse_container_line(":::note[Important]").unwrap(), 0) {
            BlockDispatch::Html(html) => assert!(html.contains(r#"data-title="Important""#)),
            other => panic!("expected Html, got {other:?}"),
        }
        // Inner UNREGISTERED container: rendered literally, tracked separately.
        match processor.dispatch_block(parse_container_line(":::unknown").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert!(s.starts_with(":::unknown"), "got {s}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        // First close pairs with the inner unregistered opener -> literal,
        // it must NOT close the outer note.
        match processor.dispatch_block(parse_container_line(":::").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::"),
            other => panic!("expected literal PassThrough for inner close, got {other:?}"),
        }
        // Second close pairs with the outer note -> note.end().
        match processor.dispatch_block(parse_container_line(":::").unwrap(), 0) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected note end() Html, got {other:?}"),
        }
        processor.finalize();
        assert!(
            processor.warnings().is_empty(),
            "no warnings expected, got: {:?}",
            processor.warnings()
        );
    }

    #[test]
    fn dispatch_block_unregistered_container_pair_passthrough() {
        use crate::directive::parser::parse_container_line;
        let mut processor = DirectiveProcessor::new();

        // Unregistered opener: rendered literally, scope tracked so its close pairs with it.
        match processor.dispatch_block(parse_container_line(":::foo[x]{.c}").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => {
                assert!(s.starts_with(":::foo[x]"), "got {s}");
                assert!(s.contains(".c"), "got {s}");
            }
            other => panic!("expected PassThrough, got {other:?}"),
        }

        // Its matching close renders literally and does NOT warn about a stray.
        match processor.dispatch_block(parse_container_line(":::").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        assert!(
            !processor.warnings().iter().any(|w| w.contains("stray")),
            "unregistered open/close pair must not warn: {:?}",
            processor.warnings()
        );
    }

    #[test]
    fn dispatch_block_genuine_stray_close_warns() {
        use crate::directive::parser::parse_container_line;
        let mut processor = DirectiveProcessor::new();

        // A close with no opener on the stack is a genuine stray.
        match processor.dispatch_block(parse_container_line("::::").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, "::::"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        assert!(processor.warnings().iter().any(|w| w.contains("stray")));
    }

    #[test]
    fn dispatch_block_leaf_html_and_unregistered() {
        let mut processor = DirectiveProcessor::new().with_leaf(TestYoutube);

        let leaf = crate::directive::parser::parse_leaf_line("::youtube[abc]").unwrap();
        match processor.dispatch_block(leaf, 0) {
            BlockDispatch::Html(html) => assert!(html.contains("abc")),
            other => panic!("expected Html, got {other:?}"),
        }

        let unreg = crate::directive::parser::parse_leaf_line("::missing[y]").unwrap();
        match processor.dispatch_block(unreg, 0) {
            BlockDispatch::PassThrough(s) => assert!(s.starts_with("::missing[y]"), "got {s}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_block_marker() {
        // A leaf whose process() returns a Marker triple must surface as
        // BlockDispatch::Marker with all three fields intact.
        struct MarkerLeaf;

        impl LeafDirective for MarkerLeaf {
            fn name(&self) -> &'static str {
                "marker"
            }

            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                DirectiveOutput::Marker {
                    marker: Marker::new("marker").with_attr("flavor", "leaf"),
                    body: "the body".to_owned(),
                }
            }
        }

        let mut processor = DirectiveProcessor::new().with_leaf(MarkerLeaf);

        let leaf = crate::directive::parser::parse_leaf_line("::marker[x]").unwrap();
        match processor.dispatch_block(leaf, 0) {
            BlockDispatch::Marker { marker, body } => {
                assert_eq!(marker.name, "marker");
                assert_eq!(marker.attr("flavor"), Some("leaf"));
                assert_eq!(body, "the body");
            }
            other => panic!("expected Marker, got {other:?}"),
        }
    }

    #[test]
    fn test_block_marker_renders_through_the_walker() {
        // Covers the walker's BlockDispatch::Marker arm end-to-end: a leaf
        // directive returning a Marker must reach the backend as
        // marker_open + text(body) + marker_close, in that order. Inspecting
        // the BlockDispatch value alone would not catch a swapped open/close
        // or a dropped body.
        struct StatusLeaf;

        impl LeafDirective for StatusLeaf {
            fn name(&self) -> &'static str {
                "badge"
            }

            fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::marker(
                    Marker::new(crate::STATUS_MARKER).with_attr("color", "blue"),
                    args.content(),
                )
            }
        }

        let processor = DirectiveProcessor::new().with_leaf(StatusLeaf);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "::badge[Shipped]",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result
                .html
                .contains(r#"<span class="status status-blue">Shipped</span>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_container_marker_renders_through_the_walker() {
        // Covers the container arm of dispatch_block that returns
        // DirectiveOutput::Marker.
        struct StatusContainer;

        impl ContainerDirective for StatusContainer {
            fn name(&self) -> &'static str {
                "banner"
            }

            fn start(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::marker(
                    Marker::new(crate::STATUS_MARKER).with_attr("color", "red"),
                    args.content(),
                )
            }

            fn opened_scope(&self) -> bool {
                true
            }

            fn end(&mut self, _depth: usize) -> Option<String> {
                Some(String::new())
            }
        }

        let processor = DirectiveProcessor::new().with_container(StatusContainer);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            ":::banner[Outage]\n\ntext\n\n:::",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result
                .html
                .contains(r#"<span class="status status-red">Outage</span>"#),
            "got: {}",
            result.html
        );
        // The push onto `active_containers` sits outside the `match output`, so
        // it must fire for a non-`Html` start too. If it did not, the closing
        // `:::` would pop an empty stack: a "stray :::" warning plus a literal
        // `:::` in the output. The marker span alone would still render.
        assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
        assert!(
            !result.html.contains(":::"),
            "closing ::: leaked: {}",
            result.html
        );
    }

    // Continuation-style container: the first `:::mock` opens a new scope; each
    // subsequent `:::mock` only continues the already-open scope (reporting
    // `opened_scope() == false`), so a single `:::` closes the whole thing.
    // Mirrors how `TabsDirective` shares one closing `:::` across many `:::tab`.
    struct MockContinuation {
        depth: usize,
        last_start_opened: bool,
    }

    impl MockContinuation {
        fn new() -> Self {
            Self {
                depth: 0,
                last_start_opened: false,
            }
        }
    }

    impl ContainerDirective for MockContinuation {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn start(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            if self.depth == 0 {
                self.depth = 1;
                self.last_start_opened = true;
                DirectiveOutput::html("<mock>")
            } else {
                self.last_start_opened = false;
                DirectiveOutput::html("<mock-continue>")
            }
        }

        fn end(&mut self, _line: usize) -> Option<String> {
            self.depth = 0;
            Some("</mock>".to_owned())
        }

        fn opened_scope(&self) -> bool {
            self.last_start_opened
        }
    }

    #[test]
    fn continuation_container_closed_emits_no_unclosed_warning() {
        let mut processor = DirectiveProcessor::new().with_container(MockContinuation::new());

        let open_a = crate::directive::parser::parse_container_line(":::mock[A]").unwrap();
        let _ = processor.dispatch_block(open_a, 0);
        let open_b = crate::directive::parser::parse_container_line(":::mock[B]").unwrap();
        let _ = processor.dispatch_block(open_b, 0);
        let end = crate::directive::parser::parse_container_line(":::").unwrap();
        let _ = processor.dispatch_block(end, 0);

        processor.finalize();

        assert!(
            !processor.warnings().iter().any(|w| w.contains("unclosed")),
            "got: {:?}",
            processor.warnings(),
        );
    }

    #[test]
    fn continuation_container_unclosed_emits_one_unclosed_warning() {
        let mut processor = DirectiveProcessor::new().with_container(MockContinuation::new());

        let open_a = crate::directive::parser::parse_container_line(":::mock[A]").unwrap();
        let _ = processor.dispatch_block(open_a, 0);
        // Second opener CONTINUES the scope (opened_scope() == false). Without
        // the fix this would push a second entry and finalize() would warn
        // twice; the single genuinely-open scope must yield exactly one warning.
        let open_b = crate::directive::parser::parse_container_line(":::mock[B]").unwrap();
        let _ = processor.dispatch_block(open_b, 0);

        processor.finalize();

        let unclosed: Vec<_> = processor
            .warnings()
            .into_iter()
            .filter(|w| w.contains("unclosed"))
            .collect();
        assert_eq!(unclosed.len(), 1, "got: {unclosed:?}");
    }

    #[test]
    fn skip_container_nested_in_registered_does_not_corrupt_stack() {
        use crate::directive::parser::parse_container_line;

        struct SkipContainer;
        impl ContainerDirective for SkipContainer {
            fn name(&self) -> &'static str {
                "skipme"
            }
            fn start(&mut self, _a: DirectiveArgs, _c: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::Skip
            }
            fn end(&mut self, _line: usize) -> Option<String> {
                Some("SHOULD-NOT-APPEAR".to_owned())
            }
        }

        let mut processor = DirectiveProcessor::new()
            .with_container(TestNote)
            .with_container(SkipContainer);

        let _ = processor.dispatch_block(parse_container_line(":::note[T]").unwrap(), 0);
        match processor.dispatch_block(parse_container_line(":::skipme").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert!(s.starts_with(":::skipme"), "got {s}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        match processor.dispatch_block(parse_container_line(":::").unwrap(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::"),
            other => panic!("expected literal PassThrough, got {other:?}"),
        }
        match processor.dispatch_block(parse_container_line(":::").unwrap(), 0) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected note end(), got {other:?}"),
        }
    }

    #[test]
    fn unclosed_unregistered_container_emits_no_warning() {
        use crate::directive::parser::parse_container_line;
        let mut processor = DirectiveProcessor::new();
        let _ = processor.dispatch_block(parse_container_line(":::foo").unwrap(), 0);
        processor.finalize();
        assert!(
            processor.warnings().is_empty(),
            "unclosed unregistered container must be silent, got: {:?}",
            processor.warnings()
        );
    }

    #[test]
    fn unclosed_registered_container_emits_unclosed_warning() {
        use crate::directive::parser::parse_container_line;
        let mut processor = DirectiveProcessor::new().with_container(TestNote);
        let _ = processor.dispatch_block(parse_container_line(":::note").unwrap(), 0);
        processor.finalize();
        let unclosed: Vec<_> = processor
            .warnings()
            .into_iter()
            .filter(|w| w.contains("unclosed"))
            .collect();
        assert_eq!(unclosed.len(), 1, "got: {unclosed:?}");
        assert!(unclosed[0].contains("note"), "got: {unclosed:?}");
    }

    #[test]
    fn render_unregistered_nested_in_registered_is_well_formed() {
        let processor = DirectiveProcessor::new().with_container(TestNote);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        // Block directives must be blank-line separated.
        let md = ":::note[Hi]\n\n:::xyz\n\ninner\n\n:::\n\n:::\n";
        let result = renderer.render(md, Pipeline::new().with_directives(processor));

        assert_eq!(
            result.html.matches(r#"<div class="note""#).count(),
            1,
            "html: {}",
            result.html
        );
        assert_eq!(
            result.html.matches("</div>").count(),
            1,
            "html: {}",
            result.html
        );
        assert!(result.html.contains(":::xyz"), "html: {}", result.html);
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("stray") || w.contains("unclosed")),
            "warnings: {:?}",
            result.warnings
        );
    }
}
