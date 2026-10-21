//! Integration tests for `rw_confluence::render`.

use std::error::Error;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use quick_xml::Reader;
use quick_xml::events::Event;
use rw_confluence::{LocalImages, ProjectFiles, RenderOptions, RenderOutput, render};
use rw_error::error_chain;
use rw_kroki::test_support::{KrokiStub, PNG_HEADER, Reply, Request};
use rw_storage::{MockStorage, file_content_type};

#[test]
fn declared_metadata_name_does_not_change_confluence_title() {
    for metadata in ["kind: system", "kind: system\nname: payments-api"] {
        let tmp = tempfile::tempdir().unwrap();
        let markdown =
            format!("---\n{metadata}\ntitle: Metadata title\n---\n# Confluence heading\n\nBody.\n");
        let output = render(
            &markdown,
            tmp.path(),
            &RenderOptions {
                extract_title: true,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        // With title extraction enabled, the opening H1 supplies the export title;
        // metadata declarations must not override it.
        assert_eq!(output.title.as_deref(), Some("Confluence heading"));
    }
}

fn render_with_title(markdown: &str) -> RenderOutput {
    let tmp = tempfile::tempdir().expect("tempdir");
    let opts = RenderOptions {
        extract_title: true,
        ..RenderOptions::default()
    };
    render(markdown, tmp.path(), &opts).expect("render")
}

#[test]
fn render_takes_the_title_from_a_leading_h1_and_shifts_the_rest() {
    let output = render_with_title("---\nkind: guide\n---\n# Title\n\n## A\n\n### B\n\n# C\n");

    assert_eq!(output.title.as_deref(), Some("Title"));
    assert_eq!(
        output.xhtml,
        r#"<h1 id="a">A</h1><h2 id="b">B</h2><h1 id="c">C</h1>"#
    );
}

/// The title is the page's opening heading; an H1 further down, or nested in
/// a quote or tab, is an ordinary heading, and nothing shifts.
#[test]
fn render_without_a_leading_h1_has_no_title_and_keeps_levels() {
    for (markdown, expected) in [
        (
            "Intro\n\n## Before\n\n# Real Title\n\n## After\n\n# Second H1\n",
            r#"<p>Intro</p><h2 id="before">Before</h2><h1 id="real-title">Real Title</h1><h2 id="after">After</h2><h1 id="second-h1">Second H1</h1>"#,
        ),
        (
            "## Before\n\n# Title\n\n## After\n",
            r#"<h2 id="before">Before</h2><h1 id="title">Title</h1><h2 id="after">After</h2>"#,
        ),
        (
            "> # Quoted\n\n## After\n",
            r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body><h1 id="quoted">Quoted</h1></ac:rich-text-body></ac:structured-macro><h2 id="after">After</h2>"#,
        ),
        (
            "::::tabs\n\n:::tab[A]\n\n# In tab\n\n:::\n\n::::\n\n## After\n",
            r#"<p><strong>A</strong></p><h1 id="in-tab">In tab</h1><h2 id="after">After</h2>"#,
        ),
    ] {
        let output = render_with_title(markdown);

        assert_eq!(output.title, None, "{markdown:?}");
        assert_eq!(output.xhtml, expected, "{markdown:?}");
    }
}

/// Comments above the H1, such as `<!-- markdownlint-disable MD041 -->`,
/// don't stop it being the title.
#[test]
fn render_takes_the_title_from_an_h1_below_only_comments() {
    for markdown in [
        "<!-- x -->\n# T\n\n## A\n",
        "<!-- a -->\n\n<!-- b -->\n# T\n\n## A\n",
        "<!--\nline one\nline two\n-->\n# T\n\n## A\n",
        "---\nkind: guide\n---\n<!-- x -->\n# T\n\n## A\n",
    ] {
        let output = render_with_title(markdown);

        assert_eq!(output.title.as_deref(), Some("T"), "{markdown:?}");
        assert!(
            output.xhtml.ends_with(r#"<h1 id="a">A</h1>"#) && !output.xhtml.contains(r#"id="t""#),
            "{markdown:?}: {}",
            output.xhtml
        );
    }
}

#[test]
fn render_does_not_take_the_title_from_an_h1_below_other_html() {
    let output = render_with_title("<!-- x --><div></div>\n# T\n\n## A\n");

    assert_eq!(output.title, None);
    assert!(
        output
            .xhtml
            .ends_with(r#"<h1 id="t">T</h1><h2 id="a">A</h2>"#),
        "{}",
        output.xhtml
    );
}

#[test]
fn render_does_not_take_a_blank_h1_as_the_title() {
    for markdown in ["# \n\n## A\n", "# &nbsp;\n\n## A\n"] {
        let output = render_with_title(markdown);

        assert_eq!(output.title, None, "{markdown:?}");
        assert!(
            output.xhtml.ends_with(r#"<h2 id="a">A</h2>"#),
            "{markdown:?}: {}",
            output.xhtml
        );
    }
}

#[test]
fn render_writes_page_xhtml_for_plain_markdown() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    let markdown = "# Hello\n\nA paragraph.\n";
    let output = render(markdown, out, &RenderOptions::default()).expect("render succeeded");

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
        render("# Hi\n", &nested, &RenderOptions::default()).expect("render created the dir");

    assert!(nested.join("page.xhtml").exists());
    assert_ne!(output.xhtml, "");
}

#[test]
fn render_error_names_the_directory_it_could_not_create() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("file");
    std::fs::write(&file, "").expect("write blocking file");
    let out_dir = file.join("dist");

    let error = render("# Hi\n", &out_dir, &RenderOptions::default()).expect_err("render failed");

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

    let error = render("# Hi\n", tmp.path(), &RenderOptions::default()).expect_err("render failed");

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
    let output = render(markdown, out, &opts).expect("render");

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
    let output = render(markdown, out, &opts).expect("render");

    assert_eq!(output.unmatched_comments.len(), 1);
    assert_eq!(output.unmatched_comments[0].ref_id, "xyz");
    assert_eq!(output.unmatched_comments[0].text, "Original sentence here");
}

/// The bundle directory is the publish surface: `docs/confluence.md` has the
/// publisher upload every file in it, so an attachment left there by an
/// earlier render is uploaded as a phantom. Re-rendering into a reused
/// directory has to clear those out, and leave alone every file that is not
/// one.
///
/// The attachment list itself cannot name a file it did not write, because
/// nothing reads the directory to build it. So the risk this guards is the
/// publisher's upload, not rw's bookkeeping.
#[test]
fn render_drops_stale_attachments_from_previous_run() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path();

    for stale in ["diagram_aaaaaaaaaaaa.png", "image_bbbbbbbbbbbb.svg"] {
        std::fs::write(out.join(stale), b"stale").expect("write stale");
    }
    for kept in [
        "notes.txt",
        "logo.png",
        "diagram-old.png",
        "diagram_x.svg",
        "image_notes.txt",
        "image_bbbbbbbbbbbb.txt",
        "image_bbbbbbbbbbb.svg",
    ] {
        std::fs::write(out.join(kept), b"keep me").expect("write kept");
    }
    // Directories named like attachments are not leftovers.
    std::fs::create_dir(out.join("diagram_cache.png")).expect("mkdir");
    std::fs::create_dir(out.join("image_cache")).expect("mkdir");

    let output = render("# Hi\n", out, &RenderOptions::default()).expect("render");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    let mut left: Vec<String> = std::fs::read_dir(out)
        .expect("read out")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("utf8")
        })
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "diagram-old.png",
            "diagram_cache.png",
            "diagram_x.svg",
            "image_bbbbbbbbbbb.svg",
            "image_bbbbbbbbbbbb.txt",
            "image_cache",
            "image_notes.txt",
            "logo.png",
            "notes.txt",
            "page.xhtml",
        ],
    );
}

