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

/// What a container opener resolved to, recorded by the walker so it can render
/// the matching close the parser now pairs for it.
///
/// The processor no longer owns a container stack: the parser emits a balanced,
/// well-nested `ContainerDirectiveStart`/`ContainerDirectiveEnd` stream (closing
/// unclosed containers at block and EOF boundaries itself), so pairing lives in
/// the walker, which keeps one of these per open container.
pub(crate) enum ContainerOutcome {
    /// Registered handler at this index; close via
    /// [`container_end`](DirectiveProcessor::container_end).
    Handled(usize),
    /// Unregistered name, or the handler returned `Skip`: the opener rendered
    /// literally, so an explicit close renders literally too and a synthesized
    /// (implicit) close renders nothing.
    Literal,
    /// A walker built-in (`::::tabs` / `:::tab`): the walker owns the tab state
    /// and renders through the backend's tab methods, so the matching close is
    /// handled there rather than by a registered handler. The [`TabScope`]
    /// records which of the three tab shapes this close belongs to.
    Native(TabScope),
}

/// Which tab shape a [`ContainerOutcome::Native`] opener resolved to, so its
/// close renders correctly. Tab metadata (ids, labels, the reserved bar hole)
/// lives in dedicated walker fields, not here.
#[derive(Clone, Copy)]
pub(crate) enum TabScope {
    /// The `::::tabs` group container.
    Group,
    /// A `:::tab` item inside an open group.
    Item,
    /// A `:::tab` with no enclosing `::::tabs` (content rendered unwrapped).
    LoneItem,
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

    /// Dispatch a container-directive opener by name: invoke the registered
    /// handler and return the [`ContainerOutcome`] the walker records — so it
    /// can render the matching close the parser now pairs — plus owned
    /// [`BlockDispatch`] data for the walker to render now. `ctx.line()` is
    /// always `0` — block directives carry no line number (no shipped handler
    /// reads it).
    ///
    /// No container stack is kept here any more: the parser emits a balanced
    /// `ContainerDirectiveStart`/`ContainerDirectiveEnd` stream, so pairing —
    /// and the depth bookkeeping that used to close unclosed containers at a
    /// block boundary — belongs to the parser and the walker's outcome stack.
    ///
    /// The literal reconstruction of an unhandled opener hardcodes three colons,
    /// so `::::name` renders as `:::name` while the walker repeats the closer's
    /// count in full. Pinned debt, not intent — see
    /// `unregistered_container_opener_drops_extra_colons_closer_keeps_them`
    /// in `tests/block_directives.rs`.
    pub(crate) fn dispatch_container_start(
        &mut self,
        name: &str,
        args: DirectiveArgs,
    ) -> (ContainerOutcome, BlockDispatch) {
        let Some(idx) = self.container_handlers.iter().position(|h| h.matches(name)) else {
            // Unregistered: render the opener literally; its close renders
            // literally too rather than closing an enclosing registered
            // container.
            return (
                ContainerOutcome::Literal,
                BlockDispatch::PassThrough(format!(":::{name}{}", args.to_syntax())),
            );
        };
        let syntax = args.to_syntax();
        let ctx = self.config.create_context(0);
        match self.container_handlers[idx].start_named(name, args, &ctx) {
            DirectiveOutput::Html(html) => {
                (ContainerOutcome::Handled(idx), BlockDispatch::Html(html))
            }
            DirectiveOutput::Deferred(parts) => (
                ContainerOutcome::Handled(idx),
                BlockDispatch::Deferred {
                    parts,
                    source: Source::Container(idx),
                },
            ),
            // Handler declined: the opener renders literally.
            DirectiveOutput::Skip => (
                ContainerOutcome::Literal,
                BlockDispatch::PassThrough(format!(":::{name}{syntax}")),
            ),
        }
    }

    /// Render a container close previously opened as
    /// [`ContainerOutcome::Handled(idx)`](ContainerOutcome::Handled). The
    /// handler's `end()` is called with line `0` — block directives carry no
    /// line number (no shipped handler reads it). `None` from the handler emits
    /// nothing.
    pub(crate) fn container_end(&mut self, idx: usize) -> BlockDispatch {
        BlockDispatch::Html(self.container_handlers[idx].end(0).unwrap_or_default())
    }

    /// Dispatch a leaf directive: invoke the registered handler and return
    /// owned [`BlockDispatch`] data for the walker to render. `ctx.line()` is
    /// always `0` — block directives carry no line number (no shipped handler
    /// reads it).
    ///
    /// A leaf opens no scope, so there is no container pairing to record.
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

        let (outcome, dispatch) =
            processor.dispatch_container_start("note", DirectiveArgs::parse("Important", ""));
        assert!(matches!(outcome, ContainerOutcome::Handled(0)));
        match dispatch {
            BlockDispatch::Html(html) => {
                assert!(html.contains(r#"<div class="note" data-title="Important">"#));
            }
            other => panic!("expected Html, got {other:?}"),
        }

        // The walker records `Handled(idx)` and asks for the matching close by
        // index — the processor keeps no stack of its own.
        match processor.container_end(0) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected Html, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_container_unregistered_opener_is_literal() {
        let mut processor = DirectiveProcessor::new();

        // Unregistered opener: the walker records `Literal` so its close renders
        // literally too, and the opener passes through as its own source text.
        let (outcome, dispatch) =
            processor.dispatch_container_start("foo", DirectiveArgs::parse("x", ".c"));
        assert!(matches!(outcome, ContainerOutcome::Literal));
        match dispatch {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::foo[x]{.c}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
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
    fn dispatch_container_skip_is_literal() {
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

        let mut processor = DirectiveProcessor::new().with_container(SkipContainer);

        // A handler that returns `Skip` declines: the walker records `Literal`,
        // and `end()` is never asked for — its close renders literally.
        let (outcome, dispatch) =
            processor.dispatch_container_start("skipme", DirectiveArgs::default());
        assert!(matches!(outcome, ContainerOutcome::Literal));
        match dispatch {
            BlockDispatch::PassThrough(s) => assert_eq!(s, ":::skipme"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
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
