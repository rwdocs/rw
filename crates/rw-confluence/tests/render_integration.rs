//! Integration tests for `rw_confluence::render`.

use rw_confluence::{RenderOptions, render};

#[test]
fn render_writes_page_xhtml_for_plain_markdown() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let markdown = "# Hello\n\nA paragraph.\n";
    let output = render(markdown, out, RenderOptions::default()).expect("render succeeded");

    // page.xhtml contains the rendered body.
    let xhtml = std::fs::read_to_string(out.join("page.xhtml")).expect("page.xhtml exists");
    assert!(xhtml.contains("<h1"), "missing h1: {xhtml}");
    assert!(xhtml.contains("A paragraph"), "missing body text: {xhtml}");
    assert_eq!(xhtml, output.xhtml, "in-memory xhtml differs from file");

    // No diagrams → no PNGs, no attachments.
    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );

    // title is None with default options (extract_title=false).
    assert!(output.title.is_none());

    // No current_xhtml → no preservation, no unmatched comments.
    assert!(output.unmatched_comments.is_empty());

    // No warnings emitted for plain markdown without diagrams.
    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);

    // Regression guard: the bundle layout is page.xhtml + PNGs only — no
    // manifest.json on disk.
    assert!(
        !out.join("manifest.json").exists(),
        "manifest.json should not be written"
    );
}

#[test]
fn render_creates_out_dir_if_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let nested = tmp
        .path()
        .join("nested")
        .join("does")
        .join("not")
        .join("exist");

    let output =
        render("# Hi\n", &nested, RenderOptions::default()).expect("render created the dir");

    assert!(nested.join("page.xhtml").exists());
    assert!(!output.xhtml.is_empty());
}

#[test]
fn render_preserves_inline_comment_markers_when_current_xhtml_provided() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let markdown = "Hello marked text here.\n";
    let current_xhtml = "<p>Hello \
        <ac:inline-comment-marker ac:ref=\"abc\">marked text</ac:inline-comment-marker> \
        here.</p>";

    let opts = RenderOptions {
        current_xhtml: Some(current_xhtml.to_owned()),
        ..RenderOptions::default()
    };
    let output = render(markdown, out, opts).expect("render");

    assert!(output.xhtml.contains("ac:inline-comment-marker"));
    assert!(output.xhtml.contains(r#"ac:ref="abc""#));
    assert!(output.unmatched_comments.is_empty());
}

#[test]
fn render_reports_unmatched_comments_when_text_changed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let markdown = "Different content entirely.\n";
    let current_xhtml = "<p><ac:inline-comment-marker ac:ref=\"xyz\">\
        Original sentence here</ac:inline-comment-marker></p>";

    let opts = RenderOptions {
        current_xhtml: Some(current_xhtml.to_owned()),
        ..RenderOptions::default()
    };
    let output = render(markdown, out, opts).expect("render");

    assert_eq!(output.unmatched_comments.len(), 1);
    assert_eq!(output.unmatched_comments[0].ref_id, "xyz");
    assert_eq!(output.unmatched_comments[0].text, "Original sentence here");
}

#[test]
fn render_drops_stale_pngs_from_previous_run() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // Simulate a previous render that left multiple stale PNGs behind,
    // plus one non-PNG file that should be left alone.
    std::fs::write(out.join("diagram-stale-a.png"), b"PNG").expect("write a");
    std::fs::write(out.join("diagram-stale-b.png"), b"PNG").expect("write b");
    std::fs::write(out.join("notes.txt"), b"keep me").expect("write notes");

    let output = render("# Hi\n", out, RenderOptions::default()).expect("render");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert!(
        !out.join("diagram-stale-a.png").exists(),
        "stale a not removed"
    );
    assert!(
        !out.join("diagram-stale-b.png").exists(),
        "stale b not removed"
    );
    assert!(
        out.join("notes.txt").exists(),
        "non-PNG file should be left alone"
    );
}

#[test]
fn render_ignores_subdirectories_ending_in_png() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // Simulate a publisher cache directory named like a PNG file.
    std::fs::create_dir(out.join("cache.png")).expect("mkdir");

    let output = render("# Hi\n", out, RenderOptions::default()).expect("render");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert!(
        out.join("cache.png").is_dir(),
        "directory should still exist"
    );
}

#[test]
fn render_surfaces_preservation_warning_on_malformed_current_xhtml() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let opts = RenderOptions {
        current_xhtml: Some("<p>unclosed paragraph".to_owned()),
        ..RenderOptions::default()
    };
    let output = render("# Hi\n", out, opts).expect("render");

    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.contains("comment preservation skipped")),
        "got: {:?}",
        output.warnings
    );
}

#[test]
fn render_status_directive_emits_native_status_macro() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let markdown = "Ship :status[On Track]{color=green} now.\n";
    let output = render(markdown, out, RenderOptions::default()).expect("render succeeded");

    assert!(
        output.xhtml.contains(concat!(
            r#"<ac:structured-macro ac:name="status" ac:schema-version="1">"#,
            r#"<ac:parameter ac:name="colour">Green</ac:parameter>"#,
            r#"<ac:parameter ac:name="title">On Track</ac:parameter>"#,
            r#"</ac:structured-macro>"#,
        )),
        "got: {}",
        output.xhtml
    );

    // The HTML backend's span must never reach Confluence storage format.
    assert!(
        !output.xhtml.contains("class=\"status"),
        "html markup leaked: {}",
        output.xhtml
    );
}

#[test]
fn render_status_directive_escapes_label_and_defaults_color() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // No color attribute, and a label needing XML escaping.
    let markdown = "State :status[A & B].\n";
    let output = render(markdown, out, RenderOptions::default()).expect("render succeeded");

    assert!(
        output
            .xhtml
            .contains(r#"<ac:parameter ac:name="colour">Grey</ac:parameter>"#),
        "expected default Grey: {}",
        output.xhtml
    );
    assert!(
        output.xhtml.contains("A &amp; B"),
        "label not escaped: {}",
        output.xhtml
    );
}
