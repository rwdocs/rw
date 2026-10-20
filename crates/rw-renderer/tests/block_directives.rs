//! Stream-native leaf & container directive behavior, driven through the full
//! `MarkdownRenderer` pipeline.

use std::sync::Arc;

use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, DiagramRouter,
    DiagramSource, Providers, ResolveContext, Resolved,
};
use rw_renderer::{HtmlBackend, MarkdownRenderer, RenderResult, SearchDocumentBackend};

fn render_tabs(md: &str) -> RenderResult {
    MarkdownRenderer::<HtmlBackend>::new().render(md)
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
    MarkdownRenderer::<HtmlBackend>::new().render(md)
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
fn inline_directive_inside_a_code_span_stays_literal() {
    // A directive inside an inline code span must render verbatim, never
    // expand (shipped behavior, CHANGELOG 0.1.25). The parser never tokenizes
    // directive syntax inside a code span, so `:status` — the one inline
    // built-in — proves it.
    let result = render_status("A `:status[Done]{color=green}` B");
    assert!(
        result
            .html
            .contains("<code>:status[Done]{color=green}</code>"),
        "directive inside a code span must stay literal; got: {}",
        result.html
    );
    assert!(
        !result.html.contains("status-green"),
        "directive inside a code span must not expand; got: {}",
        result.html
    );
}

#[test]
fn inline_directive_expands_outside_a_code_span_on_the_same_line() {
    // The code span suppresses only what it encloses: on a line carrying both,
    // the bare directive expands and the quoted one stays literal.
    let result = render_status("Use :status[A]{color=green} not `:status[B]{color=red}`.");
    assert!(
        result
            .html
            .contains(r#"<span class="status status-green">A</span>"#),
        "the bare directive must expand; got: {}",
        result.html
    );
    assert!(
        result.html.contains("<code>:status[B]{color=red}</code>"),
        "the quoted directive must stay literal; got: {}",
        result.html
    );
    assert!(
        !result.html.contains("status-red"),
        "the quoted directive must not expand; got: {}",
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
    // outside it must not close the tab group a second time — a double close
    // would emit the group twice. Neither the group nor the item
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
fn closing_delimiter_inside_a_nested_block_is_stray() {
    // The reverse of the test above: a `:::` inside a blockquote or list item
    // cannot close a tab opened outside it, or the panel's `</div>` lands
    // inside that block.
    for md in [
        "::::tabs\n\n:::tab[A]\n\nbody\n\n> :::\n\nafter\n\n:::\n\n::::\n",
        "::::tabs\n\n:::tab[A]\n\n- item\n\n  :::\n\n  more\n\nafter\n\n:::\n\n::::\n",
    ] {
        let result = render_tabs(md);
        let html = &result.html;

        assert_eq!(
            html.matches("<div").count(),
            html.matches("</div>").count(),
            "unbalanced divs: {html}"
        );
        assert!(html.contains("<p>:::</p>"), "stray kept literally: {html}");
        assert!(
            html.ends_with("<p>after</p></div></div>"),
            "content after the block left the panel: {html}"
        );
        assert_eq!(
            result.warnings,
            ["stray ::: with no opening directive"],
            "{md:?}"
        );
    }
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
fn unrecognized_container_opener_and_closer_keep_their_colons() {
    let result = MarkdownRenderer::<HtmlBackend>::new().render("::::foo[x]{.c}\n\nBody.\n\n::::");
    assert!(
        result.html.contains("<p>::::foo[x]{.c}</p>"),
        "opener should keep all four colons; got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>::::</p>"),
        "closer should keep all four colons; got: {}",
        result.html
    );
    assert_eq!(
        result.warnings,
        ["unknown container directive ':::foo'"],
        "the four-colon close pairs with its own opener"
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
fn frontmatter_inline_directive_shaped_text_raises_no_warning() {
    // A frontmatter value shaped like an inline directive must never reach the
    // inline scanner: `:foo` is not a built-in, so if the metadata block leaked
    // into the walk it would warn "unknown inline directive ':foo'" and
    // `--strict` publishing would fail on it (CHANGELOG 0.1.25). The container
    // case above cannot catch that — a `:::`-shaped value is block syntax and
    // never reaches the inline scanner even from body text.
    let md = "---\ntitle: see :foo[x]\n---\n\nBody.";
    let result = render_status(md);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
    assert_eq!(result.html, "<p>Body.</p>", "got: {}", result.html);
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

/// Fills reach the buffer through the backend's `raw_html`, like every other
/// emission — so a backend that drops markup drops fills too. Without that,
/// the tab bar (a fill, not walk-time output) would leak into the search
/// index; the panel `<div>`s are emitted inline through the backend's own
/// no-op tab methods.
#[test]
fn tabs_emit_no_markup_into_a_search_document() {
    let result = MarkdownRenderer::<SearchDocumentBackend>::new().render(
        "::::tabs\n\n:::tab[macOS]\n\nmac body\n\n:::\n\n:::tab[Linux]\n\nlinux body\n\n:::\n\n::::\n");

    assert!(
        !result.html.contains('<'),
        "markup leaked into the search document: {}",
        result.html
    );
    assert!(result.html.contains("mac body"), "got: {}", result.html);
    assert!(result.html.contains("linux body"), "got: {}", result.html);
    // Tab labels are markup content, not prose — the backend drops them.
    assert!(!result.html.contains("macOS"), "got: {}", result.html);
}

/// The closing tags a missing `:::` forces the walker to emit at end of input
/// take the same backend route as an explicit tab-group close.
#[test]
fn unclosed_tabs_emit_no_markup_into_a_search_document() {
    let result = MarkdownRenderer::<SearchDocumentBackend>::new()
        .render("::::tabs\n\n:::tab[macOS]\n\nmac body\n\n:::\n\n:::tab[Linux]\n\nlinux body\n");

    assert!(
        !result.html.contains('<'),
        "markup leaked into the search document: {}",
        result.html
    );
    assert!(result.html.contains("mac body"), "got: {}", result.html);
    assert!(result.html.contains("linux body"), "got: {}", result.html);
}

/// Claims `demo` fences and answers each with an SVG naming its own source, so
/// a test can tell which fence filled which hole.
struct DemoDiagrams;

impl DiagramProvider for DemoDiagrams {
    fn handles(&self, language: &str) -> bool {
        language == "demo"
    }

    fn resolve(
        &self,
        requests: &[&DiagramRequest],
        _ctx: &ResolveContext<'_>,
    ) -> Vec<Result<Resolved, DiagramError>> {
        requests
            .iter()
            .map(|request| {
                let DiagramSource::Inline(source) = &request.source else {
                    panic!("the walker requests only fences");
                };
                Ok(Resolved {
                    asset: Asset::Inline(DiagramContent::Svg(format!(
                        "<svg>{}</svg>",
                        source.trim()
                    ))),
                    size: None,
                    digest: "0".to_owned(),
                    warnings: Vec::new(),
                })
            })
            .collect()
    }
}

#[test]
fn diagram_and_directive_holes_interleave_in_one_document() {
    // Markup closing a tab group: the panel plus the group's opening tags.
    const TAB_GROUP_CLOSE: &str = "</div></div>";

    // Two independent hole sources — the built-in tab group and a diagram fence
    // — reserving into the same buffer. Both reserve at the current end of an
    // append-only buffer, so their offsets are non-decreasing without any
    // coordination between them. Asserting the nested fill lands inside a real,
    // filled panel element (`id="panel-0-0"`, from the tab group's own bar hole)
    // verifies both hole sources landed correctly, not just the diagram's.
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

    let providers = Providers::empty().with(Arc::new(DemoDiagrams) as Arc<dyn DiagramProvider>);
    let renderer = MarkdownRenderer::<HtmlBackend>::new()
        .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>);
    let pass = renderer.begin(markdown);
    let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
    let result = pass.finish(&resolutions);

    // The nested diagram fills inside the tab panel, the trailing one outside it.
    assert_between(
        &result.html,
        "<svg>x</svg>",
        r#"id="panel-0-0""#,
        TAB_GROUP_CLOSE,
    );
    assert!(
        result.html.contains("<svg>y</svg>"),
        "trailing fill missing: {}",
        result.html
    );
    let panel_end = result
        .html
        .rfind(TAB_GROUP_CLOSE)
        .expect("tab group should close");
    assert!(
        result.html.find("<svg>y</svg>").expect("trailing fill") > panel_end,
        "trailing fill landed inside the tab group: {}",
        result.html
    );
}

#[test]
fn inline_directive_inside_an_unclaimed_container_opener_still_expands() {
    // The Parser splits inline directives out of the text runs it tokenizes,
    // but an unrecognized container line renders literally, reconstructed from
    // its parsed args — text that was never tokenized. That is why the Walker
    // keeps one scanner: it re-runs the tokenizer over exactly this
    // reconstruction, so the inner `:status` built-in still expands.
    let result = MarkdownRenderer::<HtmlBackend>::new()
        .render(":::foo[:status[Stable]{color=green}]\n\nBody.\n\n:::");
    assert_eq!(
        result.html,
        concat!(
            r#"<p>:::foo[<span class="status status-green">Stable</span>]</p>"#,
            "<p>Body.</p><p>:::</p>",
        ),
    );
    assert_eq!(result.warnings, ["unknown container directive ':::foo'"]);
}

#[test]
fn an_escape_in_a_literal_directive_line_stays_content() {
    for (md, html, warning) in [
        (
            r"::foo[\:status[X]]",
            "<p>::foo[:status[X]]</p>",
            "unknown leaf directive '::foo'",
        ),
        (
            "::foo[&#58;status[X]]",
            "<p>::foo[:status[X]]</p>",
            "unknown leaf directive '::foo'",
        ),
        (
            r":::foo[\:status[X]]",
            "<p>:::foo[:status[X]]</p>",
            "unknown container directive ':::foo'",
        ),
        (
            r"::: foo[\:status[X]]",
            "<p>::: foo[:status[X]]</p>",
            "unknown container directive ':::foo'",
        ),
        (
            r"::foo[:status[A] \:status[B] :status[C]]",
            r#"<p>::foo[<span class="status status-grey">A</span> :status[B] <span class="status status-grey">C</span>]</p>"#,
            "unknown leaf directive '::foo'",
        ),
        (
            r"::foo[:status[X\]]]",
            r#"<p>::foo[<span class="status status-grey">X]</span>]</p>"#,
            "unknown leaf directive '::foo'",
        ),
    ] {
        let result = render_status(md);
        assert_eq!(result.html, html, "{md}");
        assert_eq!(result.warnings, [warning], "{md}");
    }
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
            "unknown inline directive ':alpha'",
            "unknown inline directive ':beta'",
        ]
    );
}

#[test]
fn tabs_sharing_one_closer_do_not_group() {
    // A tab group is an outer `::::tabs` wrapping self-closing `:::tab` items.
    // Several `:::tab` sharing one closing `:::` are lone tabs (warned,
    // unwrapped), not a group.
    let result = render_tabs(":::tab[A]\n\nx\n\n:::tab[B]\n\ny\n\n:::");
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "tabs sharing one closer must not render a tab group: {}",
        result.html
    );
}

#[test]
fn nested_tab_groups_render_both_bars_without_panic() {
    // A `::::tabs` group nested inside a `:::tab` panel of an outer group must
    // not drop the outer group's reserved bar hole: with a single `Option`
    // slot, opening the inner group would overwrite the outer one, the inner
    // group's close would finalize it, and the outer group's hole would never
    // be filled — panicking assembly in debug, silently vanishing in release.
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
fn compact_tab_group_renders_as_its_blank_line_form() {
    let compact = "::::tabs\n:::tab[One]\nFirst **tab**.\n:::\n:::tab[Two]\n> quoted\n\n:::\n::::";
    let spaced = "::::tabs\n\n:::tab[One]\n\nFirst **tab**.\n\n:::\n\n:::tab[Two]\n\n> quoted\n\n:::\n\n::::";
    let (compact, spaced) = (render_tabs(compact), render_tabs(spaced));
    assert!(
        compact.html.contains(">Two</button>"),
        "got: {}",
        compact.html
    );
    assert_eq!(compact.html, spaced.html);
    assert_eq!(compact.warnings, spaced.warnings);
}

#[test]
fn tab_opener_with_text_after_its_label_renders_as_text_and_warns() {
    for opener in [":::tab macOS", ":::tab[a]b]"] {
        for md in [
            format!("::::tabs\n{opener}\nbody\n:::\n::::"),
            format!("::::tabs\n\n{opener}\n\nbody\n\n:::\n\n::::"),
        ] {
            let result = render_tabs(&md);
            assert!(result.html.contains(opener), "{md:?}: {}", result.html);
            assert!(!result.warnings.is_empty(), "{md:?}");
        }
    }
}

fn as_text(line: &str) -> String {
    format!("directive line `{line}` renders as text: put a blank line before it")
}

fn as_heading(line: &str) -> String {
    format!(
        "directive line `{line}` renders as a heading: put a blank line before the `---` or `===` under it"
    )
}

#[test]
fn directive_lines_glued_to_a_block_that_keeps_them_warn() {
    for (md, warning) in [
        // Text of a tight list item, indented or lazy.
        (
            "1. Install:\n   ::::tabs\n   :::tab[A]\n   a\n   :::\n   ::::\n2. Run.\n",
            as_text("::::tabs"),
        ),
        ("- Step\n  :::note\n  body\n  :::\n", as_text(":::note")),
        (
            "- item\n::::tabs\n:::tab[A]\na\n:::\n::::\n",
            as_text("::::tabs"),
        ),
        ("- a\n  ::youtube[x]\n  b", as_text("::youtube[x]")),
        // Lazy continuation of a quote or of a loose item's paragraph.
        (
            "> quote\n::::tabs\n:::tab[A]\na\n:::\n::::\n",
            as_text("::::tabs"),
        ),
        (
            "- one\n\n- two\n::::tabs\n:::tab[B]\nb\n:::\n::::",
            as_text("::::tabs"),
        ),
        (
            "> - a\n>\n>   b\n> :::note\n> body\n> :::",
            as_text(":::note"),
        ),
        // A setext underline turns the lines above it into a heading.
        (
            "::::tabs\n:::tab[A]\n---\na\n:::\n::::\n",
            as_heading("::::tabs"),
        ),
        (
            "::::tabs\n:::tab[A]\n===\na\n:::\n::::\n",
            as_heading("::::tabs"),
        ),
        (
            "::::tabs\n:::tab[A]\na\n:::\n::::\n---\nNext section\n",
            as_heading("::::tabs"),
        ),
        ("::youtube[x]\n---\nafter\n", as_heading("::youtube[x]")),
        // A table row, or a line of an HTML block that runs to a blank line.
        (
            "::::tabs\n:::tab[A]\n| a | b |\n|---|---|\n| 1 | 2 |\n:::\n:::tab[B]\nb\n:::\n::::\n",
            as_text(":::"),
        ),
        (
            "::::tabs\n:::tab[A]\n<div>hi</div>\n:::\n:::tab[B]\nb\n:::\n::::\n",
            as_text(":::"),
        ),
        ("<div>hi</div>\n:::note\n", as_text(":::note")),
    ] {
        let result = render_tabs(md);
        let lines: Vec<&String> = result
            .warnings
            .iter()
            .filter(|w| w.starts_with("directive line"))
            .collect();
        assert_eq!(lines, [&warning], "{md:?}");
    }
}

#[test]
fn a_closer_glued_to_a_list_warns_only_while_a_container_is_open() {
    let open = render_tabs(":::note\n\n- item\n:::\n");
    assert!(
        open.warnings.contains(&as_text(":::")),
        "{:?}",
        open.warnings
    );
    let closed = render_tabs("- item\n  :::\n");
    assert_eq!(closed.warnings, [] as [&str; 0]);
}

#[test]
fn colon_prose_does_not_warn() {
    for md in [
        "Part one ends here\n:::\nPart two starts here.\n",
        "Great job\n:tada: shipped!\n",
        "Call the function\n::std::move(x) to move it.\n",
        "Style it with\n::before\nand ::after.\n",
        "The fence looks like\n:::note followed by text\nand so on.\n",
        "Meet at\n12:30 today\n",
        "Loopback is\n::1\nalways.\n",
        "\\:::note\nbody\n\\:::\n",
        "&#58;::note\nbody\n",
        "*see\n:::*\n",
        "| a | b |\n|---|---|\n| :::note | x |\n",
        "::: some closing words",
        "- a\n  ::before\n  ::1\n  ::std::move\n  :::\n",
        "- a\n  \\:::note\n  &#58;::note\n",
        "> a\n\\:::note\n",
        "<!--\n:::note\n-->\n",
        "<pre>\n:::note\n</pre>\n",
    ] {
        let result = render_tabs(md);
        assert_eq!(result.warnings, [] as [&str; 0], "{md:?}");
    }
}

#[test]
fn a_glued_block_warns_once_for_its_first_directive_line() {
    let result = render_tabs("- a\n  :::note\n  :::tip\n- b\n  ::youtube[x]\n");
    assert_eq!(
        result.warnings,
        [as_text(":::note"), as_text("::youtube[x]")]
    );
}

#[test]
fn a_directive_line_opening_a_tight_list_item_warns() {
    for (md, line) in [
        ("- ::::tabs\n- b", "::::tabs"),
        ("- ::youtube[x]\n- b", "::youtube[x]"),
        ("1. :::note\n2. b", ":::note"),
        ("3) :::note\n4) b", ":::note"),
        (
            "::::tabs\n:::tab[A]\nx\n:::\n::::\n\n- :::tab[B]\n  y\n- b",
            ":::tab[B]",
        ),
        (
            "- ::::tabs\n  :::tab[A]\n  a\n  :::\n  ::::\n- b",
            "::::tabs",
        ),
    ] {
        let result = render_tabs(md);
        let lines: Vec<&String> = result
            .warnings
            .iter()
            .filter(|w| w.starts_with("directive line"))
            .collect();
        assert_eq!(
            lines,
            [&format!(
                "directive line `{line}` renders as text: put a blank line between the list's items"
            )],
            "{md:?}"
        );
    }
}

#[test]
fn colon_prose_opening_a_tight_list_item_does_not_warn() {
    for md in [
        "- :::\n- b",
        "- ::before\n- b",
        "- ::1\n- b",
        "- \\:::note\n- b",
        "- &#58;::note\n- b",
        "- :::note followed by text\n- b",
        "- ::std::move\n- b",
    ] {
        let result = render_tabs(md);
        assert_eq!(result.warnings, [] as [&str; 0], "{md:?}");
    }
}

#[test]
fn an_opener_with_a_title_after_its_name_warns() {
    for md in [":::note Title\nbody\n:::\n", ":::note Title\n\nbody"] {
        let result = render_tabs(md);
        assert_eq!(
            result.warnings,
            [
                "directive line `:::note Title` renders as text: only a `[label]` and `{attributes}` may follow the name"
            ],
            "{md:?}"
        );
    }
}

#[test]
fn a_label_with_markup_warns() {
    let result = render_tabs(":::note[Your **Title**]\nbody\n:::\n");
    assert_eq!(
        result.warnings,
        [
            "directive line `:::note[Your **Title**]` renders as text: a directive label can't contain markup"
        ]
    );
    let result = render_tabs("::::tabs\n:::tab[`npm`]\na\n:::\n::::\n");
    assert!(
        result.warnings.contains(
            &"directive line `:::tab[`npm`]` renders as text: a directive label can't contain markup"
                .to_owned()
        ),
        "{:?}",
        result.warnings
    );
}

#[test]
fn unknown_directive_names_warn_once_per_page() {
    let result = render_tabs(
        ":::note\n\nx\n\n:::\n\n:::note\nb\n:::\n\n::foo[x]\n\n::foo{a=1}\n\n::::tip[T]\n\n::::\n\n:bar[x] :bar[y]",
    );
    assert_eq!(
        result.warnings,
        [
            "unknown container directive ':::note'",
            "unknown leaf directive '::foo'",
            "unknown container directive ':::tip'",
            "unknown inline directive ':bar'",
        ]
    );
}

#[test]
fn a_leaf_that_could_be_prose_does_not_warn() {
    let result = render_tabs("::before\n\n::1\n\n::1[x]");
    assert_eq!(result.warnings, [] as [&str; 0]);
}
