//! Inline directive trait.
//!
//! Inline directives use single-colon syntax: `:name[content]{attrs}`

use super::{DirectiveArgs, DirectiveContext, DirectiveOutput, Replacements};

/// Handler for inline directives: `:name[content]{attrs}`
///
/// Inline directives appear within text flow and produce inline HTML elements.
/// They are expanded during the pulldown-cmark event walk — as the renderer
/// flushes text (`flush_text`) it scans `Event::Text` content for `:name[…]`
/// syntax and dispatches to the handler; there is no separate pre-pass.
///
/// # Post-Processing
///
/// Inline directives support post-processing via [`post_process`](Self::post_process).
/// During the event walk, return intermediate HTML that is then transformed
/// during the post-processing pass.
///
/// # Thread Safety
///
/// Handlers implement `Send` only (not `Sync`) since each document gets its own
/// processor instance. For parallel document processing, create separate processor
/// instances per thread.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::{DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective};
///
/// struct KbdDirective;
///
/// impl InlineDirective for KbdDirective {
///     fn name(&self) -> &str { "kbd" }
///
///     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
///     }
/// }
/// ```
pub trait InlineDirective: Send {
    /// Directive name (e.g., "kbd", "abbr").
    ///
    /// This is matched against the directive syntax: `:name[...]`
    fn name(&self) -> &str;

    /// Process the inline directive.
    ///
    /// Returns [`DirectiveOutput::Html`] to emit HTML, [`DirectiveOutput::Skip`]
    /// to pass through unchanged.
    ///
    /// Note: [`DirectiveOutput::Markdown`] is supported but uncommon for inline
    /// directives since they typically produce simple HTML.
    fn process(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput;

    /// Register post-processing replacements for intermediate marker elements.
    ///
    /// Inline directives that emit neutral marker elements during
    /// [`process`](Self::process) can override this to rewrite those markers
    /// into final HTML after rendering, using the [`Replacements`] collector.
    /// The default is a no-op — most inline directives emit final HTML
    /// directly and need nothing here.
    fn post_process(&mut self, _replacements: &mut Replacements) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct TestKbd;

    impl InlineDirective for TestKbd {
        fn name(&self) -> &'static str {
            "kbd"
        }

        fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
        }
    }

    #[test]
    fn test_inline_directive() {
        let mut kbd = TestKbd;

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        let args = DirectiveArgs::parse("Ctrl+C", "");
        let output = kbd.process(args, &ctx);

        assert_eq!(
            output,
            DirectiveOutput::Html("<kbd>Ctrl+C</kbd>".to_owned())
        );
    }

    #[test]
    fn test_inline_directive_name() {
        let kbd = TestKbd;
        assert_eq!(kbd.name(), "kbd");
    }

    #[test]
    fn test_default_post_process() {
        let mut kbd = TestKbd;
        let mut replacements = Replacements::new();
        kbd.post_process(&mut replacements);
        assert!(replacements.is_empty());
    }
}
