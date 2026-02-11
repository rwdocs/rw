//! Tabs container directive.
//!
//! Implements `ContainerDirective` for tabbed content blocks.

use std::fmt::Write;

use crate::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Replacements,
};
use crate::state::escape_html;

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

/// Container directive for tabbed content blocks.
///
/// Converts `:::tab[Label]` syntax to accessible tabbed HTML.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::DirectiveProcessor;
/// use rw_renderer::TabsDirective;
///
/// let mut processor = DirectiveProcessor::new()
///     .with_container(TabsDirective::new());
///
/// let input = r#":::tab[macOS]
/// Install with Homebrew.
/// :::tab[Linux]
/// Install with apt.
/// :::"#;
///
/// let output = processor.process(input);
/// assert!(output.contains(r#"<rw-tabs data-id="0">"#));
///
/// let mut html = output.clone();
/// processor.post_process(&mut html);
/// assert!(html.contains(r#"role="tablist""#));
/// ```
pub struct TabsDirective {
    groups: Vec<TabsGroup>,
    current_group: Option<TabsGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    /// Stack to track nested tabs (`group_start_line`).
    stack: Vec<usize>,
    warnings: Vec<String>,
}

impl TabsDirective {
    /// Create a new tabs directive handler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            current_group: None,
            next_group_id: 0,
            next_tab_id: 0,
            stack: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Consume the directive and return collected tab groups.
    #[must_use]
    pub fn into_groups(self) -> Vec<TabsGroup> {
        self.groups
    }
}

impl Default for TabsDirective {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerDirective for TabsDirective {
    fn name(&self) -> &'static str {
        "tab"
    }

