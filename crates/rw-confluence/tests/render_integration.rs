//! Integration tests for `rw_confluence::render`.

use std::error::Error;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use quick_xml::Reader;
use quick_xml::events::Event;
use rw_confluence::{RenderOptions, RenderOutput, render};
use rw_error::error_chain;

// ── A Kroki that answers over loopback ────────────────────────────────────

/// A stand-in Kroki: answers every POST with the same PNG, and records the
/// path and body of each request so a test can see what was actually asked for.
///
/// `rw-kroki`'s own stub is `#[cfg(test)]`-private to that crate, so this is
/// the integration-test copy. Bound to an ephemeral loopback port; the serving
/// thread stops and is joined on drop.
struct FakeKroki {
    /// Base URL to point [`RenderOptions::kroki_url`] at.
    url: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeKroki {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the fake Kroki");
        let url = format!(
            "http://{}",
            listener.local_addr().expect("the fake Kroki's address")
        );
        listener
            .set_nonblocking(true)
            .expect("poll the fake Kroki's listener");

        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(&stream, &requests),
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// The `(path, body)` of each request served so far.
    fn requests(&self) -> Vec<(String, String)> {
        self.requests.lock().clone()
    }
}

impl Drop for FakeKroki {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A 400x200 PNG signature and IHDR chunk — all that any size probe reads.
fn png_bytes() -> Vec<u8> {
    Vec::from([
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR
        0x00, 0x00, 0x01, 0x90, // width 400
        0x00, 0x00, 0x00, 0xC8, // height 200
        0x08, 0x06, 0x00, 0x00, 0x00, // bit depth, colour type, …
    ])
}

/// Answer one request, recording its path and body.
fn serve(stream: &TcpStream, requests: &Mutex<Vec<(String, String)>>) {
    stream
        .set_nonblocking(false)
        .expect("read the fake Kroki connection");
    // Without this a client that connects and then stalls blocks this thread
    // forever, and the join in `Drop` behind it — a hung test run with nothing
    // to time it out.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bound the fake Kroki's read");
    let mut reader = BufReader::new(stream.try_clone().expect("clone the connection"));

    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("a request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).expect("a header") == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().expect("a numeric Content-Length");
        }
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).expect("the posted source");
    let body = String::from_utf8(body).expect("UTF-8 diagram source");
    requests.lock().push((path, body));

    let png = png_bytes();
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        png.len()
    )
    .expect("write the response head");
    stream.write_all(&png).expect("write the response");
    stream.flush().expect("flush the response");
}

#[test]
fn declared_metadata_name_does_not_change_confluence_title() {
    for metadata in ["kind: system", "kind: system\nname: payments-api"] {
        let tmp = tempfile::tempdir().unwrap();
        let markdown =
            format!("---\n{metadata}\ntitle: Metadata title\n---\n# Confluence heading\n\nBody.\n");
        let output = render(
            &markdown,
            tmp.path(),
            RenderOptions {
                extract_title: true,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        // With title extraction enabled, the first H1 supplies the export title;
        // metadata declarations must not override it.
        assert_eq!(output.title.as_deref(), Some("Confluence heading"));
    }
}

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
    assert_eq!(
        output.unmatched_comments,
        [] as [rw_confluence::UnmatchedComment; 0]
    );

    // No warnings emitted for plain markdown without diagrams.
    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);

    // The bundle layout is page.xhtml + PNGs only.
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
    assert_ne!(output.xhtml, "");
}

#[test]
fn render_error_names_the_directory_it_could_not_create() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("file");
    std::fs::write(&file, "").expect("write blocking file");
    let out_dir = file.join("dist");

    let error = render("# Hi\n", &out_dir, RenderOptions::default()).expect_err("render failed");

    assert_eq!(
        error.to_string(),
        format!("cannot create directory {}", out_dir.display())
    );
    let cause = error.source().expect("the I/O error is the source");
    assert_eq!(error_chain(&error), format!("{error}: {cause}"));
}