// ── local images ──────────────────────────────────────────────────────────

/// Renders `markdown` as the project file `docs/guide.md` into a fresh
/// directory, reading images from `storage`.
fn render_images(
    markdown: &str,
    storage: MockStorage,
    extract_title: bool,
) -> (tempfile::TempDir, RenderOutput) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let opts = RenderOptions {
        extract_title,
        ..image_options(storage)
    };
    let output = render(markdown, tmp.path(), &opts).expect("render");
    (tmp, output)
}

/// Options that read images from `storage` for the project file
/// `docs/guide.md`.
fn image_options(storage: MockStorage) -> RenderOptions {
    RenderOptions {
        images: Some(LocalImages {
            files: ProjectFiles::new("", |path| file_content_type(path).is_some())
                .expect("the project root"),
            source_path: "docs/guide.md".to_owned(),
            storage: Some(Arc::new(storage)),
            project_dir: PathBuf::new(),
        }),
        ..RenderOptions::default()
    }
}

fn attachment(name: &str) -> String {
    format!(r#"<ac:image><ri:attachment ri:filename="{name}" /></ac:image>"#)
}

/// Spelled out rather than recomputed, like the diagram names: Confluence
/// keys attachments by name, so a change to how they are derived re-uploads
/// every image.
const FLOW: &str = "image_3b21278178ed.png";
const LOGO_A: &str = "image_0aafb3298f8f.svg";
const LOGO_B: &str = "image_ee4395eb3a8e.svg";

#[test]
fn render_bundles_a_local_image_as_an_attachment_named_after_its_content() {
    let (tmp, output) = render_images(
        "![](img/flow.png)\n",
        MockStorage::new().with_project_file("docs/img/flow.png", "flow"),
        false,
    );

    assert_eq!(output.attachments, [FLOW]);
    assert_eq!(std::fs::read(tmp.path().join(FLOW)).expect("read"), b"flow");
    assert_eq!(output.xhtml, format!("<p>{}</p>", attachment(FLOW)));
    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);
}