    fn start(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput {
        let label = if args.content.is_empty() {
            "Tab".to_owned()
        } else {
            strip_quotes(&args.content).to_owned()
        };

        if self.stack.is_empty() {
            // Start new group and first tab
            let group_id = self.next_group_id;
            self.next_group_id += 1;
            let tab_id = self.next_tab_id;
            self.next_tab_id += 1;

            self.current_group = Some(TabsGroup {
                id: group_id,
                tabs: vec![TabMetadata {
                    id: tab_id,
                    label,
                    line: ctx.line,
                }],
            });
            self.stack.push(ctx.line);

            // Blank line after opening tags for pulldown-cmark
            DirectiveOutput::html(format!(
                "<rw-tabs data-id=\"{group_id}\">\n\n<rw-tab data-id=\"{tab_id}\">\n"
            ))
        } else {
            // Close previous tab, open new one in same group
            let tab_id = self.next_tab_id;
            self.next_tab_id += 1;

            if let Some(ref mut group) = self.current_group {
                group.tabs.push(TabMetadata {
                    id: tab_id,
                    label,
                    line: ctx.line,
                });
            }

            // Blank lines around tags for pulldown-cmark block parsing
            DirectiveOutput::html(format!("\n</rw-tab>\n\n<rw-tab data-id=\"{tab_id}\">\n"))
        }
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        if self.stack.pop().is_some() {
            // Close tab AND tabs container
            if let Some(group) = self.current_group.take() {
                self.groups.push(group);
            }
            // Blank line before closing tags for pulldown-cmark
            Some("\n</rw-tab>\n</rw-tabs>".to_owned())
        } else {
            // Should not happen if DirectiveProcessor is correct
            None
        }
    }

    fn post_process(&mut self, replacements: &mut Replacements) {
        for group in &self.groups {
            // Replace <rw-tabs data-id="N"> with accessible HTML
            let opening = render_tabs_open(group);
            replacements.add(format!(r#"<rw-tabs data-id="{}">"#, group.id), opening);

            // Replace each <rw-tab data-id="N"> with panel HTML
            for (idx, tab) in group.tabs.iter().enumerate() {
                let panel_open = render_panel_open(group.id, tab, idx == 0);
                replacements.add(format!(r#"<rw-tab data-id="{}">"#, tab.id), panel_open);
            }
        }

        // Replace common closing tags (if any groups were processed)
        if !self.groups.is_empty() {
            replacements.add("</rw-tab>", "</div>");
            replacements.add("</rw-tabs>", "</div>");
        }
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Render the opening HTML for a tabs container.
fn render_tabs_open(group: &TabsGroup) -> String {
    let mut output = String::with_capacity(512);

    // Container div
    let _ = write!(output, r#"<div class="tabs" id="tabs-{}">"#, group.id);

    // Tab buttons
    output.push_str(r#"<div class="tabs-buttons" role="tablist">"#);
    for (idx, tab) in group.tabs.iter().enumerate() {
        let selected = idx == 0;
        let tab_id = format!("tab-{}-{}", group.id, tab.id);
        let panel_id = format!("panel-{}-{}", group.id, tab.id);

        let _ = write!(
            output,
            r#"<button role="tab" id="{tab_id}" aria-controls="{panel_id}" aria-selected="{selected}" tabindex="{}">{}</button>"#,
            if selected { "0" } else { "-1" },
            escape_html(&tab.label)
        );
    }
    output.push_str("</div>");

    output
}

/// Render the opening HTML for a tab panel.
fn render_panel_open(group_id: usize, tab: &TabMetadata, is_first: bool) -> String {
    let hidden = if is_first { "" } else { " hidden" };
    let tab_id = format!("tab-{}-{}", group_id, tab.id);
    let panel_id = format!("panel-{}-{}", group_id, tab.id);

    format!(r#"<div role="tabpanel" id="{panel_id}" aria-labelledby="{tab_id}"{hidden}>"#)
}

/// Strip surrounding quotes (single or double) from a string.
fn strip_quotes(s: &str) -> &str {
    let is_quoted =
        (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''));
    if is_quoted && s.len() >= 2 {
        return &s[1..s.len() - 1];
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::DirectiveProcessor;

    #[test]
    fn test_simple_tabs() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r":::tab[macOS]
Install with Homebrew.
:::tab[Linux]
Install with apt.
:::";

        let output = processor.process(input);

        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
        assert!(output.contains(r#"<rw-tab data-id="0">"#));
        assert!(output.contains(r#"<rw-tab data-id="1">"#));
        assert!(output.contains("</rw-tab>"));
        assert!(output.contains("</rw-tabs>"));
        assert!(output.contains("Install with Homebrew."));
        assert!(output.contains("Install with apt."));
    }

    #[test]
    fn test_post_process() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r":::tab[macOS]
Content A
:::tab[Linux]
Content B
:::";

        let output = processor.process(input);
        let mut html = output;
        processor.post_process(&mut html);

        // Check accessible HTML structure
        assert!(html.contains(r#"<div class="tabs" id="tabs-0">"#));
        assert!(html.contains(r#"role="tablist""#));
        assert!(html.contains(r#"role="tab""#));
        assert!(html.contains(r#"role="tabpanel""#));
        assert!(html.contains(r#"aria-selected="true""#));
        assert!(html.contains(r#"aria-selected="false""#));
        assert!(html.contains(" hidden>"));

        // Check custom elements are replaced
        assert!(!html.contains("<rw-tabs"));
        assert!(!html.contains("<rw-tab"));
    }

    #[test]
    fn test_tabs_with_code_fence() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r#":::tab[Example]

```python
:::tab inside code
print("hello")
```

:::"#;

        let output = processor.process(input);

        // Code block content should not be transformed
        assert!(output.contains(":::tab inside code"));
        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
    }

    #[test]
    fn test_tab_without_label() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r":::tab
Content
:::";

        let output = processor.process(input);
        let mut html = output;
        processor.post_process(&mut html);

        assert!(html.contains(">Tab</button>"));
    }

    #[test]
    fn test_quoted_label() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r#":::tab["macOS и Linux"]
Content
:::"#;

        let output = processor.process(input);
        let mut html = output;
        processor.post_process(&mut html);

        assert!(html.contains(">macOS и Linux</button>"));
    }

    #[test]
    fn test_multiple_tab_groups() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r":::tab[A]
Content A
:::

Some text between.

:::tab[B]
Content B
:::";

        let output = processor.process(input);

        assert!(output.contains(r#"<rw-tabs data-id="0">"#));
        assert!(output.contains(r#"<rw-tabs data-id="1">"#));
    }

    #[test]
    fn test_html_escaping() {
        let mut processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let input = r":::tab[<script>]
Content
:::";

        let output = processor.process(input);
        let mut html = output;
        processor.post_process(&mut html);

        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("><script>"));
    }
}
