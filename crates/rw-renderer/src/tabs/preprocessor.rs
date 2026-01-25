//! Tabs preprocessor for converting CommonMark directives to HTML elements.
//!
//! Converts `::: tabs` / `::: tab` / `:::` syntax to `<rw-tabs>` / `<rw-tab>`
//! elements that pass through pulldown-cmark unchanged.

use super::fence::FenceTracker;

/// Metadata for a single tab within a tab group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabMetadata {
    /// Unique ID for this tab within the document.
    pub id: usize,
    /// Display label for the tab button.
    pub label: String,
    /// Line number where the tab was defined (1-indexed).
    pub line: usize,
}

/// Metadata for a tab group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabsGroup {
    /// Unique ID for this tab group.
    pub id: usize,
    /// Tabs within this group.
    pub tabs: Vec<TabMetadata>,
}

/// Parser state for directive processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Normal markdown processing.
    Normal,
    /// Inside `::: tabs` block, waiting for tab.
    InTabs,
    /// Inside `::: tab` block.
    InTab,
}

/// Preprocessor that converts tab directives to HTML elements.
///
/// Uses a state machine to track nesting and collect metadata:
/// - `::: tabs` → `<rw-tabs data-id="N">`
/// - `::: tab Label` → `<rw-tab data-id="M">` (implicitly closes previous tab)
/// - `:::` (closing) → `</rw-tab></rw-tabs>` (closes tab and container)
///
/// # Example
///
/// ```
/// use rw_renderer::TabsPreprocessor;
///
/// let mut preprocessor = TabsPreprocessor::new();
/// // ::: tab B implicitly closes ::: tab A
/// // Final ::: closes the last tab AND the container
/// let output = preprocessor.process(r#"
/// ::: tabs
/// ::: tab macOS
/// Install with Homebrew.
/// ::: tab Linux
/// Install with apt.
/// :::
/// "#);
///
/// assert!(output.contains("<rw-tabs"));
/// assert!(output.contains("<rw-tab"));
///
/// let groups = preprocessor.into_groups();
/// assert_eq!(groups.len(), 1);
/// assert_eq!(groups[0].tabs.len(), 2);
/// ```
pub struct TabsPreprocessor {
    state: State,
    fence: FenceTracker,
    warnings: Vec<String>,
    groups: Vec<TabsGroup>,
    current_group: Option<TabsGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    tabs_start_line: usize,
}

