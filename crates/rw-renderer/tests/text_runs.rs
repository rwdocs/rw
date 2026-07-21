//! Characterization tests: rw's directive syntax against pulldown-cmark's
//! text-event splitting.
//!
//! cmark emits a separate `Text` event at every `[` and `]`, and splits again
//! at HTML entities and backslash escapes — `a &amp; b :status[X]{color=green}`
//! arrives as seven events. `Walker` coalesces adjacent runs into `text_buffer`
//! and flushes them through `flush_text` → `parse_line` before any non-text
//! event, so *when* runs are joined decides whether a directive is seen at all.
//!
//! These tests pin what the renderer does **today**, so an upcoming refactor
//! splitting the walker into a tokenizer and an interpreter can be verified
//! byte-identical. Some pinned behavior looks wrong; where it does, the test
//! says so. Nothing here should be "fixed" to make an expectation prettier —
//! that would make a regression indistinguishable from an intentional change.
//!
//! Both directions matter. Tests asserting a directive *expands* fail if a
//! refactor joins too little. The tests whose names say a run was interrupted
//! mid-`[...]` are the ones that fail if it joins too much; each was checked
//! by sabotaging the coalescing logic and confirming it went red. Not every
//! literal-asserting test discriminates — some produce identical output
//! either way, and say so.

use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{
    HtmlBackend, MarkdownRenderer, Pipeline, RenderResult, StatusDirective, TabsDirective,
};

