//! Stream-native leaf & container directive behavior, driven through the full
//! `MarkdownRenderer` pipeline.

use rw_renderer::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor,
    Fills, InlineDirective, LeafDirective, Part,
};
use rw_renderer::{
    CodeBlockProcessor, FenceAttrs, HtmlBackend, MarkdownRenderer, Pipeline, ProcessResult,
    RenderResult, SearchDocumentBackend, TabsDirective,
};

fn render_tabs(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

/// Assert `needle` lands strictly inside the `open`…`close` pair — i.e. the
/// hole was spliced into the enclosing element, not merely somewhere on the
/// page. Uses the first `open` and the last `close`.
fn assert_between(html: &str, needle: &str, open: &str, close: &str) {
    let at = html
        .find(needle)
        .unwrap_or_else(|| panic!("{needle} not found in: {html}"));
    let open_end = html
        .find(open)
        .unwrap_or_else(|| panic!("{open} not found in: {html}"))
        + open.len();
    let close_at = html
        .rfind(close)
        .unwrap_or_else(|| panic!("{close} not found in: {html}"));

    assert!(
        at >= open_end && at < close_at,
        "{needle} landed outside {open}…{close}: {html}"
    );
}

fn render_status(md: &str) -> RenderResult {
    MarkdownRenderer::<HtmlBackend>::new().render(
        md,
        Pipeline::new().with_directives(DirectiveProcessor::new()),
    )
}

#[test]
fn container_with_block_body_renders() {
    let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("Install with Homebrew."),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("Install with apt."),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p>:::"), "got: {}", result.html);
}

#[test]
fn bracketed_attr_delimiter_is_recognized() {
    let md = "::::tabs\n\n:::tab[Label with spaces]\n\nBody.\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(">Label with spaces</button>"),
        "got: {}",
        result.html
    );
}

