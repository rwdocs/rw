//! Tabbed content: an outer `::::tabs` group container wrapping self-closing
//! `:::tab[Label]` items — the `CommonMark` generic-directives nested shape.
//!
//! Each `:::tab` is an ordinary balanced container: its `start` emits the
//! panel's opening `<div role="tabpanel">`, its `end` the closing `</div>`. The
//! group's tab bar can only be rendered once every tab's label is known (after
//! the group's closing `::::`), so `::::tabs` reserves a *hole* for the bar and
//! [`fills`](ContainerDirective::fills) supplies it after the walk.

use std::fmt::Write;

use crate::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, Fills, HoleKey, Part,
};
use crate::util::escape_html;

/// One tab within a group.
struct TabMetadata {
    /// Document-global tab id (used in element ids).
    id: usize,
    /// Display label from `:::tab[Label]`.
    label: String,
}

/// A tab group: its id, its tabs, and the reserved tab-bar hole.
struct TabsGroup {
    id: usize,
    tabs: Vec<TabMetadata>,
    bar_hole: HoleKey,
}

/// What an entry on the internal scope stack represents, so [`end`] can tell
/// what it is closing without being told the directive name.
///
/// [`end`]: ContainerDirective::end
enum Scope {
    /// The `::::tabs` group container.
    Group,
    /// A `:::tab` item inside an open group.
    Item,
    /// A `:::tab` with no enclosing `::::tabs` (content rendered unwrapped).
    LoneItem,
}

/// Tabbed-content directive: handles the `tabs` group container and its nested
/// `tab` items (see [`matches`](ContainerDirective::matches)).
///
/// # Example
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
/// use rw_renderer::directive::DirectiveProcessor;
/// use rw_renderer::TabsDirective;
///
/// let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
/// let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .render(md, Pipeline::new().with_directives(directives));
/// assert!(result.html.contains(r#"role="tablist""#));
/// ```
pub struct TabsDirective {
    /// Completed groups awaiting their bar-hole fill.
    groups: Vec<TabsGroup>,
    /// Stack of groups currently open, innermost last. A `::::tabs` nested
    /// inside a `:::tab` panel of another open group pushes a second entry
    /// rather than overwriting the first, so the outer group's reserved bar
    /// hole is never dropped.
    open: Vec<TabsGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    next_hole_key: HoleKey,
    /// One entry per open `tabs`/`tab` scope, so `end()` (which gets no name)
    /// knows what it closes. Stays in lockstep with the processor's frame stack
    /// for this handler.
    scope_stack: Vec<Scope>,
    warnings: Vec<String>,
}

impl TabsDirective {
    /// Create a new tabs directive handler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            open: Vec::new(),
            next_group_id: 0,
            next_tab_id: 0,
            next_hole_key: 0,
            scope_stack: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn next_hole(&mut self) -> HoleKey {
        let key = self.next_hole_key;
        self.next_hole_key += 1;
        key
    }

    /// Open a `::::tabs` group: reserve its deferred tab-bar hole.
    ///
    /// Opens a new group; see [`open`](Self::open) for why this is a stack.
    fn open_group(&mut self) -> DirectiveOutput {
        let id = self.next_group_id;
        self.next_group_id += 1;
        let bar_hole = self.next_hole();
        self.open.push(TabsGroup {
            id,
            tabs: Vec::new(),
            bar_hole,
        });
        self.scope_stack.push(Scope::Group);
        DirectiveOutput::Deferred(vec![Part::Hole(bar_hole)])
    }

    /// Open a `:::tab[Label]` item: emit its panel opening inline.
    fn open_item(&mut self, args: &DirectiveArgs) -> DirectiveOutput {
        let label = if args.content().is_empty() {
            "Tab".to_owned()
        } else {
            strip_quotes(args.content()).to_owned()
        };

        let Some(group) = self.open.last_mut() else {
            // `:::tab` outside any `::::tabs`: no bar to join. Render its content
            // unwrapped and warn, rather than leaking chrome or literal syntax.
            self.warnings.push(
                "`:::tab` outside a `::::tabs` group; its content is rendered without tab chrome"
                    .to_owned(),
            );
            self.scope_stack.push(Scope::LoneItem);
            return DirectiveOutput::Html(String::new());
        };

        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        let is_first = group.tabs.is_empty();
        let group_id = group.id;
        group.tabs.push(TabMetadata { id: tab_id, label });
        self.scope_stack.push(Scope::Item);
        DirectiveOutput::Html(render_panel_open(group_id, tab_id, is_first))
    }
}

impl Default for TabsDirective {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerDirective for TabsDirective {
    fn name(&self) -> &'static str {
        "tabs"
    }

    fn matches(&self, name: &str) -> bool {
        name == "tabs" || name == "tab"
    }

    fn start(&mut self, args: DirectiveArgs, ctx: &DirectiveContext) -> DirectiveOutput {
        // The processor always dispatches via `start_named`; a bare `start`
        // (e.g. a direct unit-test call) is treated as opening the group.
        self.start_named("tabs", args, ctx)
    }

