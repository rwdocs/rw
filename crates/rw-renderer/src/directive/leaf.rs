//! Leaf directive trait.
//!
//! Leaf directives use double-colon syntax: `::name[content]{attrs}`

use super::{DirectiveArgs, DirectiveContext, DirectiveOutput, Replacements};

/// Handler for leaf directives: `::name[content]{attrs}`
///
/// Leaf directives are self-contained blocks (like void HTML elements).
/// They can return markdown (for `::include`) or HTML (for `::youtube`).
///
/// # Two-Phase Processing
///
/// Leaf directives support post-processing via [`post_process`](Self::post_process).
/// During preprocessing, return intermediate HTML that will be transformed
/// during post-processing.
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
///     DirectiveArgs, DirectiveContext, DirectiveOutput, LeafDirective, Replacements,
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
///             args.content
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
    /// - [`DirectiveOutput::Markdown`] for content that needs full pipeline processing
    ///   (used by `::include` to inline file contents)
    /// - [`DirectiveOutput::Skip`] to pass through unchanged
    fn process(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput;

    /// Register string replacements to apply after rendering.
    ///
    /// All replacements are collected and applied in a single pass.
    /// Override this method if your directive needs post-processing.
    fn post_process(&mut self, _replacements: &mut Replacements) {}

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
    use std::path::Path;

    struct TestYoutube;

    impl LeafDirective for TestYoutube {
        fn name(&self) -> &'static str {
            "youtube"
        }

        fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            DirectiveOutput::html(format!(
                r#"<iframe src="https://www.youtube.com/embed/{}"></iframe>"#,
                args.content
            ))
        }
    }

    struct TestInclude {
        warnings: Vec<String>,
    }

    impl TestInclude {
        fn new() -> Self {
            Self {
                warnings: Vec::new(),
            }
        }
    }

    impl LeafDirective for TestInclude {
        fn name(&self) -> &'static str {
            "include"
        }

        fn process(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput {
            let path = ctx.resolve_path(&args.content);
            match ctx.read(&path) {
                Ok(contents) => DirectiveOutput::markdown(contents),
                Err(e) => {
                    self.warnings.push(format!(
                        "line {}: failed to include '{}': {}",
                        ctx.line, args.content, e
                    ));
                    DirectiveOutput::Skip
                }
            }
        }

        fn warnings(&self) -> &[String] {
            &self.warnings
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

        assert!(matches!(output, DirectiveOutput::Html(s) if s.contains("dQw4w9WgXcQ")));
    }

    #[test]
    fn test_include_success() {
        let mut include = TestInclude::new();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 5,
            read_file: &|_| Ok("# Included Content".to_string()),
        };

        let args = DirectiveArgs::parse("snippet.md", "");
        let output = include.process(args, &ctx);

        assert_eq!(
            output,
            DirectiveOutput::Markdown("# Included Content".to_string())
        );
        assert!(include.warnings().is_empty());
    }

    #[test]
    fn test_include_failure() {
        let mut include = TestInclude::new();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 10,
            read_file: &|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "not found",
                ))
            },
        };

        let args = DirectiveArgs::parse("missing.md", "");
        let output = include.process(args, &ctx);

        assert_eq!(output, DirectiveOutput::Skip);
        assert_eq!(include.warnings().len(), 1);
        assert!(include.warnings()[0].contains("line 10"));
        assert!(include.warnings()[0].contains("missing.md"));
    }

    #[test]
    fn test_default_post_process() {
        let mut youtube = TestYoutube;
        let mut replacements = Replacements::new();
        youtube.post_process(&mut replacements);
        assert!(replacements.is_empty());
    }

    #[test]
    fn test_default_warnings() {
        let youtube = TestYoutube;
        assert!(youtube.warnings().is_empty());
    }
}