#[test]
fn status_inline_alone_on_a_line_still_expands() {
    let result = render_status(":status[Done]{color=green}");
    // The inline directive must expand even when it is alone on a line (i.e. it
    // must not be mistaken for / swallowed by block-directive deferral).
    // `status-green` comes from HtmlBackend::status_open, so the class is the
    // observable proof of expansion.
    assert!(
        result
            .html
            .contains(r#"<span class="status status-green">Done</span>"#),
        "got: {}",
        result.html
    );
}

#[test]
fn delimiter_then_bold_commits_as_normal_paragraph() {
    let result = render_tabs(":::no and **bold**");
    assert!(result.html.contains("<p>"), "got: {}", result.html);
    assert!(
        result.html.contains("<strong>bold</strong>"),
        "got: {}",
        result.html
    );
}

#[test]
fn multi_tab_group_renders_without_spurious_warning() {
    let md = "::::tabs\n\n:::tab[A]\n\nContent A\n\n:::\n\n:::tab[B]\n\nContent B\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"<div class="tabs" id="tabs-0">"#),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"<button role="tab""#),
        "got: {}",
        result.html
    );
    assert!(result.html.contains(">A</button>"), "got: {}", result.html);
    assert!(result.html.contains(">B</button>"), "got: {}", result.html);
    assert!(result.html.contains("Content A"), "got: {}", result.html);
    assert!(result.html.contains("Content B"), "got: {}", result.html);
    // No form of the intermediate marker may survive into the output.
    assert!(!result.html.contains("rw-tab"), "got: {}", result.html);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn three_tab_group_renders_without_spurious_warning() {
    let md = "::::tabs\n\n:::tab[A]\n\nContent A\n\n:::\n\n:::tab[B]\n\nContent B\n\n:::\n\n:::tab[C]\n\nContent C\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(result.html.contains(">A</button>"), "got: {}", result.html);
    assert!(result.html.contains(">B</button>"), "got: {}", result.html);
    assert!(result.html.contains(">C</button>"), "got: {}", result.html);
    assert!(result.html.contains("Content A"), "got: {}", result.html);
    assert!(result.html.contains("Content B"), "got: {}", result.html);
    assert!(result.html.contains("Content C"), "got: {}", result.html);
    assert!(!result.html.contains("<rw-tab"), "got: {}", result.html);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unclosed_nested_tab_group_warns() {
    // The group's own `::::` is missing, so it — and its still-open last `tab`
    // — both run to end of input. Both the group frame and its unclosed last
    // item deterministically warn, so the count is pinned: exactly one
    // `:::tabs (` (group) and exactly one `:::tab (` (item) warning.
    let md = "::::tabs\n\n:::tab[A]\n\nContent A\n\n:::\n\n:::tab[B]\n\nContent B";
    let result = render_tabs(md);
    assert_eq!(
        unclosed_group_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
    assert_eq!(
        unclosed_item_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
}

/// Index of the last `</div>` in `html`, or a panic naming the offender.
fn last_div_close(html: &str) -> usize {
    html.rfind("</div>")
        .unwrap_or_else(|| panic!("no </div> in: {html}"))
}

fn unclosed_warning_count(result: &RenderResult) -> usize {
    result
        .warnings
        .iter()
        .filter(|w| w.contains("unclosed container directive :::tab"))
        .count()
}

/// Count of "unclosed container directive `:::tabs` (…)" warnings — the group
/// frame. Anchored on the trailing space, so it doesn't also count the item
/// warning below (`":::tabs ("` is never a substring match for `":::tab ("`).
fn unclosed_group_warning_count(result: &RenderResult) -> usize {
    result
        .warnings
        .iter()
        .filter(|w| w.contains("unclosed container directive :::tabs ("))
        .count()
}

/// Count of "unclosed container directive `:::tab` (…)" warnings — a single
/// `:::tab` item. `":::tab ("` does not match `":::tabs ("` (the character
/// after `tab` is `s`, not a space), so this cleanly excludes the group
/// warning above.
fn unclosed_item_warning_count(result: &RenderResult) -> usize {
    result
        .warnings
        .iter()
        .filter(|w| w.contains("unclosed container directive :::tab ("))
        .count()
}

#[test]
fn unclosed_tab_inside_blockquote_closes_before_the_blockquote() {
    // A container left open must be closed when its *enclosing block* ends,
    // not at end of input — otherwise its </div>s land after </blockquote>
    // and the nesting is crossed.
    // The tab item closes properly inside the blockquote; the enclosing
    // `::::tabs` group is what's left genuinely unclosed.
    let md = "> intro\n>\n> ::::tabs\n>\n> :::tab[A]\n>\n> body\n>\n> :::\n\nafter\n";
    let result = render_tabs(md);
    let html = &result.html;

    let bq_close = html
        .find("</blockquote>")
        .unwrap_or_else(|| panic!("no </blockquote> in: {html}"));
    assert!(
        last_div_close(html) < bq_close,
        "tab panel closed outside the blockquote: {html}"
    );
    // Content after the blockquote must not be swallowed by the tab panel.
    assert_between(html, "<p>body</p>", "<blockquote>", "</blockquote>");
    assert!(
        html.find("<p>after</p>").expect("after") > bq_close,
        "content after the blockquote leaked inside it: {html}"
    );
    assert_eq!(unclosed_warning_count(&result), 1, "{:?}", result.warnings);
}

#[test]
fn unclosed_tab_inside_list_item_closes_before_the_item() {
    // Same shape as the blockquote case above: the tab item closes; the group
    // is what's left unclosed by the missing `::::`.
    let md = "- item\n\n  ::::tabs\n\n  :::tab[A]\n\n  body\n\n  :::\n\n- second\n";
    let result = render_tabs(md);
    let html = &result.html;

    let ul_close = html
        .find("</ul>")
        .unwrap_or_else(|| panic!("no </ul> in: {html}"));
    let li_close = html
        .find("</li>")
        .unwrap_or_else(|| panic!("no </li> in: {html}"));
    assert!(
        last_div_close(html) < li_close,
        "tab panel closed outside the list item: {html}"
    );
    assert!(
        html.find("<p>second</p>").expect("second") > ul_close.min(li_close),
        "the second item leaked into the tab panel: {html}"
    );
    assert_eq!(unclosed_warning_count(&result), 1, "{:?}", result.warnings);
}

#[test]
fn unclosed_tab_at_top_level_still_closes_at_end_of_input() {
    // Top-level containers have no enclosing block, so end-of-input closing
    // stays correct: trailing content belongs to the open panel. Both the
    // group and its last (only) tab are left open here, matching the original
    // input's total absence of any closing colon, so trailing content stays
    // nested inside both until end of input closes them together.
    let md = "::::tabs\n\n:::tab[A]\n\nA\n\nAFTER\n";
    let result = render_tabs(md);
    let html = &result.html;

    assert!(html.ends_with("</div>"), "got: {html}");
    assert!(
        html.contains("<p>AFTER</p></div></div>"),
        "trailing content left the open panel: {html}"
    );
    // Both the group and its last item are left open, and each warns exactly
    // once — see `unclosed_nested_tab_group_warns`.
    assert_eq!(
        unclosed_group_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
    assert_eq!(
        unclosed_item_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
}

#[test]
fn closing_delimiter_after_the_enclosing_blockquote_does_not_close_twice() {
    // The container was already balanced at `</blockquote>`, so the stray `:::`
    // outside it must not reach the handler's `end()` a second time — a double
    // close would emit the tab group twice. Neither the group nor the item
    // closes inside the blockquote, so both are force-closed at the blockquote
    // boundary and the lone trailing `:::` is a genuine stray.
    let md = "> ::::tabs\n>\n> :::tab[A]\n>\n> body\n\n:::\n\nafter\n";
    let result = render_tabs(md);
    let html = &result.html;

    assert_eq!(
        html.matches(r#"role="tablist""#).count(),
        1,
        "the tab group was emitted twice: {html}"
    );
    assert_eq!(
        html.matches("<div").count(),
        html.matches("</div>").count(),
        "unbalanced divs: {html}"
    );
    // Neither the group nor the item closes inside the blockquote, so both
    // are force-closed at the blockquote boundary — one warning each, plus
    // the unrelated stray-`:::` warning for the trailing colon (not asserted
    // here; see the test's own comment above).
    assert_eq!(
        unclosed_group_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
    assert_eq!(
        unclosed_item_warning_count(&result),
        1,
        "got: {:?}",
        result.warnings
    );
}

#[test]
fn closed_tab_group_inside_blockquote_is_unaffected() {
    let md = "> intro\n>\n> ::::tabs\n>\n> :::tab[A]\n>\n> body\n>\n> :::\n>\n> ::::\n>\n> tail\n\nafter\n";
    let result = render_tabs(md);
    let html = &result.html;

    assert_between(html, "<p>body</p>", "<blockquote>", "</blockquote>");
    assert!(
        html.contains("<p>tail</p></blockquote>"),
        "trailing blockquote content misplaced: {html}"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn closed_tab_group_inside_list_item_is_unaffected() {
    let md =
        "- item\n\n  ::::tabs\n\n  :::tab[A]\n\n  body\n\n  :::\n\n  ::::\n\n  tail\n\n- second\n";
    let result = render_tabs(md);
    let html = &result.html;

    assert_between(html, "<p>body</p>", "<li>", "</li>");
    assert!(
        html.contains("<p>tail</p></li>"),
        "trailing item content misplaced: {html}"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn two_separate_closed_tab_groups_emit_no_warnings() {
    // Two independent closed groups in one document. The second group's
    // `::::tabs` must open a fresh scope (group state resets after the first
    // group closes), not be mistaken for a continuation of the first.
    let md = "::::tabs\n\n:::tab[A]\n\nx\n\n:::\n\n:::tab[B]\n\ny\n\n:::\n\n::::\n\n\
              between\n\n\
              ::::tabs\n\n:::tab[C]\n\nz\n\n:::\n\n:::tab[D]\n\nw\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert_eq!(
        result.html.matches(r#"role="tablist""#).count(),
        2,
        "got: {}",
        result.html
    );
    assert!(result.html.contains("between"), "got: {}", result.html);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn tabs_html_has_no_stray_blank_runs() {
    let md = "::::tabs\n\n:::tab[A]\n\nBody.\n\n:::\n\n::::";
    let result = render_tabs(md);
    // The tab panel sits flush against the body paragraph. Asserts the
    // whitespace invariant (no stray newline before the body <p>) without
    // coupling to the panel's generated id.
    assert!(result.html.contains("<p>Body."), "got: {}", result.html);
    assert!(
        !result.html.contains("\n<p>Body."),
        "stray newline before tab body: {}",
        result.html,
    );
}

#[test]
fn directive_inside_indented_code_stays_literal() {
    let result = render_tabs("    :::tab[X]\n    body\n    :::");
    assert!(result.html.contains("<code>"), "got: {}", result.html);
    assert!(result.html.contains(":::tab[X]"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn directive_inside_fenced_code_stays_literal() {
    let result = render_tabs("```\n:::tab[X]\n```");
    assert!(result.html.contains(":::tab[X]"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn container_inside_blockquote_is_recognized() {
    let md = "> ::::tabs\n>\n> :::tab[Q]\n>\n> Body.\n>\n> :::\n>\n> ::::";
    let result = render_tabs(md);
    assert!(result.html.contains("<blockquote>"), "got: {}", result.html);
    assert_between(
        &result.html,
        r#"role="tablist""#,
        "<blockquote>",
        "</blockquote>",
    );
}

#[test]
fn container_inside_loose_list_is_recognized() {
    let md = "- item\n\n  ::::tabs\n\n  :::tab[L]\n\n  Body.\n\n  :::\n\n  ::::";
    let result = render_tabs(md);
    assert!(result.html.contains("<li>"), "got: {}", result.html);
    assert_between(&result.html, r#"role="tablist""#, "<li>", "</li>");
}

#[test]
fn unregistered_container_renders_literally_no_warning() {
    // An unregistered :::foo … ::: pair must not produce a "stray" warning —
    // the closing ::: is matched with its own opener, not treated as unpaired.
    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        ":::foo[x]\n\nBody.\n\n:::",
        Pipeline::new().with_directives(DirectiveProcessor::new()),
    );
    assert!(
        result.html.contains("<p>:::foo[x]</p>"),
        "got: {}",
        result.html
    );
    assert!(result.html.contains("<p>:::</p>"), "got: {}", result.html);
    assert!(
        result.warnings.is_empty(),
        "unregistered open/close pair must not warn; got: {:?}",
        result.warnings
    );
}

#[test]
fn unregistered_container_opener_drops_extra_colons_closer_keeps_them() {
    // Pinned debt, not a statement of intent: the literal opener is built with
    // a hardcoded ":::" while the matching closer repeats its colon count, so a
    // four-colon opener round-trips as three. Only a render from source can
    // show that round trip: `dispatch_container_start` is never given the
    // opener's colon count, so no unit-level caller can pair a source colon
    // count against the output.
    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        "::::foo[x]{.c}\n\nBody.\n\n::::",
        Pipeline::new().with_directives(DirectiveProcessor::new()),
    );
    assert!(
        result.html.contains("<p>:::foo[x]{.c}</p>"),
        "opener should lose its fourth colon; got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>::::</p>"),
        "closer should keep all four colons; got: {}",
        result.html
    );
    assert!(
        result.warnings.is_empty(),
        "the four-colon close pairs with its own opener; got: {:?}",
        result.warnings
    );
}

#[test]
fn frontmatter_directive_shaped_text_is_inert() {
    let md = "---\ntitle: x\nnote: ':::tab[oops]'\n---\n\nBody.";
    let result = render_tabs(md);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn empty_container_body_has_no_stray_paragraph() {
    let md = "::::tabs\n\n:::tab[Empty]\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p></p>"), "got: {}", result.html);
}

#[test]
fn delimiter_with_trailing_hard_break_commits_as_paragraph() {
    let md = ":::tab[X]  \nstill same paragraph";
    let result = render_tabs(md);
    assert!(result.html.contains("<p>"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn two_consecutive_normal_paragraphs_do_not_leak_state() {
    // Regression guard: pending_paragraph / paragraph_open must not leak across
    // paragraph boundaries.
    let result = render_tabs("para one\n\npara two");
    assert_eq!(
        result.html.matches("<p>").count(),
        2,
        "got: {}",
        result.html
    );
    assert_eq!(
        result.html.matches("</p>").count(),
        2,
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>para one</p>"),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>para two</p>"),
        "got: {}",
        result.html
    );
}

#[test]
fn directive_immediately_followed_by_paragraph() {
    // A container directly followed by ordinary content renders both correctly.
    let md = "::::tabs\n\n:::tab[A]\n\nInside.\n\n:::\n\n::::\n\nAfter the tabs.";
    let result = render_tabs(md);
    assert!(result.html.contains("Inside."), "got: {}", result.html);
    assert!(
        result.html.contains("<p>After the tabs.</p>"),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p>:::"), "got: {}", result.html);
}

#[test]
fn nested_same_name_containers_render_balanced() {
    use rw_renderer::directive::ContainerDirective;
    struct Details {
        depth: usize,
    }
    impl ContainerDirective for Details {
        fn name(&self) -> &'static str {
            "details"
        }
        fn start(&mut self, _a: DirectiveArgs, _c: &DirectiveContext) -> DirectiveOutput {
            self.depth += 1;
            DirectiveOutput::html("<details>".to_owned())
        }
        fn end(&mut self, _line: usize) -> Option<String> {
            self.depth -= 1;
            Some("</details>".to_owned())
        }
    }
    let directives = DirectiveProcessor::new().with_container(Details { depth: 0 });
    let md = ":::details[Outer]\n\n:::details[Inner]\n\nx\n\n:::\n\n:::";
    let result = MarkdownRenderer::<HtmlBackend>::new()
        .render(md, Pipeline::new().with_directives(directives));
    assert_eq!(
        result.html.matches("</details>").count(),
        2,
        "got: {}",
        result.html
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unclosed_container_warns() {
    let md = "::::tabs\n\n:::tab[Open]\n\nbody";
    let result = render_tabs(md);
    assert!(
        result.warnings.iter().any(|w| w.contains("unclosed")),
        "got: {:?}",
        result.warnings
    );
}

#[test]
fn tab_label_is_html_escaped_and_quotes_stripped() {
    // Special characters in the label are HTML-escaped. (A label that is a
    // recognized HTML tag like `<script>` can't be used here: pulldown-cmark
    // consumes that whole line as an HTML block before the directive scanner
    // sees it. `<` / `&` that don't start an HTML block reach the directive
    // and are escaped on the way out.)
    let r1 = render_tabs("::::tabs\n\n:::tab[a < b & c]\n\nx\n\n:::\n\n::::");
    assert!(r1.html.contains("a &lt; b &amp; c"), "got: {}", r1.html);
    // Surrounding quotes are stripped from the label.
    let r2 = render_tabs("::::tabs\n\n:::tab[\"macOS и Linux\"]\n\nx\n\n:::\n\n::::");
    assert!(
        r2.html.contains(">macOS и Linux</button>"),
        "got: {}",
        r2.html
    );
}

/// A container that defers its opening tag, proving the walker reserves a hole
/// during the walk and fills it afterwards.
#[derive(Default)]
struct DeferredContainer {
    seen: usize,
}

impl ContainerDirective for DeferredContainer {
    fn name(&self) -> &'static str {
        "deferred"
    }

    fn start(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        self.seen += 1;
        DirectiveOutput::Deferred(vec![Part::Hole(1), Part::Html("<p>body</p>".into())])
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        Some("</section>".to_owned())
    }

    fn fills(&mut self, fills: &mut Fills) {
        // Content known only after the walk — here, the opener count.
        fills.set(1, format!(r#"<section data-seen="{}">"#, self.seen));
    }
}

#[test]
fn deferred_container_fills_hole_after_walk() {
    let processor = DirectiveProcessor::new().with_container(DeferredContainer::default());
    let renderer = MarkdownRenderer::<HtmlBackend>::new();

    // The leading paragraph puts the hole at a non-zero offset, so the
    // splice position is actually exercised rather than degenerating to 0.
    let result = renderer.render(
        "intro\n\n:::deferred\n\ntext\n\n:::\n",
        Pipeline::new().with_directives(processor),
    );

    // The fill lands after the intro paragraph and before the directive's own
    // literal parts — exactly where the hole was reserved.
    assert_between(
        &result.html,
        r#"<section data-seen="1">"#,
        "intro</p>",
        "<p>body</p>",
    );
}

/// A leaf directive that defers its output, proving the walker reserves a hole
/// during the walk and fills it afterwards.
#[derive(Default)]
struct DeferredLeaf {
    seen: usize,
}

impl LeafDirective for DeferredLeaf {
    fn name(&self) -> &'static str {
        "deferredleaf"
    }

    fn process(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        self.seen += 1;
        DirectiveOutput::Deferred(vec![Part::Hole(1)])
    }

    fn fills(&mut self, fills: &mut Fills) {
        // Content known only after the walk — here, the invocation count.
        fills.set(1, format!(r#"<aside data-seen="{}"></aside>"#, self.seen));
    }
}

#[test]
fn deferred_leaf_fills_hole_after_walk() {
    let processor = DirectiveProcessor::new().with_leaf(DeferredLeaf::default());
    let renderer = MarkdownRenderer::<HtmlBackend>::new();

    // The leading paragraph puts the hole at a non-zero offset, so the
    // splice position is actually exercised rather than degenerating to 0.
    let result = renderer.render(
        "intro\n\n::deferredleaf\n",
        Pipeline::new().with_directives(processor),
    );

    let fill = result
        .html
        .find(r#"<aside data-seen="1"></aside>"#)
        .unwrap_or_else(|| panic!("hole was not filled: {}", result.html));
    let intro_end = result
        .html
        .find("intro</p>")
        .unwrap_or_else(|| panic!("intro paragraph missing: {}", result.html))
        + "intro</p>".len();

    assert!(
        fill >= intro_end,
        "fill landed before the intro paragraph closed: {}",
        result.html
    );
}

/// Two container directives that both pick local hole key `0` — the natural
/// choice for a handler numbering its own holes from zero.
struct LocalKeyZeroContainer {
    name: &'static str,
    fill: &'static str,
}

impl ContainerDirective for LocalKeyZeroContainer {
    fn name(&self) -> &str {
        self.name
    }

    fn start(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        DirectiveOutput::Deferred(vec![Part::Hole(0)])
    }

    fn end(&mut self, _line: usize) -> Option<String> {
        None
    }

    fn fills(&mut self, fills: &mut Fills) {
        fills.set(0, self.fill.to_owned());
    }
}

#[test]
fn two_handlers_using_the_same_local_hole_key_do_not_collide() {
    let processor = DirectiveProcessor::new()
        .with_container(LocalKeyZeroContainer {
            name: "alpha",
            fill: "<p>ALPHA-FILL</p>",
        })
        .with_container(LocalKeyZeroContainer {
            name: "beta",
            fill: "<p>BETA-FILL</p>",
        });
    let renderer = MarkdownRenderer::<HtmlBackend>::new();

    let result = renderer.render(
        ":::alpha\n\na\n\n:::\n\n:::beta\n\nb\n\n:::\n",
        Pipeline::new().with_directives(processor),
    );

    assert!(
        result.html.contains("ALPHA-FILL"),
        "first handler's fill was lost: {}",
        result.html
    );
    assert!(
        result.html.contains("BETA-FILL"),
        "second handler's fill was lost: {}",
        result.html
    );
}

/// An inline directive that (incorrectly) defers content. Inline directives
/// have no `fills()` hook, so a hole they reserve could never be filled.
struct DeferringInline;

impl InlineDirective for DeferringInline {
    fn name(&self) -> &'static str {
        "deferredinline"
    }

    fn process(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        DirectiveOutput::Deferred(vec![Part::Html("LITERAL".into()), Part::Hole(0)])
    }
}

#[test]
fn deferred_inline_directive_warns_instead_of_reserving_a_hole() {
    let processor = DirectiveProcessor::new().with_inline(DeferringInline);
    let renderer = MarkdownRenderer::<HtmlBackend>::new();

    // Inside a heading: inline directives commonly run within a scope, where
    // reserving a hole would also trip `reserve_hole`'s empty-scopes assert.
    let result = renderer.render(
        "# Title :deferredinline[x]\n",
        Pipeline::new().with_directives(processor),
    );

    assert!(
        result.html.contains("LITERAL"),
        "literal parts must still be emitted: {}",
        result.html
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("deferredinline") && w.contains("defer")),
        "expected a warning naming the directive: {:?}",
        result.warnings
    );
}

/// Fills reach the buffer through the backend's `raw_html`, like every other
/// emission — so a backend that drops markup drops fills too. Without that,
/// the tab bar and panel `<div>`s (which are fills, not walk-time output) leak
/// into the search index.
#[test]
fn tabs_emit_no_markup_into_a_search_document() {
    let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
    let result = MarkdownRenderer::<SearchDocumentBackend>::new().render(
        "::::tabs\n\n:::tab[macOS]\n\nmac body\n\n:::\n\n:::tab[Linux]\n\nlinux body\n\n:::\n\n::::\n",
        Pipeline::new().with_directives(directives),
    );

    assert!(
        !result.html.contains('<'),
        "markup leaked into the search document: {}",
        result.html
    );
    assert!(result.html.contains("mac body"), "got: {}", result.html);
    assert!(result.html.contains("linux body"), "got: {}", result.html);
    // Tab labels are markup content, not prose — the backend drops them.
    assert!(!result.html.contains("tablist"), "got: {}", result.html);
}

/// The closing tags a missing `:::` forces the processor to emit at end of
/// input take the same backend route as an in-walk `end()`.
#[test]
fn unclosed_tabs_emit_no_markup_into_a_search_document() {
    let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
    let result = MarkdownRenderer::<SearchDocumentBackend>::new().render(
        "::::tabs\n\n:::tab[macOS]\n\nmac body\n\n:::\n\n:::tab[Linux]\n\nlinux body\n",
        Pipeline::new().with_directives(directives),
    );

    assert!(
        !result.html.contains('<'),
        "markup leaked into the search document: {}",
        result.html
    );
    assert!(result.html.contains("mac body"), "got: {}", result.html);
    assert!(result.html.contains("linux body"), "got: {}", result.html);
}

/// Defers every `demo` block, filling it after the walk.
#[derive(Default)]
struct DeferringProcessor {
    seen: Vec<usize>,
}

impl CodeBlockProcessor for DeferringProcessor {
    fn process(
        &mut self,
        language: &str,
        _attrs: &FenceAttrs,
        _source: &str,
        index: usize,
    ) -> ProcessResult {
        if language != "demo" {
            return ProcessResult::PassThrough;
        }
        self.seen.push(index);
        ProcessResult::Deferred
    }

    fn fills(&mut self, fills: &mut Fills) {
        for index in &self.seen {
            let key = u32::try_from(*index).expect("code block index exceeds hole key width");
            fills.set(key, format!("<i>block {index}</i>"));
        }
    }
}

#[test]
fn code_block_and_directive_holes_interleave_in_one_document() {
    // Markup closing a tab group: the panel plus the group's opening tags.
    const TAB_GROUP_CLOSE: &str = "</div></div>";

    // Two independent hole sources — a tab container and a code-block
    // processor — reserving into the same buffer. Both reserve at the current
    // end of an append-only buffer, so their offsets are non-decreasing without
    // any coordination between them. Asserting the nested fill lands inside a
    // real, filled panel element (`id="panel-0-0"`, from the tab container's
    // own hole) verifies both hole sources landed correctly, not just the
    // code-block processor's.
    let markdown = "\
::::tabs

:::tab[One]

```demo
x
```

:::

::::

```demo
y
```
";

    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        markdown,
        Pipeline::new()
            .with_directives(DirectiveProcessor::new().with_container(TabsDirective::new()))
            .with_processor(DeferringProcessor::default()),
    );

    // The nested block fills inside the tab panel, the trailing one outside it.
    assert_between(
        &result.html,
        "<i>block 0</i>",
        r#"id="panel-0-0""#,
        TAB_GROUP_CLOSE,
    );
    assert!(
        result.html.contains("<i>block 1</i>"),
        "trailing fill missing: {}",
        result.html
    );
    let panel_end = result
        .html
        .rfind(TAB_GROUP_CLOSE)
        .expect("tab group should close");
    assert!(
        result.html.find("<i>block 1</i>").expect("trailing fill") > panel_end,
        "trailing fill landed inside the tab group: {}",
        result.html
    );
}

#[test]
fn inline_directive_inside_an_unclaimed_container_opener_still_expands() {
    // The Parser splits inline directives out of the text runs it tokenizes,
    // but a container line nobody claimed comes back through
    // `BlockDispatch::PassThrough` as a literal reconstructed from its parsed
    // args — text that was never tokenized. That is why the Walker keeps one
    // scanner: it re-runs the tokenizer over exactly this reconstruction.
    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        ":::foo[:status[Stable]{color=green}]\n\nBody.\n\n:::",
        Pipeline::new().with_directives(DirectiveProcessor::new()),
    );
    assert_eq!(
        result.html,
        concat!(
            r#"<p>:::foo[<span class="status status-green">Stable</span>]</p>"#,
            "<p>Body.</p><p>:::</p>",
        ),
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unknown_inline_directives_warn_in_document_order() {
    // Warnings gate `--strict` publishing and leave no trace in the HTML, so
    // an output comparison cannot see them: their text and their order are
    // asserted here in full. `:status` is built-in and must stay silent.
    let result = render_status(":alpha[x] then :beta[y] then :status[z]");
    assert_eq!(
        result.warnings,
        vec![
            "unknown inline directive ':alpha' — no handler registered (or handler returned Skip)",
            "unknown inline directive ':beta' — no handler registered (or handler returned Skip)",
        ]
    );
}

#[test]
fn nested_tabs_render_byte_identical_to_the_old_continuation_output() {
    // The proposal-aligned nested input must produce exactly the bytes the old
    // continuation input produced (the viewer depends on this markup).
    const EXPECTED: &str = r#"<div class="tabs" id="tabs-0"><div class="tabs-buttons" role="tablist"><button role="tab" id="tab-0-0" aria-controls="panel-0-0" aria-selected="true" tabindex="0">macOS</button><button role="tab" id="tab-0-1" aria-controls="panel-0-1" aria-selected="false" tabindex="-1">Linux</button></div><div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0"><p>Install with Homebrew.</p></div><div role="tabpanel" id="panel-0-1" aria-labelledby="tab-0-1" hidden><p>Install with apt.</p></div></div>"#;
    let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n\
              :::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
    let result = render_tabs(md);
    assert_eq!(result.html, EXPECTED, "got: {}", result.html);
    assert!(
        result.warnings.is_empty(),
        "warnings: {:?}",
        result.warnings
    );
}

#[test]
fn old_continuation_syntax_no_longer_groups() {
    // The removed shared-closer form now yields lone tabs (warned, unwrapped),
    // not a tab group.
    let result = render_tabs(":::tab[A]\n\nx\n\n:::tab[B]\n\ny\n\n:::");
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "old continuation form must no longer render a tab group: {}",
        result.html
    );
}

#[test]
fn nested_tab_groups_render_both_bars_without_panic() {
    // A `::::tabs` group nested inside a `:::tab` panel of an outer group must
    // not drop the outer group's reserved bar hole: with a single `Option`
    // slot, opening the inner group overwrites the outer one, the inner
    // group's `end()` finalizes it, and the outer group's hole is never
    // filled — panicking assembly in debug, silently vanishing in release.
    let md = "::::tabs\n\n:::tab[Outer A]\n\n::::tabs\n\n:::tab[Inner X]\n\nbody\n\n:::\n\n::::\n\n:::\n\n::::";
    let result = render_tabs(md);

    assert_eq!(
        result.html.matches(r#"role="tablist""#).count(),
        2,
        "expected both group bars rendered: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"id="tabs-0""#),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"id="tabs-1""#),
        "got: {}",
        result.html
    );
    assert_eq!(
        result.html.matches("<div").count(),
        result.html.matches("</div>").count(),
        "unbalanced divs: {}",
        result.html
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unclosed_tab_item_warning_names_tab_not_tabs() {
    // Item B and the enclosing group are both left unclosed. The unclosed
    // ITEM must be reported as `:::tab`, not misnamed `:::tabs` (the single
    // handler's `name()`, which is always "tabs").
    let md = "::::tabs\n\n:::tab[A]\n\nA\n\n:::\n\n:::tab[B]\n\nB\n\nAFTER\n";
    let result = render_tabs(md);

    assert!(
        result.warnings.iter().any(|w| w.contains(":::tab (")),
        "expected an accurately-named unclosed `:::tab` warning: {:?}",
        result.warnings
    );
}
