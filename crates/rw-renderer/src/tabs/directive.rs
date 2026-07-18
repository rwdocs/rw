//! Tabs container directive.
//!
//! Implements `ContainerDirective` for tabbed content blocks.

use std::borrow::Cow;
use std::fmt::Write;

use crate::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Fills, HoleKey, Part,
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
    /// Hole reserved for this tab's panel opening.
    panel_hole: HoleKey,
}

/// Metadata for a tab group.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TabsGroup {
    /// Unique ID for this tab group.
    pub(crate) id: usize,
    /// Tabs within this group.
    pub(crate) tabs: Vec<TabMetadata>,
    /// Hole reserved for this group's tab bar.
    bar_hole: HoleKey,
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
    /// Whether a tab group is currently open, as a 0-or-1 entry holding the
    /// group's start line. Tabs do not nest: the first `:::tab` pushes, every
    /// subsequent `:::tab` continues the same group without pushing, and the
    /// single closing `:::` pops. So this never grows past depth 1 and cannot,
    /// on its own, distinguish "opened a new group" from "continued one" —
    /// `last_start_opened` carries that distinction.
    stack: Vec<usize>,
    /// Whether the most recent `start()` opened a new tab group (vs. continued
    /// an existing one with another `:::tab`). Read by `opened_scope()`.
    last_start_opened: bool,
    /// Next hole key. Separate from `next_group_id`/`next_tab_id`, which are
    /// two independent sequences and would collide as keys.
    next_hole_key: HoleKey,
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
            last_start_opened: false,
            next_hole_key: 0,
            warnings: Vec::new(),
        }
    }

    /// Reserve the next hole key for this directive.
    fn next_hole(&mut self) -> HoleKey {
        let key = self.next_hole_key;
        self.next_hole_key += 1;
        key
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
            // The bar needs every tab's label, so it cannot be rendered until
            // the walk has passed the group's closing `:::`.
            let bar_hole = self.next_hole();
            let panel_hole = self.next_hole();

            self.current_group = Some(TabsGroup {
                id: group_id,
                tabs: vec![TabMetadata {
                    id: tab_id,
                    label,
                    line: ctx.line(),
                    panel_hole,
                }],
                bar_hole,
            });
            self.stack.push(ctx.line());
            self.last_start_opened = true;

            DirectiveOutput::Deferred(vec![Part::Hole(bar_hole), Part::Hole(panel_hole)])
        } else {
            // Close previous tab, open new one in same group
            let tab_id = self.next_tab_id;
            self.next_tab_id += 1;
            let panel_hole = self.next_hole();

            let Some(group) = self.current_group.as_mut() else {
                unreachable!(
                    "tabs invariant violated: a non-empty `stack` must imply `current_group` is Some, \
                     otherwise `panel_hole` is emitted with nothing to fill it"
                )
            };
            group.tabs.push(TabMetadata {
                id: tab_id,
                label,
                line: ctx.line(),
                panel_hole,
            });

            self.last_start_opened = false;

            // Closes the previous panel, then opens this one.
            DirectiveOutput::Deferred(vec![
                Part::Html(Cow::Borrowed("</div>")),
                Part::Hole(panel_hole),
            ])
        }
    }

    fn opened_scope(&self) -> bool {
        self.last_start_opened
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        if self.stack.pop().is_some() {
            if let Some(group) = self.current_group.take() {
                self.groups.push(group);
            }
            // Closes the last panel, then the tabs container.
            Some("</div></div>".to_owned())
        } else {
            // Should not happen if DirectiveProcessor is correct
            None
        }
    }

    fn fills(&mut self, fills: &mut Fills) {
        // Every group has reached `end()` by now — including one whose closing
        // `:::` was missing, which the processor closes at end of input — so
        // `self.groups` holds them all and each reserved hole gets filled.
        debug_assert!(
            self.current_group.is_none(),
            "fills() called with a tab group still open: its holes would go unfilled"
        );

        for group in &self.groups {
            fills.set(group.bar_hole, render_tabs_open(group));

            for (idx, tab) in group.tabs.iter().enumerate() {
                fills.set(tab.panel_hole, render_panel_open(group.id, tab, idx == 0));
            }
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
    use crate::{HtmlBackend, MarkdownRenderer, Pipeline};

    #[test]
    fn unclosed_group_still_closes_its_divs() {
        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        // No closing `:::` — the group runs to the end of the document.
        let result = renderer.render(
            ":::tab[A]\n\nA\n\n:::tab[B]\n\nB\n\nAFTER\n",
            Pipeline::new().with_directives(processor),
        );

        let opens = result.html.matches("<div").count();
        let closes = result.html.matches("</div>").count();
        assert_eq!(
            opens, closes,
            "unbalanced divs ({opens} open, {closes} close): {}",
            result.html
        );
        assert!(
            result.html.contains("AFTER"),
            "content after the unclosed group vanished: {}",
            result.html
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("unclosed container directive :::tab")),
            "expected the unclosed-container warning: {:?}",
            result.warnings
        );
    }
}
