//! Tabs container directive.
//!
//! Implements `ContainerDirective` for tabbed content blocks.

use std::fmt::Write;

use crate::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Replacements,
};
use crate::util::escape_html;

/// Metadata for a single tab within a tab group.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TabMetadata {
    /// Unique ID for this tab within the document.
    pub(crate) id: usize,
    /// Display label for the tab button.
    pub(crate) label: String,
    /// Line number where the tab was defined (1-indexed).
    pub(crate) line: usize,
}

/// Metadata for a tab group.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TabsGroup {
    /// Unique ID for this tab group.
    pub(crate) id: usize,
    /// Tabs within this group.
    pub(crate) tabs: Vec<TabMetadata>,
}

/// Container directive for tabbed content blocks.
///
/// Converts `:::tab[Label]` syntax to accessible tabbed HTML.
///
/// # Example
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
/// use rw_renderer::directive::DirectiveProcessor;
/// use rw_renderer::TabsDirective;
///
/// let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
/// let md = ":::tab[macOS]\n\nInstall with Homebrew.\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::";
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .render(md, Pipeline::new().with_directives(directives));
/// assert!(result.html.contains(r#"role="tablist""#));
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
        let label = if args.content().is_empty() {
            "Tab".to_owned()
        } else {
            strip_quotes(args.content()).to_owned()
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
                    line: ctx.line(),
                }],
            });
            self.stack.push(ctx.line());

            DirectiveOutput::html(format!(
                "<rw-tabs data-id=\"{group_id}\"><rw-tab data-id=\"{tab_id}\">"
            ))
        } else {
            // Close previous tab, open new one in same group
            let tab_id = self.next_tab_id;
            self.next_tab_id += 1;

            if let Some(ref mut group) = self.current_group {
                group.tabs.push(TabMetadata {
                    id: tab_id,
                    label,
                    line: ctx.line(),
                });
            }

            DirectiveOutput::html(format!("</rw-tab><rw-tab data-id=\"{tab_id}\">"))
        }
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        if self.stack.pop().is_some() {
            // Close tab AND tabs container
            if let Some(group) = self.current_group.take() {
                self.groups.push(group);
            }
            Some("</rw-tab></rw-tabs>".to_owned())
        } else {
            // Should not happen if DirectiveProcessor is correct
            None
        }
    }

    fn post_process(&mut self, replacements: &mut Replacements) {
        for group in &self.groups {
            let opening = render_tabs_open(group);
            replacements.add(format!(r#"<rw-tabs data-id="{}">"#, group.id), opening);

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