#[test]
fn render_bundles_one_attachment_for_the_same_content() {
    let (tmp, output) = render_images(
        "![](img/flow.png) ![](./img/flow.png?v=2) ![](/copy.png)\n",
        MockStorage::new()
            .with_project_file("docs/img/flow.png", "flow")
            .with_project_file("copy.png", "flow"),
        false,
    );

    assert_eq!(output.attachments, [FLOW]);
    assert_eq!(output.xhtml.matches(&attachment(FLOW)).count(), 3);
    assert_eq!(std::fs::read_dir(tmp.path()).expect("read").count(), 2);
}

#[test]
fn render_keeps_same_named_files_in_different_folders_apart() {
    let (tmp, output) = render_images(
        "![](a/logo.svg) ![](b/logo.svg)\n",
        MockStorage::new()
            .with_project_file("docs/a/logo.svg", "<svg>a</svg>")
            .with_project_file("docs/b/logo.svg", "<svg>b</svg>"),
        false,
    );

    assert_eq!(output.attachments, [LOGO_A, LOGO_B]);
    assert_eq!(
        output.xhtml,
        format!("<p>{} {}</p>", attachment(LOGO_A), attachment(LOGO_B))
    );
    for (name, content) in [(LOGO_A, "<svg>a</svg>"), (LOGO_B, "<svg>b</svg>")] {
        assert_eq!(
            std::fs::read(tmp.path().join(name)).expect("read"),
            content.as_bytes()
        );
    }
}

/// An image the bundle can't carry still publishes as an attachment named
/// after its file, for the publisher to upload.
#[test]
fn render_leaves_images_it_cannot_bundle_to_the_publisher() {
    let (_tmp, output) = render_images(
        "![](img/missing.png) ![](../../outside.png) ![](img/a.bmp)\n",
        MockStorage::new().with_project_file("docs/img/a.bmp", "bmp"),
        false,
    );

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert_eq!(
        output.xhtml,
        format!(
            "<p>{} {} {}</p>",
            attachment("missing.png"),
            attachment("outside.png"),
            attachment("a.bmp")
        )
    );
    assert_eq!(
        output.warnings,
        [
            "image `../../outside.png` is outside the project",
            "image `img/a.bmp` is not a supported image type",
            "attachment 'missing.png' is not in the bundle; upload it to the page yourself",
            "attachment 'outside.png' is not in the bundle; upload it to the page yourself",
            "attachment 'a.bmp' is not in the bundle; upload it to the page yourself",
        ]
    );
}

/// With title extraction the opening H1 is not on the page, so neither is an
/// image in it, and the bundle must not carry an attachment nothing shows.
#[test]
fn render_does_not_bundle_an_image_in_the_title() {
    let (tmp, output) = render_images(
        "# ![Logo](a/logo.svg) Guide\n\n![Flow](img/flow.png)\n",
        MockStorage::new()
            .with_project_file("docs/a/logo.svg", "<svg>a</svg>")
            .with_project_file("docs/img/flow.png", "flow"),
        true,
    );

    assert_eq!(output.attachments, [FLOW]);
    assert_eq!(std::fs::read(tmp.path().join(FLOW)).expect("read"), b"flow");
    assert!(!tmp.path().join(LOGO_A).exists());
    assert!(!output.xhtml.contains(LOGO_A), "{}", output.xhtml);
}