impl TabsPreprocessor {
    /// Create a new preprocessor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            fence: FenceTracker::new(),
            warnings: Vec::new(),
            groups: Vec::new(),
            current_group: None,
            next_group_id: 0,
            next_tab_id: 0,
            tabs_start_line: 0,
        }
    }

    /// Process markdown text and return transformed output.
    ///
    /// Tab directives are converted to `<rw-tabs>` and `<rw-tab>` elements.
    /// Metadata is collected and can be retrieved with [`into_groups`](Self::into_groups).
    #[must_use]
    pub fn process(&mut self, input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        let lines: Vec<&str> = input.lines().collect();
        let line_count = lines.len();

        for (idx, line) in lines.into_iter().enumerate() {
            let line_num = idx + 1;
            let processed = self.process_line(line, line_num);
            output.push_str(&processed);
            // Preserve line endings
            if idx < line_count - 1 || input.ends_with('\n') {
                output.push('\n');
            }
        }

        // Check for unclosed tabs at end of input
        self.finalize();

        output
    }

    /// Get warnings generated during processing.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Consume the preprocessor and return collected tab groups.
    #[must_use]
    pub fn into_groups(self) -> Vec<TabsGroup> {
        self.groups
    }

    /// Process a single line and return the transformed output.
    fn process_line(&mut self, line: &str, line_num: usize) -> String {
        // Update fence state
        self.fence.update(line);

        // Skip directive processing inside code fences
        if self.fence.in_fence() {
            return line.to_string();
        }

        // Check for directive
        let trimmed = line.trim();
        if let Some(directive) = parse_directive(trimmed) {
            match directive {
                Directive::Tabs => self.handle_tabs(line_num),
                Directive::Tab(label) => self.handle_tab(label, line_num),
                Directive::Close => self.handle_close(line_num),
            }
        } else {
            line.to_string()
        }
    }

    /// Handle `::: tabs` directive.
    fn handle_tabs(&mut self, line_num: usize) -> String {
        match self.state {
            State::Normal => {
                let group_id = self.next_group_id;
                self.next_group_id += 1;
                self.current_group = Some(TabsGroup {
                    id: group_id,
                    tabs: Vec::new(),
                });
                self.tabs_start_line = line_num;
                self.state = State::InTabs;
                // Blank line after opening tag for pulldown-cmark
                format!("<rw-tabs data-id=\"{group_id}\">\n")
            }
            State::InTabs | State::InTab => {
                // Nested tabs not supported
                self.warnings.push(format!(
                    "line {line_num}: nested ::: tabs not supported, passing through"
                ));
                "::: tabs".to_string()
            }
        }
    }

    /// Handle `::: tab Label` directive.
    fn handle_tab(&mut self, label: String, line_num: usize) -> String {
        match self.state {
            State::InTabs => {
                let tab_id = self.next_tab_id;
                self.next_tab_id += 1;

                if let Some(ref mut group) = self.current_group {
                    group.tabs.push(TabMetadata {
                        id: tab_id,
                        label: label.clone(),
                        line: line_num,
                    });
                }

                self.state = State::InTab;
                // Blank line after opening tag for pulldown-cmark
                format!("<rw-tab data-id=\"{tab_id}\">\n")
            }
            State::InTab => {
                // Close previous tab, open new one
                let tab_id = self.next_tab_id;
                self.next_tab_id += 1;

                if let Some(ref mut group) = self.current_group {
                    group.tabs.push(TabMetadata {
                        id: tab_id,
                        label: label.clone(),
                        line: line_num,
                    });
                }

                // Blank lines around tags for pulldown-cmark block parsing
                format!("\n</rw-tab>\n\n<rw-tab data-id=\"{tab_id}\">\n")
            }
            State::Normal => {
                self.warnings.push(format!(
                    "line {line_num}: ::: tab outside ::: tabs, passing through"
                ));
                format!("::: tab {label}")
            }
        }
    }

    /// Handle `:::` closing directive.
    fn handle_close(&mut self, line_num: usize) -> String {
        match self.state {
            State::InTab => {
                // Close tab AND tabs container (per RD-029: "closes everything")
                if let Some(group) = self.current_group.take() {
                    self.groups.push(group);
                }
                self.state = State::Normal;
                // Blank line before closing tags for pulldown-cmark
                "\n</rw-tab>\n</rw-tabs>".to_string()
            }
            State::InTabs => {
                // Close tabs group (empty tabs case)
                if let Some(group) = self.current_group.take() {
                    if group.tabs.is_empty() {
                        self.warnings.push(format!(
                            "line {}: ::: tabs with no tabs, skipping",
                            self.tabs_start_line
                        ));
                        self.state = State::Normal;
                        return String::new();
                    }
                    self.groups.push(group);
                }
                self.state = State::Normal;
                "</rw-tabs>".to_string()
            }
            State::Normal => {
                // Stray closing, warn and pass through
                self.warnings.push(format!(
                    "line {line_num}: stray ::: with no opening directive"
                ));
                ":::".to_string()
            }
        }
    }

    /// Finalize processing and check for unclosed blocks.
    fn finalize(&mut self) {
        match self.state {
            State::InTab => {
                self.warnings.push(format!(
                    "line {}: unclosed ::: tabs (missing closing :::)",
                    self.tabs_start_line
                ));
            }
            State::InTabs => {
                self.warnings.push(format!(
                    "line {}: unclosed ::: tabs (missing closing :::)",
                    self.tabs_start_line
                ));
            }
            State::Normal => {}
        }
    }
}

impl Default for TabsPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed directive type.
#[derive(Debug, PartialEq, Eq)]
enum Directive {
    Tabs,
    Tab(String),
    Close,
}

