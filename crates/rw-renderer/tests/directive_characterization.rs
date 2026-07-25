//! Golden lock on rendered output + warnings for rw's directive syntax.
//!
//! These strings are user-visible: the HTML is what the viewer styles and what
//! published bundles carry, and the warnings are what `--strict` publishing
//! prints. Changing any of them is a behavior change that needs a CHANGELOG
//! entry — never edit a golden to make a test pass.

use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline, render_comment_body};

/// Render the way a page is rendered: an empty pipeline, no code-block
/// processors.
fn page_render(md: &str) -> (String, Vec<String>) {
    let out = MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new());
    (out.html, out.warnings)
}

const STATUS_PLAIN: &str = ":status[Done]";
const STATUS_COLOR: &str = ":status[Done]{color=green}";
const TABS: &str =
    "::::tabs\n\n:::tab[macOS]\n\nmac body\n\n:::\n\n:::tab[Linux]\n\nlin body\n\n:::\n\n::::";
const TABS_LONE: &str = ":::tab[Solo]\n\nbody\n\n:::";
const TABS_UNCLOSED: &str = "::::tabs\n\n:::tab[A]\n\nbody";
const TABS_EMPTY: &str = "::::tabs\n\n::::";
const UNKNOWN_INLINE: &str = "Text :foo[bar] more.";
const UNKNOWN_LEAF: &str = "::foo[bar]";
const UNKNOWN_CONTAINER: &str = ":::foo\n\nbody\n\n:::";
const CONTAINER_WITH_STATUS: &str = ":::foo[:status[X]]";
const STRAY_CLOSE: &str = "before\n\n:::\n\nafter";

#[test]
fn page_html_golden() {
    assert_eq!(
        page_render(STATUS_PLAIN).0,
        r#"<p><span class="status status-grey">Done</span></p>"#
    );
    assert_eq!(
        page_render(STATUS_COLOR).0,
        r#"<p><span class="status status-green">Done</span></p>"#
    );
    assert_eq!(
        page_render(TABS).0,
        r#"<div class="tabs" id="tabs-0"><div class="tabs-buttons" role="tablist"><button role="tab" id="tab-0-0" aria-controls="panel-0-0" aria-selected="true" tabindex="0">macOS</button><button role="tab" id="tab-0-1" aria-controls="panel-0-1" aria-selected="false" tabindex="-1">Linux</button></div><div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0"><p>mac body</p></div><div role="tabpanel" id="panel-0-1" aria-labelledby="tab-0-1" hidden><p>lin body</p></div></div>"#
    );
    assert_eq!(page_render(TABS_LONE).0, "<p>body</p>");
    assert_eq!(
        page_render(TABS_UNCLOSED).0,
        r#"<div class="tabs" id="tabs-0"><div class="tabs-buttons" role="tablist"><button role="tab" id="tab-0-0" aria-controls="panel-0-0" aria-selected="true" tabindex="0">A</button></div><div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0"><p>body</p></div></div>"#
    );
    assert_eq!(
        page_render(TABS_EMPTY).0,
        r#"<div class="tabs" id="tabs-0"><div class="tabs-buttons" role="tablist"></div></div>"#
    );
    assert_eq!(page_render(UNKNOWN_INLINE).0, "<p>Text :foo[bar] more.</p>");
    assert_eq!(page_render(UNKNOWN_LEAF).0, "<p>::foo[bar]</p>");
    assert_eq!(
        page_render(UNKNOWN_CONTAINER).0,
        "<p>:::foo</p><p>body</p><p>:::</p>"
    );
    assert_eq!(
        page_render(CONTAINER_WITH_STATUS).0,
        r#"<p>:::foo[<span class="status status-grey">X</span>]</p>"#
    );
    assert_eq!(
        page_render(STRAY_CLOSE).0,
        "<p>before</p><p>:::</p><p>after</p>"
    );
}

#[test]
fn page_warnings_golden() {
    let no_warnings: Vec<String> = Vec::new();

    assert_eq!(page_render(STATUS_PLAIN).1, no_warnings);
    assert_eq!(page_render(STATUS_COLOR).1, no_warnings);
    assert_eq!(page_render(TABS).1, no_warnings);
    assert_eq!(
        page_render(TABS_LONE).1,
        vec![
            "`:::tab` outside a `::::tabs` group; its content is rendered without tab chrome"
                .to_owned()
        ]
    );
    assert_eq!(
        page_render(TABS_UNCLOSED).1,
        vec![
            "unclosed container directive :::tab (missing closing :::)".to_owned(),
            "unclosed container directive :::tabs (missing closing :::)".to_owned(),
        ]
    );
    assert_eq!(
        page_render(TABS_EMPTY).1,
        vec!["`::::tabs` group has no `:::tab` items".to_owned()]
    );
    assert_eq!(
        page_render(UNKNOWN_INLINE).1,
        vec!["unknown inline directive ':foo'".to_owned()]
    );
    assert_eq!(page_render(UNKNOWN_LEAF).1, no_warnings);
    assert_eq!(page_render(UNKNOWN_CONTAINER).1, no_warnings);
    assert_eq!(page_render(CONTAINER_WITH_STATUS).1, no_warnings);
    assert_eq!(
        page_render(STRAY_CLOSE).1,
        vec!["stray ::: with no opening directive".to_owned()]
    );
}

#[test]
fn comment_body_keeps_directives_literal() {
    // The restricted comment subset must NOT expand directives.
    let s = render_comment_body("Use :status[Done]{color=green} here.");
    assert!(s.contains(":status[Done]{color=green}"), "got: {s}");
    assert!(!s.contains("status-green"), "got: {s}");

    let t = render_comment_body(TABS);
    assert_eq!(
        t,
        "<p>::::tabs</p><p>:::tab[macOS]</p><p>mac body</p><p>:::</p>\
         <p>:::tab[Linux]</p><p>lin body</p><p>:::</p><p>::::</p>"
    );
    assert!(!t.contains("role=\"tablist\""), "got: {t}");
}