#[test]
fn render_error_names_the_page_it_could_not_write() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let page = tmp.path().join("page.xhtml");
    std::fs::create_dir(&page).expect("blocking dir");

    let error = render("# Hi\n", tmp.path(), RenderOptions::default()).expect_err("render failed");

    assert_eq!(
        error.to_string(),
        format!("cannot write {}", page.display())
    );
    let cause = error.source().expect("the I/O error is the source");
    assert_eq!(error_chain(&error), format!("{error}: {cause}"));
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
    assert_eq!(
        output.unmatched_comments,
        [] as [rw_confluence::UnmatchedComment; 0]
    );
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

/// The bundle directory is the publish surface: `docs/confluence.md` documents
/// the upload step as a `dist/*.png` glob, so anything left in there from an
/// earlier render is uploaded as a phantom attachment. Re-rendering into a
/// reused directory has to clear it out — while leaving alone everything that
/// is not a diagram this render produced.
///
/// The attachment list itself cannot name a file it did not write, because
/// nothing reads the directory to build it. So the risk this guards is the
/// publisher's glob, not rw's bookkeeping.
#[test]
fn render_drops_stale_pngs_from_previous_run() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // Simulate a previous render that left multiple stale PNGs behind,
    // plus one non-PNG file that should be left alone.
    std::fs::write(out.join("diagram-stale-a.png"), b"PNG").expect("write a");
    std::fs::write(out.join("diagram-stale-b.png"), b"PNG").expect("write b");
    std::fs::write(out.join("notes.txt"), b"keep me").expect("write notes");
    // A publisher cache directory named like a PNG file: a directory entry
    // ending in `.png` must not be mistaken for a leftover diagram.
    std::fs::create_dir(out.join("cache.png")).expect("mkdir");

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
    assert!(
        out.join("cache.png").is_dir(),
        "a directory ending in .png should be left alone"
    );
}

