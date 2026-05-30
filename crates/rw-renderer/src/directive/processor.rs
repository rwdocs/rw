//! Directive processor for `CommonMark` directives.
//!
//! Registries for inline/leaf/container handlers, dispatched during the
//! pulldown-cmark event walk, plus post-processing (after rendering) of
//! intermediate markers.

use std::io;
use std::path::{Path, PathBuf};

use super::parser::ParsedDirective;
use super::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective,
    LeafDirective, Replacements,
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
    /// (a container `end()` that returns `None`, or a popped container whose
    /// handler is no longer registered).
    Html(String),
    /// A marker triple — `raw_html(open) + text(body) + raw_html(close)`.
    Marker {
        open: String,
        body: String,
        close: String,
    },
    /// Markdown that the walker must re-parse in context.
    Markdown(String),
    /// Literal text the walker renders as an ordinary paragraph (`<p>…</p>`).
    PassThrough(String),
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
    /// Maximum include depth to prevent infinite recursion.
    ///
    /// Default: 10
    pub max_include_depth: usize,
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
            max_include_depth: 10,
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

    /// Set the maximum include depth.
    #[must_use]
    pub fn with_max_include_depth(mut self, depth: usize) -> Self {
        self.max_include_depth = depth;
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
/// Dispatches directive handlers during the pulldown-cmark event walk and
/// post-processes intermediate markers after rendering.
///
/// # Example
///
/// Register handlers, then drive them through
/// [`MarkdownRenderer::render`](crate::MarkdownRenderer::render): block
/// directives (leaf and container) expand during the pulldown-cmark event
/// walk, and inline directives (`:name[…]`) expand from `Event::Text`
/// content — inline code spans, code blocks, and raw HTML pass through
/// unchanged.
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
    /// Stack of active container directive names for dispatching `end()` calls.
    active_containers: Vec<String>,
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

    /// Maximum recursive include depth, surfaced to the walker (which now owns
    /// the recursion for `DirectiveOutput::Markdown`).
    pub(crate) fn max_include_depth(&self) -> usize {
        self.config.max_include_depth
    }

    /// Dispatch a parsed block directive (leaf or container): invoke the
    /// registered handler, perform the `active_containers` push/pop and warning
    /// bookkeeping, and return owned [`BlockDispatch`] data for the walker to
    /// render. `ctx.line()` is always `0` — block directives carry no line
    /// number (no shipped handler reads it).
    pub(crate) fn dispatch_block(&mut self, directive: ParsedDirective) -> BlockDispatch {
        match directive {
            ParsedDirective::ContainerStart { name, args, .. } => {
                let Some(idx) = self
                    .container_handlers
                    .iter()
                    .position(|h| h.name() == name)
                else {
                    return BlockDispatch::PassThrough(format!(":::{name}{}", args.to_syntax()));
                };
                let syntax = args.to_syntax();
                let ctx = self.config.create_context(0);
                match self.container_handlers[idx].start(args, &ctx) {
                    DirectiveOutput::Html(html) => {
                        self.active_containers.push(name);
                        BlockDispatch::Html(html)
                    }
                    DirectiveOutput::Marker { open, body, close } => {
                        self.active_containers.push(name);
                        BlockDispatch::Marker { open, body, close }
                    }
                    DirectiveOutput::Markdown(md) => {
                        self.active_containers.push(name);
                        BlockDispatch::Markdown(md)
                    }
                    DirectiveOutput::Skip => {
                        BlockDispatch::PassThrough(format!(":::{name}{syntax}"))
                    }
                }
            }
            ParsedDirective::ContainerEnd { colon_count } => {
                if let Some(name) = self.active_containers.pop() {
                    let html = self
                        .container_handlers
                        .iter()
                        .position(|h| h.name() == name)
                        .and_then(|idx| self.container_handlers[idx].end(0))
                        .unwrap_or_default();
                    BlockDispatch::Html(html)
                } else {
                    self.warnings
                        .push("stray ::: with no opening directive".to_owned());
                    BlockDispatch::PassThrough(":".repeat(colon_count))
                }
            }
            ParsedDirective::Leaf { name, args } => {
                let Some(idx) = self.leaf_handlers.iter().position(|h| h.name() == name) else {
                    return BlockDispatch::PassThrough(format!("::{name}{}", args.to_syntax()));
                };
                let syntax = args.to_syntax();
                let ctx = self.config.create_context(0);
                match self.leaf_handlers[idx].process(args, &ctx) {
                    DirectiveOutput::Html(html) => BlockDispatch::Html(html),
                    DirectiveOutput::Marker { open, body, close } => {
                        BlockDispatch::Marker { open, body, close }
                    }
                    DirectiveOutput::Markdown(md) => BlockDispatch::Markdown(md),
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
    /// `name`. Called by [`MarkdownRenderer`](crate::MarkdownRenderer) while
    /// flushing buffered text content from the pulldown-cmark event stream.
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

    pub(crate) fn finalize(&mut self) {
        for name in self.active_containers.drain(..) {
            self.warnings.push(format!(
                "unclosed container directive :::{name} (missing closing :::)"
            ));
        }
    }

    /// Post-process rendered HTML.
    ///
    /// Collects all replacements from handlers and applies them in a single pass.
    pub fn post_process(&mut self, html: &mut String) {
        let capacity =
            self.inline_handlers.len() + self.leaf_handlers.len() + self.container_handlers.len();
        let mut replacements = Replacements::with_capacity(capacity);

        // Collect replacements from all handlers
        for handler in &mut self.inline_handlers {
            handler.post_process(&mut replacements);
        }
        for handler in &mut self.leaf_handlers {
            handler.post_process(&mut replacements);
        }
        for handler in &mut self.container_handlers {
            handler.post_process(&mut replacements);
        }

        // Apply all replacements in single pass
        replacements.apply(html);
    }

    /// Record a warning. Called by [`InlineDirectiveStream`] when it
    /// encounters cases it can't fully honor (e.g., an inline directive
    /// returning `DirectiveOutput::Markdown`).
    ///
    /// [`InlineDirectiveStream`]: super::InlineDirectiveStream
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
        // Inline directives are expanded via `transform_events` (during the
        // pulldown-cmark event stream), not by `process`. Drive the full
        // `MarkdownRenderer` pipeline so the wiring runs end-to-end.

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
    fn test_inline_directive_post_process_runs() {
        struct MarkerDirective;

        impl InlineDirective for MarkerDirective {
            fn name(&self) -> &'static str {
                "marker"
            }

            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                DirectiveOutput::html("<rw-marker>")
            }

            fn post_process(&mut self, replacements: &mut Replacements) {
                replacements.add("<rw-marker>", r#"<span class="marker">"#);
            }
        }

        let processor = DirectiveProcessor::new().with_inline(MarkerDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(":marker[x]", Pipeline::new().with_directives(processor));

        assert!(
            result.html.contains(r#"<span class="marker">"#),
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
        // line should expand. The `transform_events` stream is responsible
        // for skipping `Tag::CodeBlock` content; `process` no longer touches
        // inline syntax at all.

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
            .with_source_path("/docs/guide.md")
            .with_max_include_depth(5);

        assert_eq!(config.base_dir, PathBuf::from("/docs"));
        assert_eq!(config.source_path, Some(PathBuf::from("/docs/guide.md")));
        assert_eq!(config.max_include_depth, 5);
    }

    #[test]
    fn inline_directive_after_leaf_token_in_paragraph_still_expands() {
        // Regression guard: a `::leaf` token mid-line must not stop the
        // scanner from finding a later `:inline` directive on the same line.
        // Driven through the full pipeline because inline expansion now
        // happens in `transform_events`, not in `process`.

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
        match processor.dispatch_block(start) {
            BlockDispatch::Html(html) => {
                assert!(html.contains(r#"<div class="note" data-title="Important">"#));
            }
            other => panic!("expected Html, got {other:?}"),
        }

        let end = parse_container_line(":::").unwrap();
        match processor.dispatch_block(end) {
            BlockDispatch::Html(html) => assert_eq!(html, "</div>"),
            other => panic!("expected Html, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_block_unregistered_and_stray_passthrough() {
        let mut processor = DirectiveProcessor::new();

        let unreg = crate::directive::parser::parse_container_line(":::foo[x]{.c}").unwrap();
        match processor.dispatch_block(unreg) {
            BlockDispatch::PassThrough(s) => {
                assert!(s.starts_with(":::foo[x]"), "got {s}");
                assert!(s.contains(".c"), "got {s}");
            }
            other => panic!("expected PassThrough, got {other:?}"),
        }

        let stray = crate::directive::parser::parse_container_line("::::").unwrap();
        match processor.dispatch_block(stray) {
            BlockDispatch::PassThrough(s) => assert_eq!(s, "::::"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
        assert!(processor.warnings().iter().any(|w| w.contains("stray")));
    }

    #[test]
    fn dispatch_block_leaf_html_and_unregistered() {
        let mut processor = DirectiveProcessor::new().with_leaf(TestYoutube);

        let leaf = crate::directive::parser::parse_leaf_line("::youtube[abc]").unwrap();
        match processor.dispatch_block(leaf) {
            BlockDispatch::Html(html) => assert!(html.contains("abc")),
            other => panic!("expected Html, got {other:?}"),
        }

        let unreg = crate::directive::parser::parse_leaf_line("::missing[y]").unwrap();
        match processor.dispatch_block(unreg) {
            BlockDispatch::PassThrough(s) => assert!(s.starts_with("::missing[y]"), "got {s}"),
            other => panic!("expected PassThrough, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_block_marker_and_markdown() {
        // A container whose start() returns Markdown must surface as
        // BlockDispatch::Markdown AND push onto active_containers (so the
        // following end() fires the handler's `end()`).
        struct MarkdownContainer;

        impl ContainerDirective for MarkdownContainer {
            fn name(&self) -> &'static str {
                "mdwrap"
            }

            fn start(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::markdown("expanded body")
            }

            fn end(&mut self, _line: usize) -> Option<String> {
                Some("<!--mdwrap-end-->".to_owned())
            }
        }

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
                    open: "<open>".to_owned(),
                    body: "the body".to_owned(),
                    close: "</close>".to_owned(),
                }
            }
        }

        let mut processor = DirectiveProcessor::new()
            .with_container(MarkdownContainer)
            .with_leaf(MarkerLeaf);

        let start = crate::directive::parser::parse_container_line(":::mdwrap").unwrap();
        match processor.dispatch_block(start) {
            BlockDispatch::Markdown(md) => assert_eq!(md, "expanded body"),
            other => panic!("expected Markdown, got {other:?}"),
        }

        // Proves the container name was pushed: the closing ::: pops it and
        // dispatches the handler's end().
        let end = crate::directive::parser::parse_container_line(":::").unwrap();
        match processor.dispatch_block(end) {
            BlockDispatch::Html(html) => assert_eq!(html, "<!--mdwrap-end-->"),
            other => panic!("expected Html from container end(), got {other:?}"),
        }

        let leaf = crate::directive::parser::parse_leaf_line("::marker[x]").unwrap();
        match processor.dispatch_block(leaf) {
            BlockDispatch::Marker { open, body, close } => {
                assert_eq!(open, "<open>");
                assert_eq!(body, "the body");
                assert_eq!(close, "</close>");
            }
            other => panic!("expected Marker, got {other:?}"),
        }
    }
}
