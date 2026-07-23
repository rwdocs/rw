//! Directive processor for `CommonMark` directives.
//!
//! Registries for inline/leaf/container handlers, dispatched during the render
//! walk, plus collection of the deferred content that fills their reserved
//! holes once the walk completes.

use std::io;
use std::path::{Path, PathBuf};

use super::fills::{GlobalFills, Source};
use super::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Fills, InlineDirective,
    LeafDirective, Part,
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
    /// when the closing `:::` is reached. `name` is the opener's own name as
    /// written (e.g. `"tab"`), not the handler's `name()` — a handler whose
    /// `matches()` accepts more than one name (like `TabsDirective`, for both
    /// `tabs` and `tab`) would otherwise misreport an unclosed opener under
    /// its single registered name.
    Handled {
        idx: usize,
        depth: usize,
        name: String,
    },
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
    /// Opener names of containers already balanced early, when their
    /// enclosing blockquote or list item ended. They are off
    /// `active_containers` (so they are neither closed nor popped twice) but
    /// [`finalize`](DirectiveProcessor::finalize) still owes each one warning.
    closed_at_block_end: Vec<String>,
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
        // Guard is name-based (not `matches`): it catches a second handler
        // registered under the same primary name, not one that merely aliases
        // a name another handler already `matches`.
        if self.container_handlers.iter().any(|h| h.name() == name) {
            self.warnings.push(format!(
                "container directive ':::{name}' is registered more than once; only the first handler will be dispatched"
            ));
        }
        self.container_handlers.push(Box::new(handler));
        self
    }

    /// Dispatch a container-directive opener: invoke the registered handler
    /// and return owned [`BlockDispatch`] data for the walker to render.
    /// `ctx.line()` is always `0` — block directives carry no line number (no
    /// shipped handler reads it).
    ///
    /// Any handled, non-`Skip` opener pushes an `active_containers` frame that
    /// its matching closer pops.
    ///
    /// `depth` is the walker's enclosing block nesting (blockquote and list
    /// levels) at this directive. A frame remembers it so an unclosed
    /// container is balanced when its enclosing block ends — see
    /// [`close_containers_nested_in`](Self::close_containers_nested_in).
    ///
    /// The literal reconstruction of an unhandled opener hardcodes three
    /// colons, so `::::name` renders as `:::name` while
    /// [`dispatch_container_end`](Self::dispatch_container_end) repeats its
    /// closer's count in full. Pinned debt, not intent — see
    /// `unregistered_container_opener_drops_extra_colons_closer_keeps_them`
    /// in `tests/block_directives.rs`.
    pub(crate) fn dispatch_container_start(
        &mut self,
        name: &str,
        args: DirectiveArgs,
        depth: usize,
    ) -> BlockDispatch {
        let Some(idx) = self.container_handlers.iter().position(|h| h.matches(name)) else {
            // Unregistered: render the opener literally and track the
            // scope so its closing ::: renders literally too, rather
            // than closing an enclosing registered container.
            self.active_containers
                .push(ContainerFrame::Literal { depth });
            return BlockDispatch::PassThrough(format!(":::{name}{}", args.to_syntax()));
        };
        let syntax = args.to_syntax();
        let ctx = self.config.create_context(0);
        let output = self.container_handlers[idx].start_named(name, args, &ctx);
        // A handled opener owns its scope unless it declined (`Skip`, which
        // pushes its own Literal frame below).
        if !matches!(output, DirectiveOutput::Skip) {
            self.active_containers.push(ContainerFrame::Handled {
                idx,
                depth,
                name: name.to_owned(),
            });
        }
        match output {
            DirectiveOutput::Html(html) => BlockDispatch::Html(html),
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

    /// Dispatch a container-directive closer: pop the innermost open scope and
    /// return owned [`BlockDispatch`] data for the walker to render. The
    /// handler's `end()` is called with line `0` — block directives carry no
    /// line number (no shipped handler reads it).
    ///
    /// A closer with nothing on the stack is a stray delimiter. It warns and
    /// renders literally rather than being swallowed, so the document still
    /// shows what the author wrote.
    pub(crate) fn dispatch_container_end(&mut self, colon_count: usize) -> BlockDispatch {
        match self.active_containers.pop() {
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
        }
    }

    /// Dispatch a leaf directive: invoke the registered handler and return
    /// owned [`BlockDispatch`] data for the walker to render. `ctx.line()` is
    /// always `0` — block directives carry no line number (no shipped handler
    /// reads it).
    ///
    /// A leaf opens no scope, so there is no `active_containers` bookkeeping
    /// and no enclosing depth to record.
    pub(crate) fn dispatch_leaf(&mut self, name: &str, args: DirectiveArgs) -> BlockDispatch {
        let Some(idx) = self.leaf_handlers.iter().position(|h| h.name() == name) else {
            return BlockDispatch::PassThrough(format!("::{name}{}", args.to_syntax()));
        };
        let syntax = args.to_syntax();
        let ctx = self.config.create_context(0);
        match self.leaf_handlers[idx].process(args, &ctx) {
            DirectiveOutput::Html(html) => BlockDispatch::Html(html),
            DirectiveOutput::Deferred(parts) => BlockDispatch::Deferred {
                parts,
                source: Source::Leaf(idx),
            },
            DirectiveOutput::Skip => BlockDispatch::PassThrough(format!("::{name}{syntax}")),
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
    /// [`dispatch_container_end`](Self::dispatch_container_end), so without this
    /// its opening tags would be left dangling and the rest of the document
    /// would nest inside them — for tabs, inside a `hidden` panel, making it
    /// invisible.
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
            if let ContainerFrame::Handled { idx, name, .. } = frame {
                if let Some(html) = self.container_handlers[idx].end(0) {
                    write_html(&html, out);
                }
                self.closed_at_block_end.push(name);
            }
        }
    }

    pub(crate) fn finalize(&mut self) {
        // mem::take avoids borrowing self.active_containers while reading
        // self.container_handlers / pushing to self.warnings below.
        let frames = std::mem::take(&mut self.active_containers);
        let closed_early = std::mem::take(&mut self.closed_at_block_end);
        let names = frames
            .into_iter()
            .filter_map(|frame| match frame {
                // Literal frames: the opener rendered as plain text, so there is
                // no managed container to warn about.
                ContainerFrame::Handled { name, .. } => Some(name),
                ContainerFrame::Literal { .. } => None,
            })
            // Containers balanced early at a blockquote/list-item boundary are
            // off the stack but just as unclosed: they still owe one warning.
            .chain(closed_early);
        for name in names {
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
    fn dispatch_container_start_and_end() {
        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        match processor.dispatch_container_start("note", DirectiveArgs::parse("Important", ""), 0) {
            BlockDispatch::Html(html) => {
                assert!(html.contains(r#"<div class="note" data-title="Important">"#));
            }
            other => panic!("expected Html, got {other:?}"),
        }

        match processor.dispatch_container_end(3) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected Html, got {other:?}"),
        }
    }

    #[test]
    fn unregistered_container_nested_in_registered_does_not_corrupt_stack() {
        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        // Outer registered container opens its scope.
        match processor.dispatch_container_start("note", DirectiveArgs::parse("Important", ""), 0) {
            BlockDispatch::Html(html) => assert!(html.contains(r#"data-title="Important""#)),
            other => panic!("expected Html, got {other:?}"),
        }
        // Inner UNREGISTERED container: rendered literally, tracked separately.
        match processor.dispatch_container_start("unknown", DirectiveArgs::default(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::unknown"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        // First close pairs with the inner unregistered opener -> literal,
        // it must NOT close the outer note.
        match processor.dispatch_container_end(3) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::"),
            other => panic!("expected literal PassThrough for inner close, got {other:?}"),
        }
        // Second close pairs with the outer note -> note.end().
        match processor.dispatch_container_end(3) {
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
    fn dispatch_container_unregistered_pair_passthrough() {
        let mut processor = DirectiveProcessor::new();

        // Unregistered opener: rendered literally, scope tracked so its close pairs with it.
        match processor.dispatch_container_start("foo", DirectiveArgs::parse("x", ".c"), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::foo[x]{.c}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }

        // Its matching close renders literally and does NOT warn about a stray.
        match processor.dispatch_container_end(3) {
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
    fn dispatch_container_end_genuine_stray_close_warns() {
        let mut processor = DirectiveProcessor::new();

        // A close with no opener on the stack is a genuine stray.
        match processor.dispatch_container_end(4) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, "::::"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        assert!(processor.warnings().iter().any(|w| w.contains("stray")));
    }

    #[test]
    fn dispatch_leaf_html_and_unregistered() {
        let mut processor = DirectiveProcessor::new().with_leaf(TestYoutube);

        match processor.dispatch_leaf("youtube", DirectiveArgs::parse("abc", "")) {
            BlockDispatch::Html(html) => assert!(html.contains("abc")),
            other => panic!("expected Html, got {other:?}"),
        }

        match processor.dispatch_leaf("missing", DirectiveArgs::parse("y", "")) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, "::missing[y]"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
    }

    #[test]
    fn skip_container_nested_in_registered_does_not_corrupt_stack() {
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

        let _ = processor.dispatch_container_start("note", DirectiveArgs::parse("T", ""), 0);
        match processor.dispatch_container_start("skipme", DirectiveArgs::default(), 0) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::skipme"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        match processor.dispatch_container_end(3) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::"),
            other => panic!("expected literal PassThrough, got {other:?}"),
        }
        match processor.dispatch_container_end(3) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected note end(), got {other:?}"),
        }
    }

    #[test]
    fn unclosed_unregistered_container_emits_no_warning() {
        let mut processor = DirectiveProcessor::new();
        let _ = processor.dispatch_container_start("foo", DirectiveArgs::default(), 0);
        processor.finalize();
        assert!(
            processor.warnings().is_empty(),
            "unclosed unregistered container must be silent, got: {:?}",
            processor.warnings()
        );
    }

    #[test]
    fn unclosed_registered_container_emits_unclosed_warning() {
        let mut processor = DirectiveProcessor::new().with_container(TestNote);
        let _ = processor.dispatch_container_start("note", DirectiveArgs::default(), 0);
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