/// The renderer places no file for a source with a scheme, so neither does
/// the page.
#[test]
fn render_links_an_image_with_a_scheme_as_a_url() {
    let (_tmp, output) = render_images("![](ftp://example.com/a.png)\n", MockStorage::new(), false);

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert_eq!(
        output.xhtml,
        r#"<p><ac:image><ri:url ri:value="ftp://example.com/a.png" /></ac:image></p>"#
    );
    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);
}

/// Like a diagram, an image that cannot be written fails alone, and the
/// warning names the file to upload by hand.
#[test]
fn render_survives_an_image_it_cannot_write() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(tmp.path().join(FLOW)).expect("blocking dir");

    let output = render(
        "![Flow](img/flow.png)\n",
        tmp.path(),
        &image_options(MockStorage::new().with_project_file("docs/img/flow.png", "flow")),
    )
    .expect("render");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert_eq!(output.warnings.len(), 1, "got: {:?}", output.warnings);
    assert!(
        output.warnings[0].starts_with(&format!(
            "image `docs/img/flow.png`: writing {FLOW} failed: "
        )),
        "got: {:?}",
        output.warnings
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
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });
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
    let output = render(markdown, out, &opts).expect("render succeeded");

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
            PNG_HEADER,
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
    for Request { path, body, .. } in &requests {
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

/// The real provider warns about every attribute it does not know, so this is
/// where an `alt` leaking into the request would show.
#[test]
fn render_writes_alt_text_without_a_provider_warning() {
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let opts = RenderOptions {
        kroki_url: Some(kroki.url.clone()),
        ..RenderOptions::default()
    };
    let output = render(
        "```plantuml {alt=\"Payment & refund flow\"}\n@startuml\nA -> B\n@enduml\n```\n",
        tmp.path(),
        &opts,
    )
    .expect("render succeeded");

    assert!(
        output
            .xhtml
            .contains(r#"ac:alt="Payment &amp; refund flow"><ri:attachment"#),
        "{}",
        output.xhtml,
    );
    assert!(output.warnings.is_empty(), "got: {:?}", output.warnings);
}

/// A diagram that cannot be written out fails alone: it becomes an error figure
/// and the page still publishes, with the other diagram's attachment intact.
#[test]
fn render_survives_a_diagram_it_cannot_write() {
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });
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
    let output = render(markdown, out, &opts).expect("the page still renders");

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

    // The code blocks between the diagrams are what make a code-block index
    // differ from the diagram's position.
    let markdown = concat!(
        "    indented\n\n",
        "```plantuml\n@startuml\nGrace -> Heidi\n@enduml\n```\n\n",
        "```rust\nlet x = 1;\n```\n\n",
        "```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n",
    );
    let opts = RenderOptions {
        kroki_url: Some(url),
        ..RenderOptions::default()
    };
    let output = render(markdown, tmp.path(), &opts).expect("the page still renders");

    assert!(
        output.attachments.is_empty(),
        "got: {:?}",
        output.attachments
    );
    assert_warned_once_per_error_figure(&output, 2);
    assert!(
        output.warnings[0].starts_with("diagram 1: ")
            && output.warnings[1].starts_with("diagram 2: "),
        "warnings must number diagrams from 1 in page order: {:?}",
        output.warnings,
    );
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
        &RenderOptions::default(),
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
    let output = render("# Hi\n", out, &opts).expect("render");

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
    let output = render(markdown, out, &RenderOptions::default()).expect("render succeeded");

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
    let output = render(markdown, out, &RenderOptions::default()).expect("render succeeded");

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
    for (markdown, expected) in [
        ("## Heading\n\nBody.\n", true),
        ("# Kept title\n\nBody.\n", true),
        ("# ![logo](logo.png)\n\nBody.\n", false),
        ("Body only.\n", false),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let opts = RenderOptions {
            prepend_toc: true,
            ..RenderOptions::default()
        };
        let output = render(markdown, tmp.path(), &opts).expect("render");

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
    let output = render(markdown, tmp.path(), &RenderOptions::default()).expect("render");

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
    let fresh = render(markdown, tmp.path(), &RenderOptions::default()).expect("fresh render");
    let opts = RenderOptions {
        current_xhtml: Some("<p>old</p>".to_owned()),
        ..RenderOptions::default()
    };
    let preserved = render(markdown, tmp.path(), &opts).expect("preserving render");
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
    let output = render("a word b\n", tmp.path(), &opts).expect("render");

    assert_eq!(
        output.xhtml,
        r#"<p>a <ac:inline-comment-marker ac:ref="r">word</ac:inline-comment-marker> b</p>"#
    );
}