    fn start_named(
        &mut self,
        name: &str,
        args: DirectiveArgs,
        _ctx: &DirectiveContext,
    ) -> DirectiveOutput {
        if name == "tabs" {
            self.open_group()
        } else {
            self.open_item(&args)
        }
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        match self.scope_stack.pop() {
            Some(Scope::Item) => Some("</div>".to_owned()),
            Some(Scope::Group) => {
                if let Some(group) = self.open.pop() {
                    if group.tabs.is_empty() {
                        self.warnings
                            .push("`::::tabs` group has no `:::tab` items".to_owned());
                    }
                    self.groups.push(group);
                }
                Some("</div>".to_owned())
            }
            // LoneItem: open_item() emitted no opening tag, so nothing to
            // close. None: unreachable (a matching start() always precedes
            // end()); a safe no-op.
            Some(Scope::LoneItem) | None => None,
        }
    }

    fn fills(&mut self, fills: &mut Fills) {
        debug_assert!(
            self.open.is_empty(),
            "fills() called with a tab group still open: its bar hole would go unfilled"
        );
        for group in &self.groups {
            fills.set(group.bar_hole, render_tabs_open(group));
        }
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Render the opening HTML for a tabs container (container div + tab bar).
fn render_tabs_open(group: &TabsGroup) -> String {
    let mut output = String::with_capacity(512);
    let _ = write!(output, r#"<div class="tabs" id="tabs-{}">"#, group.id);
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

/// Render the opening HTML for a single tab panel.
fn render_panel_open(group_id: usize, tab_id: usize, is_first: bool) -> String {
    let hidden = if is_first { "" } else { " hidden" };
    let tab_el = format!("tab-{group_id}-{tab_id}");
    let panel_el = format!("panel-{group_id}-{tab_id}");
    format!(r#"<div role="tabpanel" id="{panel_el}" aria-labelledby="{tab_el}"{hidden}>"#)
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

    fn render(md: &str) -> crate::RenderResult {
        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());
        MarkdownRenderer::<HtmlBackend>::new()
            .render(md, Pipeline::new().with_directives(processor))
    }

    #[test]
    fn nested_group_renders_bar_and_panels() {
        let result =
            render("::::tabs\n\n:::tab[A]\n\nA body\n\n:::\n\n:::tab[B]\n\nB body\n\n:::\n\n::::");
        assert!(
            result.html.contains(r#"id="tabs-0""#),
            "got: {}",
            result.html
        );
        assert!(
            result.html.contains(r#"role="tablist""#),
            "got: {}",
            result.html
        );
        assert!(
            result.html.contains(r#"id="panel-0-0""#),
            "got: {}",
            result.html
        );
        assert!(
            result.html.contains(r#"id="panel-0-1""#),
            "got: {}",
            result.html
        );
        let opens = result.html.matches("<div").count();
        let closes = result.html.matches("</div>").count();
        assert_eq!(opens, closes, "unbalanced divs: {}", result.html);
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn unclosed_group_still_balances_and_warns() {
        // No closing `::::` — the group runs to end of document.
        let result = render("::::tabs\n\n:::tab[A]\n\nA\n\n:::\n\n:::tab[B]\n\nB\n\nAFTER\n");
        let opens = result.html.matches("<div").count();
        let closes = result.html.matches("</div>").count();
        assert_eq!(opens, closes, "unbalanced divs: {}", result.html);
        assert!(
            result.html.contains("AFTER"),
            "content vanished: {}",
            result.html
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("unclosed container directive")),
            "expected an unclosed-container warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn lone_tab_without_group_renders_unwrapped_and_warns() {
        let result = render(":::tab[X]\n\nbody\n\n:::");
        assert!(
            !result.html.contains(r#"role="tablist""#),
            "chrome leaked: {}",
            result.html
        );
        assert!(
            !result.html.contains(":::tab"),
            "literal syntax leaked: {}",
            result.html
        );
        assert!(
            result.html.contains("body"),
            "content lost: {}",
            result.html
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("outside a `::::tabs`")),
            "expected a lone-tab warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn label_is_html_escaped_and_quotes_stripped() {
        let r1 = render("::::tabs\n\n:::tab[a < b & c]\n\nx\n\n:::\n\n::::");
        assert!(r1.html.contains("a &lt; b &amp; c"), "got: {}", r1.html);
        let r2 = render("::::tabs\n\n:::tab[\"quoted\"]\n\nx\n\n:::\n\n::::");
        assert!(
            r2.html.contains(r">quoted</button>"),
            "quotes not stripped: {}",
            r2.html
        );
    }

    #[test]
    fn empty_group_renders_bar_without_buttons_and_warns() {
        let result = render("::::tabs\n\n::::");
        assert!(
            result.html.contains(r#"role="tablist""#),
            "got: {}",
            result.html
        );
        assert!(
            !result.html.contains("<button"),
            "unexpected button: {}",
            result.html
        );
        let opens = result.html.matches("<div").count();
        let closes = result.html.matches("</div>").count();
        assert_eq!(opens, closes, "unbalanced divs: {}", result.html);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("no `:::tab` items")),
            "expected an empty-group warning: {:?}",
            result.warnings
        );
    }
}
