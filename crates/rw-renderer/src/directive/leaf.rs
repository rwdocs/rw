//! Leaf directive trait.
//!
//! Leaf directives use double-colon syntax: `::name[content]{attrs}`

use super::{DirectiveArgs, DirectiveContext, DirectiveOutput, Fills};

/// Handler for leaf directives: `::name[content]{attrs}`
///
/// Leaf directives are block-level. The handler is invoked during the render
/// walk, when `::name[…]{…}` is recognized as its own blank-line-separated
/// paragraph (leading/trailing whitespace permitted). A `::name` token that
/// shares a paragraph with other text, or one indented into a code block, is
/// treated as literal text and left to the markdown parser.
///
/// They return HTML (for `::youtube`), a semantic marker, or deferred content.
///
/// # Deferred Content
///
/// A leaf whose content is not known during the walk returns
/// [`DirectiveOutput::Deferred`] from [`process`](Self::process), reserving a
/// hole in the output, and supplies that hole's content from
/// [`fills`](Self::fills) once the walk completes.
///
/// # Thread Safety
///
/// Handlers implement `Send` only (not `Sync`) since each document gets its own
/// processor instance.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::{
///     DirectiveArgs, DirectiveContext, DirectiveOutput, LeafDirective,
/// };
///
/// struct YoutubeDirective;
///
/// impl LeafDirective for YoutubeDirective {
///     fn name(&self) -> &str { "youtube" }
///
///     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         let width = args.get("width").unwrap_or("560");
///         let height = args.get("height").unwrap_or("315");
///         DirectiveOutput::html(format!(
///             r#"<iframe src="https://www.youtube.com/embed/{}" width="{width}" height="{height}" frameborder="0" allowfullscreen></iframe>"#,
///             args.content()
///         ))
///     }
/// }
/// ```
pub trait LeafDirective: Send {
    /// Directive name (e.g., "youtube", "include", "toc").
    ///
    /// This is matched against the directive syntax: `::name[...]`
    fn name(&self) -> &str;

    /// Process the leaf directive.
    ///
    /// Returns:
    /// - [`DirectiveOutput::Html`] for HTML output that passes through pulldown-cmark
    /// - [`DirectiveOutput::Marker`] for a semantic marker each backend renders
    ///   its own way
    /// - [`DirectiveOutput::Deferred`] for content that is not known during the
    ///   walk — it reserves holes that [`fills`](Self::fills) supplies afterwards
    /// - [`DirectiveOutput::Skip`] to pass through unchanged
    fn process(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput;

    /// Supply content for holes this directive reserved during the walk.
    ///
    /// Called once, after the walk completes, before assembly. Override when
    /// `process()` returned [`DirectiveOutput::Deferred`].
    ///
    /// Called on every registered handler whether or not it deferred
    /// anything.
    fn fills(&mut self, _fills: &mut Fills) {}

    /// Get warnings generated during processing.
    ///
    /// Override this method if your directive can produce warnings.
    fn warnings(&self) -> &[String] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::path::Path;

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

    #[test]
    fn test_leaf_directive() {
        let mut youtube = TestYoutube;

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        let args = DirectiveArgs::parse("dQw4w9WgXcQ", "");
        let output = youtube.process(args, &ctx);

        assert_matches!(output, DirectiveOutput::Html(s) if s.contains("dQw4w9WgXcQ"));
    }

    #[test]
    fn test_default_warnings() {
        let youtube = TestYoutube;
        assert!(youtube.warnings().is_empty());
    }
}