/// The attachment filenames are published output: Confluence keys attachments
/// by name, so renaming one re-uploads every diagram on every page and breaks
/// any link into them. The hashes below are spelled out rather than recomputed,
/// so a change to how they are derived fails here instead of on the next
/// publish.
///
/// `PlantUML` on purpose. Preprocessing is the identity for Mermaid, so a
/// Mermaid fixture would let a change that hashes the raw fence body instead of
/// the prepared source — which renames every published attachment — pass
/// unnoticed. The premise assertion below keeps this fixture from quietly
/// degrading into that state.
#[test]
fn render_names_attachments_from_the_prepared_source_digest() {
    let kroki = FakeKroki::start();
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // Two diagrams whose names sort the other way round from the order they
    // appear in: the page must keep document order, the attachment list must
    // come back sorted, and neither can be mistaken for the other.
    let markdown = concat!(
        "# Diagrams\n\n",
        "```plantuml\n@startuml\nGrace -> Heidi\n@enduml\n```\n\n",
        "```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n",
    );
    let opts = RenderOptions {
        kroki_url: Some(kroki.url.clone()),
        ..RenderOptions::default()
    };
    let output = render(markdown, out, opts).expect("render succeeded");

    let grace = "diagram_17153cbed263.png";
    let alice = "diagram_088b7fb76307.png";

    assert_eq!(
        output.attachments,
        [alice, grace],
        "the attachment list must be exactly the two files written, sorted",
    );
    for name in [grace, alice] {
        assert!(out.join(name).exists(), "{name} was not written to disk");
        assert_eq!(
            std::fs::read(out.join(name)).expect("read the attachment"),
            png_bytes(),
            "{name} does not hold the rendered PNG",
        );
    }

    for name in [grace, alice] {
        assert!(
            output
                .xhtml
                .contains(&format!(r#"<ri:attachment ri:filename="{name}" />"#)),
            "the XHTML does not reference {name}: {}",
            output.xhtml,
        );
        // The published size, end to end: Kroki answered 400x200, PlantUML
        // renders at 192 DPI, so the display width is half of it. Drop the size
        // anywhere between the provider and the markup and every diagram on
        // every published page comes out at twice its intended size.
        assert!(
            output.xhtml.contains(&format!(
                r#"<ac:image ac:width="200"><ri:attachment ri:filename="{name}" /></ac:image>"#
            )),
            "the 400px render at 192 DPI must publish at ac:width=\"200\": {}",
            output.xhtml,
        );
    }

    // Document order on the page, which is the opposite of the sorted list
    // above — so a list that came back in document order would be visible.
    let grace_at = output.xhtml.find(grace).expect("the first diagram");
    let alice_at = output.xhtml.find(alice).expect("the second diagram");
    assert!(
        grace_at < alice_at,
        "the figures are out of document order: {}",
        output.xhtml,
    );

    // This fixture's own premise, twice over.
    let requests = kroki.requests();
    assert_eq!(requests.len(), 2, "got: {requests:?}");
    for (path, body) in &requests {
        // Confluence publishes PNG whatever the fence asked for, and these
        // fences asked for nothing — the default is SVG.
        assert_eq!(path, "/plantuml/png", "the render was not forced to PNG");
        // What Kroki was sent is what the digest was computed over. If it were
        // ever the raw fence body, the names above would be wrong and this
        // fixture would have stopped exercising preprocessing at all.
        assert!(
            body.contains("skinparam dpi 192"),
            "the posted source is not the prepared source: {body}",
        );
        assert!(
            !markdown.contains("skinparam"),
            "the fence body must not already carry what preprocessing injects",
        );
    }

    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);
}

/// A diagram that cannot be written out fails alone: it becomes an error figure
/// and the page still publishes, with the other diagram's attachment intact.
#[test]
fn render_survives_a_diagram_it_cannot_write() {
    let kroki = FakeKroki::start();
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    // A directory where one of the two PNGs wants to be. The write cannot
    // succeed, on any platform, without depending on file permissions.
    std::fs::create_dir_all(out.join("diagram_17153cbed263.png")).expect("blocking dir");

    let markdown = concat!(
        "```plantuml\n@startuml\nGrace -> Heidi\n@enduml\n```\n\n",
        "```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n",
    );
    let opts = RenderOptions {
        kroki_url: Some(kroki.url.clone()),
        ..RenderOptions::default()
    };
    let output = render(markdown, out, opts).expect("the page still renders");

    assert_eq!(
        output.attachments,
        ["diagram_088b7fb76307.png"],
        "only the diagram that was written may be claimed",
    );
    assert!(
        output.xhtml.contains("Diagram rendering failed"),
        "the failed diagram should show as an error figure: {}",
        output.xhtml,
    );
    assert!(
        output
            .xhtml
            .contains("writing diagram_17153cbed263.png failed"),
        "the error figure should say what could not be written: {}",
        output.xhtml,
    );
    assert!(
        output
            .xhtml
            .contains(r#"<ri:attachment ri:filename="diagram_088b7fb76307.png" />"#),
        "the surviving diagram must still be referenced: {}",
        output.xhtml,
    );
    assert_warned_once_per_error_figure(&output, 1);
    assert!(
        output.warnings[0].contains("writing diagram_17153cbed263.png failed: "),
        "the warning should say what could not be written: {:?}",
        output.warnings,
    );
}

/// An unreachable Kroki fails every diagram and the page still renders, but
/// not silently: the error figures go to readers, so the operator (and
/// `--strict`) needs warnings.
#[test]
fn render_warns_for_each_diagram_kroki_could_not_render() {
    // Bound and dropped so nothing listens there: the connection is refused
    // at once, with no timeout to wait out.
    let url = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve a port");
        format!("http://{}", listener.local_addr().expect("its address"))
    };
    let tmp = tempfile::tempdir().expect("tempdir");

    let markdown = concat!(
        "```plantuml\n@startuml\nGrace -> Heidi\n@enduml\n```\n\n",
        "```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n",
    );
    let opts = RenderOptions {
        kroki_url: Some(url),
        ..RenderOptions::default()
    };
    let output = render(markdown, tmp.path(), opts).expect("the page still renders");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert_warned_once_per_error_figure(&output, 2);
}

/// `count` warnings, one per error figure, each the very text its figure
/// shows, so the number in a warning is the one an author finds on the page.
fn assert_warned_once_per_error_figure(output: &RenderOutput, count: usize) {
    assert_eq!(
        output.xhtml.matches("Diagram rendering failed").count(),
        count,
        "{}",
        output.xhtml,
    );
    assert_eq!(output.warnings.len(), count, "got: {:?}", output.warnings);
    for warning in &output.warnings {
        assert!(
            output
                .xhtml
                .contains(&format!("Diagram rendering failed: {warning}")),
            "no error figure says {warning:?}: {}",
            output.xhtml,
        );
    }
}

/// Without a Kroki URL a diagram fence is an ordinary code block: no request,
/// no attachment, no error figure.
#[test]
fn render_without_kroki_leaves_a_diagram_fence_a_code_block() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let output = render(
        "```plantuml\n@startuml\nA -> B\n@enduml\n```\n",
        out,
        RenderOptions::default(),
    )
    .expect("render");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert!(
        output.xhtml.contains(r#"ac:name="code""#),
        "the fence should render as a code macro: {}",
        output.xhtml,
    );
    assert!(
        !output.xhtml.contains("<ac:image"),
        "no diagram service means no image: {}",
        output.xhtml,
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

/// A page without headings would get an empty TOC macro, so it gets none.
#[test]
fn render_prepends_toc_only_when_the_page_has_headings() {
    let toc = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;
    for (markdown, expected) in [("## Heading\n\nBody.\n", true), ("Body only.\n", false)] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let opts = RenderOptions {
            prepend_toc: true,
            ..RenderOptions::default()
        };
        let output = render(markdown, tmp.path(), opts).expect("render");

        assert_eq!(
            output.xhtml.starts_with(toc),
            expected,
            "{markdown:?}: {}",
            output.xhtml
        );
    }
}

#[test]
fn render_tabs_as_bold_label_sections() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let markdown =
        "::::tabs\n\n:::tab[macOS]\n\nAlpha\n\n:::\n\n:::tab[Linux]\n\nBeta\n\n:::\n\n::::";
    let output = render(markdown, tmp.path(), RenderOptions::default()).expect("render");

    assert_eq!(
        output.xhtml,
        "<p><strong>macOS</strong></p><p>Alpha</p><p><strong>Linux</strong></p><p>Beta</p>"
    );
}

// ── code macro bodies ─────────────────────────────────────────────────────

/// Render `markdown` fresh and again with comment preservation against an
/// unrelated current page.
fn render_fresh_and_preserved(markdown: &str) -> (String, RenderOutput) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fresh = render(markdown, tmp.path(), RenderOptions::default()).expect("fresh render");
    let opts = RenderOptions {
        current_xhtml: Some("<p>old</p>".to_owned()),
        ..RenderOptions::default()
    };
    let preserved = render(markdown, tmp.path(), opts).expect("preserving render");
    (fresh.xhtml, preserved)
}

