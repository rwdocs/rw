//! Byte-identical golden tests for the generic container-directive machinery.
//! These pin behavior across the Stage-1 parser-pairing refactor: they must be
//! green before AND after. A registered `note` container plus unregistered /
//! unclosed / nested cases exercise every pairing path.

use rw_renderer::directive::{
    ContainerDirective, DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor,
};
use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

/// Minimal registered container: `:::note` → `<div class="note">…</div>`.
struct Note;
impl ContainerDirective for Note {
    fn name(&self) -> &'static str {
        "note"
    }
    fn start(&mut self, _a: DirectiveArgs, _c: &DirectiveContext) -> DirectiveOutput {
        DirectiveOutput::html(r#"<div class="note">"#.to_owned())
    }
    fn end(&mut self, _line: usize) -> Option<String> {
        Some("</div>".to_owned())
    }
}

fn render(md: &str) -> rw_renderer::RenderResult {
    let processor = DirectiveProcessor::new().with_container(Note);
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(processor))
}

#[test]
fn nested_registered_containers() {
    let r = render(":::note\n\ninner\n\n:::note\n\ndeep\n\n:::\n\n:::");
    assert_eq!(
        r.html,
        r#"<div class="note"><p>inner</p><div class="note"><p>deep</p></div></div>"#
    );
    assert!(r.warnings.is_empty(), "warnings: {:?}", r.warnings);
}

#[test]
fn unclosed_at_eof_is_balanced_and_warns() {
    let r = render(":::note\n\nbody\n");
    assert_eq!(r.html, r#"<div class="note"><p>body</p></div>"#);
    assert!(
        r.warnings.iter().any(|w| w.contains("unclosed")),
        "warnings: {:?}",
        r.warnings
    );
}

#[test]
fn unclosed_inside_blockquote_closes_at_block_end() {
    let r = render("> :::note\n>\n> body\n\nafter\n");
    assert_eq!(
        r.html,
        r#"<blockquote><div class="note"><p>body</p></div></blockquote><p>after</p>"#
    );
    assert!(r.warnings.iter().any(|w| w.contains("unclosed")));
}

#[test]
fn unclosed_inside_list_item_closes_at_item_end() {
    let r = render("- :::note\n\n  body\n\n- next\n");
    // The note must close before the item does, never crossing `</li>`.
    assert_eq!(
        r.html,
        r#"<ul><li><div class="note"><p>body</p></div></li><li><p>next</p></li></ul>"#
    );
    assert!(r.warnings.iter().any(|w| w.contains("unclosed")));
}

#[test]
fn stray_close_renders_literally_and_warns() {
    let r = render("text\n\n:::\n");
    assert_eq!(r.html, "<p>text</p><p>:::</p>");
    assert!(r.warnings.iter().any(|w| w.contains("stray")));
}

#[test]
fn two_nested_unclosed_inside_blockquote_close_innermost_first() {
    let r = render("> :::note\n>\n> :::note\n>\n> body\n\nafter\n");
    assert_eq!(
        r.html,
        r#"<blockquote><div class="note"><div class="note"><p>body</p></div></div></blockquote><p>after</p>"#
    );
    assert!(
        r.warnings.iter().filter(|w| w.contains("unclosed")).count() == 2,
        "warnings: {:?}",
        r.warnings
    );
}

#[test]
fn unregistered_open_close_pair_is_literal_no_warn() {
    let r = render(":::xyz\n\nbody\n\n:::\n");
    assert_eq!(r.html, "<p>:::xyz</p><p>body</p><p>:::</p>");
    assert!(
        !r.warnings
            .iter()
            .any(|w| w.contains("stray") || w.contains("unclosed"))
    );
}
