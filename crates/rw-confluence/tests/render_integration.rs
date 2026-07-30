//! Integration tests for `rw_confluence::render`.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use rw_confluence::{RenderOptions, render};

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
}

/// Without a Kroki URL a diagram fence is an ordinary code block, exactly as
/// before — no request, no attachment, no error figure.
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