/// Parse a trimmed line for directive syntax.
fn parse_directive(trimmed: &str) -> Option<Directive> {
    if !trimmed.starts_with(":::") {
        return None;
    }

    let rest = trimmed[3..].trim();

    if rest.is_empty() {
        return Some(Directive::Close);
    }

    if rest == "tabs" || rest.starts_with("tabs ") {
        return Some(Directive::Tabs);
    }

    if rest.starts_with("tab ") {
        let label = rest[4..].trim();
        if label.is_empty() {
            return Some(Directive::Tab("Tab".to_string()));
        }
        // Strip surrounding quotes if present
        let label = strip_quotes(label);
        return Some(Directive::Tab(label.to_string()));
    }

    if rest == "tab" {
        return Some(Directive::Tab("Tab".to_string()));
    }

    // Unknown directive - not a tabs directive
    None
}

/// Strip surrounding quotes (single or double) from a string.
fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            return &s[1..s.len() - 1];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_directive_tabs() {
        assert_eq!(parse_directive("::: tabs"), Some(Directive::Tabs));
        assert_eq!(parse_directive(":::tabs"), Some(Directive::Tabs));
        assert_eq!(parse_directive("::: tabs "), Some(Directive::Tabs));
    }

    #[test]
    fn test_parse_directive_tab() {
        assert_eq!(
            parse_directive("::: tab macOS"),
            Some(Directive::Tab("macOS".to_string()))
        );
        assert_eq!(
            parse_directive("::: tab Linux"),
            Some(Directive::Tab("Linux".to_string()))
        );
        assert_eq!(
            parse_directive("::: tab"),
            Some(Directive::Tab("Tab".to_string()))
        );
        assert_eq!(
            parse_directive("::: tab  "),
            Some(Directive::Tab("Tab".to_string()))
        );
    }

    #[test]
    fn test_parse_directive_tab_with_quotes() {
        // Double quotes
        assert_eq!(
            parse_directive(r#"::: tab "macOS и Linux""#),
            Some(Directive::Tab("macOS и Linux".to_string()))
        );
        // Single quotes
        assert_eq!(
            parse_directive("::: tab 'Windows'"),
            Some(Directive::Tab("Windows".to_string()))
        );
        // No quotes
        assert_eq!(
            parse_directive("::: tab Plain Label"),
            Some(Directive::Tab("Plain Label".to_string()))
        );
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes(r#""quoted""#), "quoted");
        assert_eq!(strip_quotes("'single'"), "single");
        assert_eq!(strip_quotes("no quotes"), "no quotes");
        assert_eq!(strip_quotes(r#""mismatched'"#), r#""mismatched'"#);
        assert_eq!(strip_quotes(r#""""#), ""); // Empty quoted string
        assert_eq!(strip_quotes(""), ""); // Empty string
    }

    #[test]
    fn test_parse_directive_close() {
        assert_eq!(parse_directive(":::"), Some(Directive::Close));
        assert_eq!(parse_directive("::: "), Some(Directive::Close));
    }

    #[test]
    fn test_parse_directive_unknown() {
        assert_eq!(parse_directive("::: note"), None);
        assert_eq!(parse_directive("::: warning"), None);
        assert_eq!(parse_directive("```rust"), None);
        assert_eq!(parse_directive("regular text"), None);
    }

    #[test]
    fn test_simple_tabs() {
        let mut pp = TabsPreprocessor::new();
        // Use implicit tab closing: ::: tab B implicitly closes ::: tab A
        // Final ::: closes the last tab AND the container
        let output = pp.process(
            r#"::: tabs
::: tab macOS
Install with Homebrew.
::: tab Linux
Install with apt.
:::"#,
        );

        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
        assert!(output.contains(r#"<rw-tab data-id="0">"#));
        assert!(output.contains(r#"<rw-tab data-id="1">"#));
        assert!(output.contains("</rw-tab>"));
        assert!(output.contains("</rw-tabs>"));
        assert!(output.contains("Install with Homebrew."));
        assert!(output.contains("Install with apt."));

        let groups = pp.into_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, 0);
        assert_eq!(groups[0].tabs.len(), 2);
        assert_eq!(groups[0].tabs[0].label, "macOS");
        assert_eq!(groups[0].tabs[1].label, "Linux");
    }

    #[test]
    fn test_tabs_with_code_block() {
        let mut pp = TabsPreprocessor::new();
        // Single ::: closes both the tab and the container
        let output = pp.process(
            r#"::: tabs
::: tab Example

```python
::: not a directive
print("hello")
```

:::"#,
        );

        // Code block content should not be transformed
        assert!(output.contains("::: not a directive"));
        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
    }

    #[test]
    fn test_nested_tabs_warning() {
        let mut pp = TabsPreprocessor::new();
        // ::: inside tab closes everything, so nested ::: tabs becomes orphan
        let output = pp.process(
            r#"::: tabs
::: tab First
::: tabs
:::"#,
        );

        assert!(pp.warnings().iter().any(|w| w.contains("nested")));
        // Inner ::: tabs should pass through as literal
        assert!(output.contains("::: tabs"));
    }

    #[test]
    fn test_tab_outside_tabs_warning() {
        let mut pp = TabsPreprocessor::new();
        let output = pp.process("::: tab Orphan\nContent\n:::");

        assert!(pp.warnings().iter().any(|w| w.contains("outside")));
        // Should pass through as literal
        assert!(output.contains("::: tab Orphan"));
    }

    #[test]
    fn test_empty_tabs_warning() {
        let mut pp = TabsPreprocessor::new();
        let _output = pp.process("::: tabs\n:::");

        assert!(pp.warnings().iter().any(|w| w.contains("no tabs")));
        let groups = pp.into_groups();
        assert!(groups.is_empty());
    }

    #[test]
    fn test_unclosed_tabs_warning() {
        let mut pp = TabsPreprocessor::new();
        let _output = pp.process("::: tabs\n::: tab Test\nContent");

        assert!(pp.warnings().iter().any(|w| w.contains("unclosed")));
    }

    #[test]
    fn test_stray_close_warning() {
        let mut pp = TabsPreprocessor::new();
        let output = pp.process(":::");

        assert!(pp.warnings().iter().any(|w| w.contains("stray")));
        // Should pass through
        assert!(output.trim() == ":::");
    }

    #[test]
    fn test_multiple_tab_groups() {
        let mut pp = TabsPreprocessor::new();
        // Single ::: closes both tab and container
        let output = pp.process(
            r#"::: tabs
::: tab A
Content A
:::

Some text between.

::: tabs
::: tab B
Content B
:::"#,
        );

        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
        assert!(output.contains(r#"<rw-tabs data-id="1">"#));

        let groups = pp.into_groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, 0);
        assert_eq!(groups[1].id, 1);
    }

    #[test]
    fn test_tab_without_label() {
        let mut pp = TabsPreprocessor::new();
        // Single ::: closes both tab and container
        let _output = pp.process(
            r#"::: tabs
::: tab
Content
:::"#,
        );

        let groups = pp.into_groups();
        assert_eq!(groups[0].tabs[0].label, "Tab");
    }

    #[test]
    fn test_preserves_line_endings() {
        let mut pp = TabsPreprocessor::new();
        let input = "Line 1\nLine 2\n";
        let output = pp.process(input);

        assert_eq!(output, input);
    }

    #[test]
    fn test_preserves_content_inside_tabs() {
        let mut pp = TabsPreprocessor::new();
        // Single ::: closes both tab and container
        let output = pp.process(
            r#"::: tabs
::: tab Test

# Heading

- List item
- Another item

```rust
fn main() {}
```

:::"#,
        );

        assert!(output.contains("# Heading"));
        assert!(output.contains("- List item"));
        assert!(output.contains("fn main() {}"));
    }

    #[test]
    fn test_tilde_fence_skip() {
        let mut pp = TabsPreprocessor::new();
        let output = pp.process(
            r#"~~~
::: tabs
~~~"#,
        );

        // Should not parse ::: tabs inside fence
        assert!(!output.contains("<rw-tabs"));
        assert!(output.contains("::: tabs"));
    }

    #[test]
    fn test_default() {
        let pp = TabsPreprocessor::default();
        assert!(pp.warnings().is_empty());
        assert!(pp.into_groups().is_empty());
    }
}
