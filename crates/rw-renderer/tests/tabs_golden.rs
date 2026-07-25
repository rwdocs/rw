//! Byte-identical golden tests for tab HTML. The viewer styles this markup and
//! published bundles carry it, so a change here is user-visible.

use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

fn render(md: &str) -> rw_renderer::RenderResult {
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new())
}

#[test]
fn two_tab_group_full_markup() {
    let r = render("::::tabs\n\n:::tab[macOS]\n\nA\n\n:::\n\n:::tab[Linux]\n\nB\n\n:::\n\n::::");
    // Pin the exact bytes: bar (buttons) spliced before panels, ids, hidden on non-first.
    assert_eq!(
        r.html,
        "<div class=\"tabs\" id=\"tabs-0\"><div class=\"tabs-buttons\" role=\"tablist\">\
<button role=\"tab\" id=\"tab-0-0\" aria-controls=\"panel-0-0\" aria-selected=\"true\" tabindex=\"0\">macOS</button>\
<button role=\"tab\" id=\"tab-0-1\" aria-controls=\"panel-0-1\" aria-selected=\"false\" tabindex=\"-1\">Linux</button>\
</div>\
<div role=\"tabpanel\" id=\"panel-0-0\" aria-labelledby=\"tab-0-0\"><p>A</p></div>\
<div role=\"tabpanel\" id=\"panel-0-1\" aria-labelledby=\"tab-0-1\" hidden><p>B</p></div>\
</div>"
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn label_escaped_and_quotes_stripped() {
    let r = render("::::tabs\n\n:::tab[a < b & c]\n\nx\n\n:::\n\n::::");
    assert!(r.html.contains("a &lt; b &amp; c"), "{}", r.html);
}

#[test]
fn empty_group_bar_without_buttons_warns() {
    let r = render("::::tabs\n\n::::");
    assert!(r.html.contains(r#"role="tablist""#), "{}", r.html);
    assert!(!r.html.contains("<button"), "{}", r.html);
    assert!(r.warnings.iter().any(|w| w.contains("no `:::tab` items")));
}

#[test]
fn lone_tab_unwrapped_warns() {
    let r = render(":::tab[X]\n\nbody\n\n:::");
    assert!(!r.html.contains(r#"role="tablist""#), "{}", r.html);
    assert!(r.html.contains("body"), "{}", r.html);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("outside a `::::tabs`"))
    );
}
