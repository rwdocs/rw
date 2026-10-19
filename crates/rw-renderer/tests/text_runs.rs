//! Characterization tests: rw's directive syntax against pulldown-cmark's
//! text-event splitting.
//!
//! cmark emits a separate `Text` event at every `[` and `]`, and splits again
//! at HTML entities and backslash escapes — `a &amp; b :status[X]{color=green}`
//! arrives as seven events. `rw_parser::Parser` coalesces adjacent runs and
//! scans them with `parse_line` before yielding any non-text event, so *when*
//! runs are joined decides whether a directive is seen at all.
//!
//! These tests pin what the renderer does, so a change to the split between
//! parser and walker can be verified byte-identical. Some pinned behavior looks
//! wrong; where it does, the test says so. Nothing here should be "fixed" to
//! make an expectation prettier — that would make a regression
//! indistinguishable from an intentional change.
//!
//! Both directions matter. Tests asserting a directive *expands* fail if a
//! change joins too little. The tests whose names say a run was interrupted
//! mid-`[...]` are the ones that fail if it joins too much; each was checked
//! by sabotaging the coalescing logic and confirming it went red. Not every
//! literal-asserting test discriminates — some produce identical output
//! either way, and say so.

use rw_renderer::{HtmlBackend, MarkdownRenderer, RenderResult};

fn render_status(md: &str) -> RenderResult {
    MarkdownRenderer::<HtmlBackend>::new().render(md)
}

fn render_tabs(md: &str) -> RenderResult {
    MarkdownRenderer::<HtmlBackend>::new().render(md)
}

/// `with_wikilinks` alone is enough to exercise `skip_wikilink_text`: the
/// parser sets it whatever the link later resolves to.
/// Deliberately avoids needing a populated `Sections` — `rw-renderer`
/// re-exports the `Sections` type but not the `Section` and `Namespace`
/// needed to fill one, and `rw-sections` is not a dev-dependency here.
fn render_status_wikilinks(md: &str) -> RenderResult {
    MarkdownRenderer::<HtmlBackend>::new()
        .with_wikilinks()
        .render(md)
}