/// The text of every `ac:plain-text-body` in `xhtml`, panicking if quick-xml
/// rejects it. quick-xml lets a stray `]]>` pass as text, so callers compare
/// the bodies too.
fn code_bodies(xhtml: &str) -> Vec<String> {
    let wrapped = format!(r#"<root xmlns:ac="urn:ac" xmlns:ri="urn:ri">{xhtml}</root>"#);
    let mut reader = Reader::from_str(&wrapped);
    let mut bodies = Vec::new();
    let mut in_body = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == "ac:plain-text-body" => {
                in_body = true;
                bodies.push(String::new());
            }
            Ok(Event::End(e)) if e.name().as_ref() == "ac:plain-text-body" => in_body = false,
            Ok(Event::CData(e)) if in_body => bodies.last_mut().unwrap().push_str(e.as_ref()),
            Ok(Event::Eof) => return bodies,
            Ok(_) => {}
            Err(error) => panic!("malformed XHTML ({error}): {xhtml}"),
        }
    }
}

/// `]]>` would end the CDATA section early and leave the rest of the code as
/// markup, so it must be split across two sections.
#[test]
fn render_keeps_a_cdata_terminator_in_code_well_formed() {
    let (fresh, preserved) = render_fresh_and_preserved("```\na]]>b\n```\n");

    assert_eq!(code_bodies(&fresh), ["a]]>b\n"]);
    assert_eq!(preserved.warnings, [] as [String; 0]);
    assert_eq!(preserved.xhtml, fresh);
}