fn render_status(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_inline(StatusDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

fn render_tabs(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

/// `with_wikilinks` alone is enough to exercise `skip_wikilink_text`: an
/// unresolved wikilink takes the `Broken` arm, which still sets the flag.
/// Deliberately avoids needing a populated `Sections` — `rw-renderer`
/// re-exports the `Sections` type but not the `Section` and `Namespace`
/// needed to fill one, and `rw-sections` is not a dev-dependency here.
fn render_status_wikilinks(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_inline(StatusDirective::new());
    MarkdownRenderer::<HtmlBackend>::new()
        .with_wikilinks(true)
        .render(md, Pipeline::new().with_directives(directives))
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
fn escaped_character_inside_a_directive_name_yields_no_directive() {
    // `:foo\*bar` splits as `Text(":foo")`, `Text("*bar")`. Joined, the name
    // candidate is `foo*bar`, which `is_valid_directive_name` rejects — so
    // nothing dispatches and nothing warns.
    //
    // This is the sharpest join/no-join discriminator in the file: flushing
    // per event would instead see `:foo` on its own, find a *valid* name,
    // dispatch it, get `Skip`, and emit a spurious warning.
    let result = render_status(r":foo\*bar");
    assert_eq!(result.html, "<p>:foo*bar</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unregistered_inline_directive_passes_through_verbatim_and_warns() {
    // The literal is the raw source slice, so attribute order and spacing
    // survive exactly as written — `{b=2 a=1 .cls #id}`, not a reconstruction.
    let result = render_status(":unknown[body]{b=2 a=1 .cls #id}");
    assert_eq!(result.html, "<p>:unknown[body]{b=2 a=1 .cls #id}</p>");
    assert_eq!(
        result.warnings,
        vec![
            "unknown inline directive ':unknown' — no handler registered (or handler returned Skip)"
        ]
    );
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
    // A colon followed by a space never starts a directive. The companion
    // case — a colon followed immediately by a letter — is
    // `bare_directive_name_in_prose_warns` below, and behaves differently.
    let result = render_status("Note: see below");
    assert_eq!(result.html, "<p>Note: see below</p>");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn a_directive_cannot_straddle_a_soft_break() {
    // The run ends at the `SoftBreak`, so `:sta` is scanned without the
    // `tus[X]{...}` that follows it on the next line, dispatches with no
    // content, and warns.
    //
    // Note what this test does *not* prove. Joining the runs across the soft
    // break yields `one :sta\ntus[X]{color=green}`, in which `:sta` is still
    // not followed by `[` — so it still dispatches content-less and still
    // warns, and the output is identical. Verified by sabotaging
    // `should_buffer` to buffer across `SoftBreak`: this test passed unchanged.
    // The discriminating case is
    // `bracket_content_interrupted_by_a_soft_break_matches_only_the_name`
    // below.
    let result = render_status("one :sta\ntus[X]{color=green}");
    assert_eq!(result.html, "<p>one :sta\ntus[X]{color=green}</p>");
    assert_eq!(
        result.warnings,
        vec!["unknown inline directive ':sta' — no handler registered (or handler returned Skip)"]
    );
}

#[test]
fn bracket_content_interrupted_by_a_soft_break_matches_only_the_name() {
    // The sharpest over-joining guard in the file. The run ends mid-`[...]`,
    // so the scanner matches the *name* `:status` but never its content: the
    // directive dispatches empty and the brackets survive as literal text.
    //
    // Join the runs across the soft break and the scanner sees the whole
    // `:status[X\ny]{color=green}`, rendering `status-green` with content
    // `X\ny` and no literal tail. Confirmed by sabotage — this assertion is
    // what fails when `should_buffer` is widened to cover `SoftBreak`.
    //
    // The empty `status-grey` badge is the same content-less dispatch pinned
    // by `bare_registered_directive_name_in_prose_renders_an_empty_badge`,
    // and is likewise pinned as-is rather than fixed.
    let result = render_status("a :status[X\ny]{color=green}");
    assert_eq!(
        result.html,
        "<p>a <span class=\"status status-grey\"></span>[X\ny]{color=green}</p>"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn heading_directive_interrupted_mid_bracket_leaks_the_literal_into_the_slug() {
    // Combines the two seams the tests around it cover separately: the run is
    // interrupted mid-`[...]` (so only the name matches and the brackets stay
    // literal) *and* the heading routes text into two side buffers. The
    // leftover has to land in both, and it does — including in the slug.
    //
    // PINNED AS-IS — a slug of `a-x-em-ycolorgreen` is not a reasonable
    // anchor for this heading, but it is what the renderer produces today.
    // Guards against a refactor routing the partial-match fallback text into
    // the rendered-HTML buffer but not the plain-text one, which would change
    // the anchor and the ToC entry while leaving the visible HTML plausible.
    let result = render_status("# a :status[X *em* y]{color=green}");
    assert_eq!(
        result.html,
        r#"<h1 id="a-x-em-ycolorgreen">a <span class="status status-grey"></span>[X <em>em</em> y]{color=green}</h1>"#
    );
    assert_eq!(result.toc.len(), 1);
    assert_eq!(result.toc[0].id, "a-x-em-ycolorgreen");
    assert_eq!(result.toc[0].title, "a [X em y]{color=green}");
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn container_delimiter_in_a_tight_list_item_is_silently_literal() {
    // A tight list item emits no `Start(Paragraph)`, so the paragraph state
    // never becomes `Deferred` and the block-directive check inside
    // `finish_pending_paragraph` is never reached at all. The delimiter is
    // just text.
    //
    // PINNED AS-IS — this looks like a real defect, deliberately recorded
    // rather than fixed, so that the refactor this file guards stays
    // byte-identical. Unlike the interrupted-opener case below, this one is
    // *completely* silent: no tab group, and no warning either, so `--strict`
    // publishing cannot catch it.
    //
    // The gating is an accident of `ParagraphState`, not a decision, which is
    // exactly the kind of quirk a tokenizer/interpreter split could drop or
    // inadvertently "fix" without anyone noticing.
    let result = render_tabs("- :::tab[Label]{#a}\n- next item\n");
    assert_eq!(
        result.html,
        "<ul><li>:::tab[Label]{#a}</li><li>next item</li></ul>"
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn container_opening_line_interrupted_by_emphasis_is_not_recognized() {
    // `process_event` answers "keep buffering?" in two structurally separate
    // places: the `still_buffering` check that decides whether a deferred
    // paragraph stays deferred, and the `should_buffer` check that drives the
    // inline path. Both hold only while the run is still pure text, so
    // emphasis in the label ends the deferral and the opener is never parsed
    // as a container.
    //
    // PINNED AS-IS — the consequences are worth noting. The whole tab group
    // silently degrades to literal paragraphs, and the *closing* `:::` then
    // reports `stray ::: with no opening directive`, which points a reader at
    // the wrong line entirely. The opener that actually failed says nothing.
    //
    // `still_buffering` gates *every* paragraph, not only `:::` openers, so
    // it is not covered by this test alone: widening just that predicate to
    // hold across emphasis also breaks
    // `bracket_content_interrupted_by_emphasis_matches_only_the_name` and
    // `inline_directive_after_emphasis_still_expands`. What is unique here is
    // the outcome — this is the only test where losing the deferral costs a
    // whole *container*, rather than one inline expansion. A refactor
    // consolidating the two predicates should expect all three to move.
    let result = render_tabs(":::tab[Label *em* more]{#a}\n\nBody.\n\n:::");
    assert_eq!(
        result.html,
        "<p>:::tab[Label <em>em</em> more]{#a}</p><p>Body.</p><p>:::</p>"
    );
    assert_eq!(result.warnings, vec!["stray ::: with no opening directive"]);
}

#[test]
fn bracket_content_interrupted_by_emphasis_matches_only_the_name() {
    // Same shape, but the run is broken by a non-`Text` event rather than a
    // soft break — the case a tokenizer is most likely to get wrong, since
    // joining across inline markup looks locally harmless. Joining here would
    // turn `[X <em>em</em> y]` into a badge's content and swallow the markup.
    let result = render_status("a :status[X *em* y]{color=green}");
    assert_eq!(
        result.html,
        r#"<p>a <span class="status status-grey"></span>[X <em>em</em> y]{color=green}</p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn a_directive_cannot_straddle_a_wikilink() {
    // Exercises `skip_wikilink_text`, the only term of the `should_buffer`
    // predicate with no other test at this seam. The wikilink is deliberately
    // unresolved, so it renders as a broken link and needs no `Sections`.
    let result = render_status_wikilinks("a :sta[[foo]]tus[X]{color=green}");
    assert_eq!(
        result.html,
        r##"<p>a :sta<a href="#" class="rw-broken-link">foo</a>tus[X]{color=green}</p>"##
    );
    assert_eq!(
        result.warnings,
        vec!["unknown inline directive ':sta' — no handler registered (or handler returned Skip)"]
    );
}

#[test]
fn bare_directive_name_in_prose_warns() {
    // PINNED AS-IS — this looks like a real defect, deliberately
    // recorded rather than fixed, so that the refactor this file guards
    // stays byte-identical.
    //
    // A colon immediately followed by a letter is scanned as a directive even
    // with no `[content]` at all, so ordinary prose containing `:sta` emits an
    // unknown-directive warning and fails `--strict` publishing. No text
    // splitting is involved; this is here because it is what makes
    // `a_directive_cannot_straddle_a_soft_break` warn, and without it that
    // warning reads like a text-split artifact.
    let result = render_status("just :sta here");
    assert_eq!(result.html, "<p>just :sta here</p>");
    assert_eq!(
        result.warnings,
        vec!["unknown inline directive ':sta' — no handler registered (or handler returned Skip)"]
    );
}

#[test]
fn bare_registered_directive_name_in_prose_renders_an_empty_badge() {
    // PINNED AS-IS — this looks like a real defect, deliberately
    // recorded rather than fixed, so that the refactor this file guards
    // stays byte-identical.
    //
    // Same scan as above, but the name *is* registered, so it dispatches with
    // empty content and silently turns a word of prose into an empty status
    // badge. No warning is emitted, so `--strict` does not catch it either.
    let result = render_status("see :status here");
    assert_eq!(
        result.html,
        r#"<p>see <span class="status status-grey"></span> here</p>"#
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
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
    // A tight list item emits no `Start(Paragraph)` at all, so the buffered
    // run is not drained by `finish_pending_paragraph`. It is drained by the
    // generic pre-dispatch flush, which fires before *any* non-text event —
    // here that happens to be `End(Item)`. Note the absence of a `<p>`.
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
    // The `{#a}` is inert — `TabsDirective` never reads `args.id`, so the id
    // is the generated `tabs-0`. It is here solely to force the extra split.
    let result = render_tabs(":::tab[Label]{#a}\n\nBody.\n\n:::");
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