#[test]
fn html_entity_before_an_inline_directive_does_not_break_it() {
    // `&amp;` decodes to its own `Text` event, splitting the run before the
    // directive ever starts.
    let result = render_status("a &amp; b :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<p>a &amp; b <span class="status status-green">X</span></p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn backslash_escape_before_an_inline_directive_does_not_break_it() {
    // The escape splits *before* the escaped character: cmark emits
    // `Text("a ")`, `Text("* b :status")`, then the bracket runs.
    let result = render_status(r"a \* b :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<p>a * b <span class="status status-green">X</span></p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn inline_directive_after_emphasis_still_expands() {
    // Emphasis forces a flush mid-paragraph (`Start(Emphasis)` is not a `Text`
    // event), so the directive lives in a run that begins after it.
    let result = render_status("a *em* :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<p>a <em>em</em> <span class="status status-green">X</span></p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unrecognized_inline_directive_passes_through_verbatim_and_warns() {
    // The literal is the raw source slice, so attribute order and spacing
    // survive exactly as written — `{b=2 a=1 .cls #id}`, not a reconstruction.
    let result = render_status(":unknown[body]{b=2 a=1 .cls #id}");
    assert_eq!(result.html, "<p>:unknown[body]{b=2 a=1 .cls #id}</p>");
    assert_eq!(result.warnings, vec!["unknown inline directive ':unknown'"]);
}

#[test]
fn paragraph_beginning_with_an_inline_directive_falls_through_to_expansion() {
    // A paragraph whose text starts with `:` is first offered to the *block*
    // parsers, on the chance it is a `:::`/`::` delimiter. Both reject this
    // one, and only then does it render as an ordinary paragraph with inline
    // expansion. Nothing else in this file exercises that fallthrough with a
    // directive that actually expands — the neighbouring
    // `paragraph_starting_with_a_colon_is_not_a_directive` reaches it with
    // plain prose, where a lost expansion would be invisible.
    let result = render_status(":status[X]{color=green} trailing text");
    assert_eq!(
        result.html,
        r#"<p><span class="status status-green">X</span> trailing text</p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn paragraph_starting_with_a_colon_is_not_a_directive() {
    let result = render_status(": just text");
    assert_eq!(result.html, "<p>: just text</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn prose_colon_mid_sentence_is_not_a_directive() {
    // A colon followed by a space never starts a directive. A colon followed
    // by a word does only with a `[...]` group; see
    // `bare_directive_name_in_prose_is_prose` below.
    let result = render_status("Note: see below");
    assert_eq!(result.html, "<p>Note: see below</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn bracket_content_interrupted_by_a_soft_break_is_not_a_directive() {
    // The sharpest over-joining guard in the file. The run ends mid-`[...]`,
    // so the scanner sees an unclosed group and the line stays literal.
    //
    // Join the runs across the soft break and the scanner sees the whole
    // `:status[X\ny]{color=green}`, rendering `status-green` with content
    // `X\ny`. Confirmed by sabotage — this assertion is what fails when the
    // coalescing is widened to cover `SoftBreak`.
    let result = render_status("a :status[X\ny]{color=green}");
    assert_eq!(result.html, "<p>a :status[X\ny]{color=green}</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn heading_directive_interrupted_mid_bracket_stays_literal() {
    // The heading routes text into two side buffers; the literal has to land
    // in both, so the anchor and the ToC entry agree with the visible HTML.
    let result = render_status("# a :status[X *em* y]{color=green}");
    assert_eq!(
        result.html,
        r#"<h1 id="a-statusx-em-ycolorgreen">a :status[X <em>em</em> y]{color=green}</h1>"#
    );
    assert_eq!(result.toc.len(), 1);
    assert_eq!(result.toc[0].id, "a-statusx-em-ycolorgreen");
    assert_eq!(result.toc[0].title, "a :status[X em y]{color=green}");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn container_delimiter_in_a_tight_list_item_is_silently_literal() {
    // A tight list item emits no `Start(Paragraph)`, so the parser never holds
    // a paragraph and the block-directive check in `decide_block` is never
    // reached at all. The delimiter is just text.
    //
    // PINNED AS-IS — this looks like a real defect, recorded rather than
    // fixed. Unlike the interrupted-opener case below, this one is
    // *completely* silent: no tab group, and no warning either, so `--strict`
    // publishing cannot catch it. The gating is an accident of paragraph
    // deferral, not a decision.
    let result = render_tabs("- :::tab[Label]{#a}\n- next item\n");
    assert_eq!(
        result.html,
        "<ul><li>:::tab[Label]{#a}</li><li>next item</li></ul>"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn container_opening_line_interrupted_by_emphasis_is_not_recognized() {
    // The parser holds a paragraph only while its run is still pure text, so
    // emphasis in the label ends the deferral and the opener is never parsed
    // as a container.
    //
    // PINNED AS-IS — the consequences are worth noting. The whole tab group
    // silently degrades to literal paragraphs, and the *closing* `:::` then
    // reports `stray ::: with no opening directive`, which points a reader at
    // the wrong line entirely. The opener that actually failed says nothing.
    //
    // The same rule is what
    // `bracket_content_interrupted_by_emphasis_is_not_a_directive` and
    // `inline_directive_after_emphasis_still_expands` pin. What is unique here
    // is the outcome — this is the only test where losing the deferral costs a
    // whole *container*, rather than one inline expansion.
    let result = render_tabs(":::tab[Label *em* more]{#a}\n\nBody.\n\n:::");
    assert_eq!(
        result.html,
        "<p>:::tab[Label <em>em</em> more]{#a}</p><p>Body.</p><p>:::</p>"
    );
    assert_eq!(result.warnings, vec!["stray ::: with no opening directive"]);
}

#[test]
fn bracket_content_interrupted_by_emphasis_is_not_a_directive() {
    // Same shape, but the run is broken by a non-`Text` event rather than a
    // soft break — the case a tokenizer is most likely to get wrong, since
    // joining across inline markup looks locally harmless. Joining here would
    // turn `[X <em>em</em> y]` into a badge's content and swallow the markup.
    let result = render_status("a :status[X *em* y]{color=green}");
    assert_eq!(
        result.html,
        "<p>a :status[X <em>em</em> y]{color=green}</p>"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn a_directive_cannot_straddle_a_wikilink() {
    // Exercises `skip_wikilink_text`, which drops the wikilink's raw-target
    // `Text` instead of joining it into the run. The wikilink is deliberately
    // unresolved, so it renders as a broken link and needs no `Sections`.
    let result = render_status_wikilinks("a :sta[[foo]]tus[X]{color=green}");
    assert_eq!(
        result.html,
        r##"<p>a :sta<a href="#" class="rw-broken-link">foo</a>tus[X]{color=green}</p>"##
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn bare_directive_name_in_prose_is_prose() {
    // An inline directive needs a `[...]` or `{...}` group, so a field name, a
    // Ruby symbol, `:D` or an emoji shortcode neither warns nor fails
    // `--strict` publishing.
    for md in [
        "just :sta here",
        "Use the :id key.",
        "Pass :symbol to the method.",
        "Nice :D",
        "Nice :smile: and :+1:",
    ] {
        let result = render_status(md);
        assert_eq!(result.html, format!("<p>{md}</p>"));
        assert!(result.warnings.is_empty(), "{md}: {:?}", result.warnings);
    }
}

#[test]
fn bare_builtin_directive_name_in_prose_is_prose() {
    // The silent case: a built-in matched without its group would render an
    // empty badge, and no warning would flag it.
    let result = render_status("Check the :status of the build.");
    assert_eq!(result.html, "<p>Check the :status of the build.</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn directive_with_braces_and_no_brackets_expands() {
    let result = render_status("a :status{color=red} b");
    assert_eq!(
        result.html,
        r#"<p>a <span class="status status-red"></span> b</p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unknown_directive_with_braces_and_no_brackets_warns() {
    let result = render_status("Set :foo{a=1} here.");
    assert_eq!(result.html, "<p>Set :foo{a=1} here.</p>");
    assert_eq!(result.warnings, vec!["unknown inline directive ':foo'"]);
}

#[test]
fn inline_directive_inside_a_blockquote_expands() {
    let result = render_status("> q :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<blockquote><p>q <span class="status status-green">X</span></p></blockquote>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn inline_directive_inside_a_tight_list_item_expands() {
    // A tight list item emits no `Start(Paragraph)` at all, so the run is not
    // drained by `decide_block` but ahead of the next non-text event — here
    // that happens to be `End(Item)`. Note the absence of a `<p>`.
    let result = render_status("- item :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<ul><li>item <span class="status status-green">X</span></li></ul>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn inline_directive_inside_a_heading_reaches_the_slug_and_toc() {
    // A heading routes text into two side buffers, and the run is flushed
    // while the heading scope is still on the stack. A refactor that flushed
    // after popping the scope would write the text into the document body
    // instead and silently change the slug — which the HTML assertion alone
    // would not localize, hence the ToC assertions.
    //
    // That the badge *label* becomes part of the slug and the ToC title is
    // pinned as-is and is arguably questionable.
    let result = render_status("# head :status[X]{color=green}");
    assert_eq!(
        result.html,
        r#"<h1 id="head-x">head <span class="status status-green">X</span></h1>"#
    );
    assert_eq!(result.toc.len(), 1);
    assert_eq!(result.toc[0].id, "head-x");
    assert_eq!(result.toc[0].title, "head X");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn container_opening_line_split_by_cmark_is_still_recognized() {
    // `:::tab[Label]{#a}` arrives as five `Text` events; the container is
    // recognized only because the paragraph's runs are joined before
    // `parse_container_line` sees them.
    //
    // The `{#a}` is inert — directive args skip `#id` tokens, so the id is
    // the generated `tabs-0`. It is here solely to force the extra split.
    let result = render_tabs("::::tabs\n\n:::tab[Label]{#a}\n\nBody.\n\n:::\n\n::::");
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains(">Label</button>"),
        "got: {}",
        result.html
    );
    assert!(result.html.contains("<p>Body.</p>"), "got: {}", result.html);
    assert!(
        !result.html.contains("<p>:::"),
        "delimiter leaked as literal text: {}",
        result.html
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}