#[test]
fn render_writes_an_empty_code_block_the_same_with_preservation() {
    let (fresh, preserved) = render_fresh_and_preserved("```\n```\n");

    assert_eq!(code_bodies(&fresh), [""]);
    assert_eq!(preserved.warnings, [] as [String; 0]);
    assert_eq!(preserved.xhtml, fresh);
}

/// Entity references in a code sample are code, not markup to resolve.
#[test]
fn render_keeps_entities_in_code_the_same_with_preservation() {
    let (fresh, preserved) = render_fresh_and_preserved("```html\n<p>&copy;&nbsp;2024</p>\n```\n");

    assert_eq!(code_bodies(&fresh), ["<p>&copy;&nbsp;2024</p>\n"]);
    assert_eq!(preserved.warnings, [] as [String; 0]);
    assert_eq!(preserved.xhtml, fresh);
}

// ── preserved output matches a fresh render ───────────────────────────────

#[test]
fn render_escapes_quotes_the_same_with_preservation() {
    let (fresh, preserved) =
        render_fresh_and_preserved("He said \"it's\" [here](https://example.com/?q='a\"b').\n");

    assert!(fresh.contains("&quot;it&#x27;s&quot;"), "{fresh}");
    assert_eq!(preserved.warnings, [] as [String; 0]);
    assert_eq!(preserved.xhtml, fresh);
}

/// Raw HTML passes through to the fresh render, so an entity the preserver
/// cannot resolve has to be written back as it came.
#[test]
fn render_keeps_an_unresolved_entity_in_raw_html_with_preservation() {
    let (fresh, preserved) = render_fresh_and_preserved("<div>I &hearts; docs</div>\n");

    assert!(fresh.contains("I &hearts; docs"), "{fresh}");
    assert_eq!(preserved.warnings, [] as [String; 0]);
    assert_eq!(preserved.xhtml, fresh);
}

/// Raw HTML passes through unchecked, and preserving would drop an attribute
/// that is not XML.
#[test]
fn render_skips_preservation_for_an_attribute_without_a_value() {
    let (fresh, preserved) = render_fresh_and_preserved("<input type=\"checkbox\" disabled />\n");

    assert!(fresh.contains("disabled"), "{fresh}");
    assert_eq!(
        preserved.warnings,
        [
            "comment preservation skipped: cannot parse rendered XHTML: \
             error while parsing attribute: position 31: \
             attribute key must be directly followed by `=` or space"
        ]
    );
    assert_eq!(preserved.xhtml, fresh);
}

/// The marker wraps only the text it matched; the spaces inside the old marker
/// stay in the paragraph, where the fresh render has them.
#[test]
fn render_places_a_marker_around_its_trimmed_text() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let opts = RenderOptions {
        current_xhtml: Some(
            r#"<p>a<ac:inline-comment-marker ac:ref="r"> word </ac:inline-comment-marker>b</p>"#
                .to_owned(),
        ),
        ..RenderOptions::default()
    };
    let output = render("a word b\n", tmp.path(), opts).expect("render");

    assert_eq!(
        output.xhtml,
        r#"<p>a <ac:inline-comment-marker ac:ref="r">word</ac:inline-comment-marker> b</p>"#
    );
}
