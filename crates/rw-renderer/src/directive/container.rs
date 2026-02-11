//! Container directive trait.
//!
//! Container directives use triple-colon syntax: `:::name` ... `:::`

use super::{DirectiveArgs, DirectiveContext, DirectiveOutput, Replacements};

/// Handler for container directives: `:::name` ... `:::`
///
/// Container directives wrap arbitrary content and have start/end phases.
/// Handlers manage their own nesting state internally (e.g., via a stack).
///
/// # Two-Phase Processing
///
/// Container directives are processed in two phases:
///
/// 1. **Preprocessing**: [`start`](Self::start) and [`end`](Self::end) are called
///    to emit intermediate HTML that passes through pulldown-cmark
///
/// 2. **Post-processing**: [`post_process`](Self::post_process) transforms
///    intermediate elements to final HTML
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
///     DirectiveArgs, DirectiveContext, DirectiveOutput, ContainerDirective, Replacements,
/// };
///
/// struct NoteDirective;
///
/// impl ContainerDirective for NoteDirective {
///     fn name(&self) -> &str { "note" }
///
///     fn start(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         let title = if args.content.is_empty() { "Note" } else { &args.content };
///         DirectiveOutput::html(format!(
///             r#"<div class="note"><div class="note-title">{title}</div><div class="note-body">"#
///         ))
///     }
///
///     fn end(&mut self, _line: usize) -> Option<String> {
///         Some("</div></div>".to_string())
///     }
/// }
/// ```
pub trait ContainerDirective: Send {
    /// Directive name (e.g., "note", "warning", "tab").
    ///
    /// This is matched against the directive syntax: `:::name`
    fn name(&self) -> &str;

    /// Handle opening `:::name[content]{attrs}`.
    ///
    /// Returns the opening output:
    /// - [`DirectiveOutput::Html`] to emit opening HTML tags
    /// - [`DirectiveOutput::Skip`] to pass through (don't handle)
    ///
    /// Note: [`DirectiveOutput::Markdown`] is supported for advanced use cases
    /// where the opening needs markdown processing.
    fn start(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput;

    /// Handle closing `:::`.
    ///
    /// Returns closing HTML, or `None` to emit nothing.
    ///
    /// **Invariant**: `DirectiveProcessor` only calls `end()` when there's a
    /// matching `start()`. If this method panics, it indicates a bug in either
    /// the processor or the handler's state management.
    fn end(&mut self, line: usize) -> Option<String>;

    /// Register string replacements to apply after rendering.
    ///
    /// All replacements are collected and applied in a single pass.
    /// Override this method if your directive needs post-processing
    /// (e.g., to transform intermediate elements to accessible HTML).
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

    struct TestNote;

    impl ContainerDirective for TestNote {
        fn name(&self) -> &'static str {
            "note"
        }

        fn start(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            let title = if args.content.is_empty() {
                "Note"
            } else {
                &args.content
            };
            DirectiveOutput::html(format!(r#"<div class="note" data-title="{title}">"#))
        }

        fn end(&mut self, _line: usize) -> Option<String> {
            Some("</div>".to_owned())
        }
    }

    struct TestDetails {
        stack: Vec<usize>,
    }

    impl TestDetails {
        fn new() -> Self {
            Self { stack: Vec::new() }
        }
    }

    impl ContainerDirective for TestDetails {
        fn name(&self) -> &'static str {
            "details"
        }

        fn start(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput {
            self.stack.push(ctx.line);
            let summary = if args.content.is_empty() {
                "Details"
            } else {
                &args.content
            };
            DirectiveOutput::html(format!(
                "<details><summary>{summary}</summary><div class=\"details-body\">"
            ))
        }

        fn end(&mut self, _line: usize) -> Option<String> {
            self.stack
                .pop()
                .expect("end() called without matching start()");
            Some("</div></details>".to_owned())
        }
    }

    #[test]
    fn test_container_start() {
        let mut note = TestNote;

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        let args = DirectiveArgs::parse("Important", "");
        let output = note.start(args, &ctx);

        assert!(matches!(output, DirectiveOutput::Html(s) if s.contains("Important")));
    }

    #[test]
    fn test_container_end() {
        let mut note = TestNote;
        let output = note.end(10);
        assert_eq!(output, Some("</div>".to_owned()));
    }

    #[test]
    fn test_container_with_stack() {
        let mut details = TestDetails::new();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 5,
            read_file: &|_| Ok(String::new()),
        };

        // Start directive
        let args = DirectiveArgs::parse("Click to expand", "");
        let start_output = details.start(args, &ctx);
        assert!(matches!(start_output, DirectiveOutput::Html(s) if s.contains("Click to expand")));

        // End directive
        let end_output = details.end(10);
        assert_eq!(end_output, Some("</div></details>".to_owned()));
    }

    #[test]
    fn test_default_post_process() {
        let mut note = TestNote;
        let mut replacements = Replacements::new();
        note.post_process(&mut replacements);
        assert!(replacements.is_empty());
    }

    #[test]
    fn test_default_warnings() {
        let note = TestNote;
        assert!(note.warnings().is_empty());
    }
}
