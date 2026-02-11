//! Directive processor for `CommonMark` directives.
//!
//! Handles preprocessing (before pulldown-cmark) and post-processing (after rendering).

use std::io;
use std::path::{Path, PathBuf};

use crate::tabs::FenceTracker;

use super::parser::{ParsedDirective, parse_container_line, parse_line};
use super::{
    ContainerDirective, DirectiveContext, DirectiveOutput, InlineDirective, LeafDirective,
    Replacements,
};

/// Type alias for the file reading callback function.
pub type ReadFileFn = dyn Fn(&Path) -> io::Result<String> + Send;

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
/// Handles both preprocessing (before pulldown-cmark) and post-processing
/// (after rendering) of directive syntax.
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use rw_renderer::directive::{
///     DirectiveProcessor, DirectiveProcessorConfig, DirectiveArgs,
///     DirectiveContext, DirectiveOutput, InlineDirective,
/// };
///
/// struct KbdDirective;
///
/// impl InlineDirective for KbdDirective {
///     fn name(&self) -> &str { "kbd" }
///     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content))
///     }
/// }
///
/// let mut processor = DirectiveProcessor::new()
///     .with_inline(KbdDirective);
///
/// let output = processor.process("Press :kbd[Ctrl+C] to copy.");
/// assert!(output.contains("<kbd>Ctrl+C</kbd>"));
/// ```
pub struct DirectiveProcessor {
    config: DirectiveProcessorConfig,
    inline_handlers: Vec<Box<dyn InlineDirective>>,
    leaf_handlers: Vec<Box<dyn LeafDirective>>,
    container_handlers: Vec<Box<dyn ContainerDirective>>,
    fence: FenceTracker,
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
            fence: FenceTracker::new(),
            active_containers: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Register an inline directive handler.
    #[must_use]
    pub fn with_inline<D: InlineDirective + 'static>(mut self, handler: D) -> Self {
        self.inline_handlers.push(Box::new(handler));
        self
    }

    /// Register a leaf directive handler.
    #[must_use]
    pub fn with_leaf<D: LeafDirective + 'static>(mut self, handler: D) -> Self {
        self.leaf_handlers.push(Box::new(handler));
        self
    }

    /// Register a container directive handler.
    #[must_use]
    pub fn with_container<D: ContainerDirective + 'static>(mut self, handler: D) -> Self {
        self.container_handlers.push(Box::new(handler));
        self
    }

    /// Preprocess markdown, converting directives to intermediate HTML or expanding includes.
    ///
    /// When a directive returns [`DirectiveOutput::Markdown`], the returned content
    /// is recursively processed (up to `max_include_depth` levels).
    #[must_use]
    pub fn process(&mut self, input: &str) -> String {
        self.process_with_depth(input, 0)
    }

    fn process_with_depth(&mut self, input: &str, depth: usize) -> String {
        if depth > self.config.max_include_depth {
            self.warnings.push(format!(
                "Maximum include depth ({}) exceeded",
                self.config.max_include_depth
            ));
            return input.to_owned();
        }

        let mut output = String::with_capacity(input.len());
        let lines: Vec<&str> = input.lines().collect();
        let line_count = lines.len();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let processed = self.process_line(line, line_num, depth);
            output.push_str(&processed);

            // Preserve line endings
            if idx < line_count - 1 || input.ends_with('\n') {
                output.push('\n');
            }
        }

        // Check for unclosed containers
        self.finalize();

        output
    }

    fn process_line(&mut self, line: &str, line_num: usize, depth: usize) -> String {
        // Update fence state
        self.fence.update(line);

        // Skip directive processing inside code fences
        if self.fence.in_fence() {
            return line.to_owned();
        }

        // Try container directive first (takes whole line)
        if let Some(directive) = parse_container_line(line) {
            return self.dispatch_container(directive, line_num, depth);
        }

        // Try inline/leaf directives (can be within a line)
        self.process_inline_directives(line, line_num, depth)
    }

    fn process_inline_directives(&mut self, line: &str, line_num: usize, depth: usize) -> String {
        let mut result = String::with_capacity(line.len());
        let mut remaining = line;

        while !remaining.is_empty() {
            if let Some((directive, start, end)) = parse_line(remaining) {
                // Add content before the directive
                result.push_str(&remaining[..start]);

                // Process the directive
                let output = self.dispatch_inline_or_leaf(directive, line_num);

                match output {
                    DirectiveOutput::Html(html) => result.push_str(&html),
                    DirectiveOutput::Markdown(md) => {
                        let processed = self.process_with_depth(&md, depth + 1);
                        result.push_str(&processed);
                    }
                    DirectiveOutput::Skip => {
                        // Pass through unchanged
                        result.push_str(&remaining[start..end]);
                    }
                }

                remaining = &remaining[end..];
            } else {
                // No more directives, add remaining content
                result.push_str(remaining);
                break;
            }
        }

        result
    }

    fn dispatch_container(
        &mut self,
        directive: ParsedDirective,
        line_num: usize,
        depth: usize,
    ) -> String {
        match directive {
            ParsedDirective::ContainerStart { name, args, .. } => {
                // Find handler index for this directive
                let handler_idx = self
                    .container_handlers
                    .iter()
                    .position(|h| h.name() == name);

                if let Some(idx) = handler_idx {
                    let syntax = args.to_syntax();
                    let ctx = self.config.create_context(line_num);
                    let output = self.container_handlers[idx].start(args, &ctx);

                    match output {
                        DirectiveOutput::Html(html) => {
                            self.active_containers.push(name);
                            html
                        }
                        DirectiveOutput::Markdown(md) => {
                            self.active_containers.push(name);
                            self.process_with_depth(&md, depth + 1)
                        }
                        DirectiveOutput::Skip => {
                            // Handler declined, pass through with original syntax
                            format!(":::{name}{syntax}")
                        }
                    }
                } else {
                    // No handler, pass through unchanged with original syntax
                    format!(":::{name}{}", args.to_syntax())
                }
            }
            ParsedDirective::ContainerEnd { colon_count } => {
                if let Some(name) = self.active_containers.pop() {
                    // Find handler index and call end
                    let handler_idx = self
                        .container_handlers
                        .iter()
                        .position(|h| h.name() == name);

                    if let Some(idx) = handler_idx {
                        self.container_handlers[idx]
                            .end(line_num)
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    // Stray closing
                    self.warnings.push(format!(
                        "line {line_num}: stray ::: with no opening directive"
                    ));
                    ":".repeat(colon_count)
                }
            }
            _ => unreachable!("dispatch_container only handles container directives"),
        }
    }

    fn dispatch_inline_or_leaf(
        &mut self,
        directive: ParsedDirective,
        line_num: usize,
    ) -> DirectiveOutput {
        match directive {
            ParsedDirective::Inline { name, args } => {
                let handler_idx = self.inline_handlers.iter().position(|h| h.name() == name);

                if let Some(idx) = handler_idx {
                    let ctx = self.config.create_context(line_num);
                    self.inline_handlers[idx].process(args, &ctx)
                } else {
                    DirectiveOutput::Skip
                }
            }
            ParsedDirective::Leaf { name, args } => {
                let handler_idx = self.leaf_handlers.iter().position(|h| h.name() == name);

                if let Some(idx) = handler_idx {
                    let ctx = self.config.create_context(line_num);
                    self.leaf_handlers[idx].process(args, &ctx)
                } else {
                    DirectiveOutput::Skip
                }
            }
            _ => DirectiveOutput::Skip,
        }
    }

    fn finalize(&mut self) {
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
        let capacity = self.leaf_handlers.len() + self.container_handlers.len();
        let mut replacements = Replacements::with_capacity(capacity);

        // Collect replacements from all handlers
        for handler in &mut self.leaf_handlers {
            handler.post_process(&mut replacements);
        }
        for handler in &mut self.container_handlers {
            handler.post_process(&mut replacements);
        }

        // Apply all replacements in single pass
        replacements.apply(html);
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

    // Test inline directive
    struct TestKbd;

    impl InlineDirective for TestKbd {
        fn name(&self) -> &'static str {
            "kbd"
        }

        fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
            DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content))
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
                args.content
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
            let title = if args.content.is_empty() {
                "Note".to_owned()
            } else {
                args.content
            };
            DirectiveOutput::html(format!(r#"<div class="note" data-title="{title}">"#))
        }

        fn end(&mut self, _line: usize) -> Option<String> {
            Some("</div>".to_owned())
        }
    }

    #[test]
    fn test_inline_directive() {
        let mut processor = DirectiveProcessor::new().with_inline(TestKbd);

        let output = processor.process("Press :kbd[Ctrl+C] to copy.");
        assert_eq!(output, "Press <kbd>Ctrl+C</kbd> to copy.");
    }

    #[test]
    fn test_multiple_inline_directives() {
        let mut processor = DirectiveProcessor::new().with_inline(TestKbd);

        let output = processor.process("Press :kbd[Ctrl+C] then :kbd[Ctrl+V].");
        assert_eq!(output, "Press <kbd>Ctrl+C</kbd> then <kbd>Ctrl+V</kbd>.");
    }

    #[test]
    fn test_leaf_directive() {
        let mut processor = DirectiveProcessor::new().with_leaf(TestYoutube);

        let output = processor.process("::youtube[dQw4w9WgXcQ]");
        assert!(output.contains("dQw4w9WgXcQ"));
    }

    #[test]
    fn test_container_directive() {
        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        let output = processor.process(":::note[Important]\nContent here\n:::");
        assert!(output.contains(r#"<div class="note" data-title="Important">"#));
        assert!(output.contains("Content here"));
        assert!(output.contains("</div>"));
    }

    #[test]
    fn test_unknown_directive_passthrough() {
        let mut processor = DirectiveProcessor::new();

        let output = processor.process(":unknown[content]");
        assert_eq!(output, ":unknown[content]");
    }

    #[test]
    fn test_unknown_container_passthrough() {
        let mut processor = DirectiveProcessor::new();

        // Without brackets
        let output = processor.process(":::unknown\nContent\n:::");
        assert!(output.contains(":::unknown"));

        // With bracket syntax - should preserve content
        let mut processor2 = DirectiveProcessor::new();
        let output2 = processor2.process(":::unknown[content]\nBody\n:::");
        assert!(output2.contains(":::unknown[content]"));

        // With bracket syntax and attributes - should preserve both
        let mut processor3 = DirectiveProcessor::new();
        let output3 = processor3.process(":::unknown[Important]{#note-1 .highlight}\nBody\n:::");
        assert!(output3.contains(":::unknown[Important]"));
        assert!(output3.contains("#note-1"));
        assert!(output3.contains(".highlight"));
    }

    #[test]
    fn test_code_fence_skipping() {
        let mut processor = DirectiveProcessor::new().with_inline(TestKbd);

        let input = "```\n:kbd[inside fence]\n```\n:kbd[outside]";
        let output = processor.process(input);

        assert!(output.contains(":kbd[inside fence]")); // Should NOT be processed
        assert!(output.contains("<kbd>outside</kbd>")); // Should be processed
    }

    #[test]
    fn test_unclosed_container_warning() {
        let mut processor = DirectiveProcessor::new().with_container(TestNote);

        let _output = processor.process(":::note\nContent");
        let warnings = processor.warnings();

        assert!(warnings.iter().any(|w| w.contains("unclosed")));
    }

    #[test]
    fn test_stray_close_warning() {
        let mut processor = DirectiveProcessor::new();

        let output = processor.process(":::");
        let warnings = processor.warnings();

        assert!(warnings.iter().any(|w| w.contains("stray")));
        assert_eq!(output.trim(), ":::");
    }

    #[test]
    fn test_nested_containers() {
        struct TestDetails {
            depth: usize,
        }

        impl ContainerDirective for TestDetails {
            fn name(&self) -> &'static str {
                "details"
            }

            fn start(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                self.depth += 1;
                DirectiveOutput::html(format!(
                    "<details data-depth=\"{}\"><summary>{}</summary>",
                    self.depth,
                    if args.content.is_empty() {
                        "Details"
                    } else {
                        &args.content
                    }
                ))
            }

            fn end(&mut self, _line: usize) -> Option<String> {
                self.depth -= 1;
                Some("</details>".to_owned())
            }
        }

        let mut processor = DirectiveProcessor::new().with_container(TestDetails { depth: 0 });

        let input = ":::details[Outer]\n:::details[Inner]\n:::\n:::";
        let output = processor.process(input);

        assert!(output.contains(r#"data-depth="1""#));
        assert!(output.contains(r#"data-depth="2""#));
        assert_eq!(output.matches("</details>").count(), 2);
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
    fn test_include_depth_limit() {
        struct TestInclude;

        impl LeafDirective for TestInclude {
            fn name(&self) -> &'static str {
                "include"
            }

            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                // Return markdown that includes itself (infinite recursion)
                DirectiveOutput::markdown("::include[self]")
            }
        }

        let config = DirectiveProcessorConfig::new().with_max_include_depth(3);
        let mut processor = DirectiveProcessor::with_config(config).with_leaf(TestInclude);

        let _output = processor.process("::include[start]");
        let warnings = processor.warnings();

        assert!(warnings.iter().any(|w| w.contains("Maximum include depth")));
    }
}
