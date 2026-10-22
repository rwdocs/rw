//! Markdown to Confluence storage format, with diagrams and local images
//! written out as attachments so the markup can reference them by filename.

use std::borrow::Cow;
use std::iter;
use std::path::Path;
use std::sync::Arc;

use quick_xml::escape::unescape;
use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramRequest, DiagramRouter, Providers, Resolutions,
    ResolveContext, Resolved,
};
use rw_renderer::{
    IncludeError, IncludeReader, MarkdownRenderer, ProjectFiles, RenderResult, image_file,
};
use rw_storage::file_content_type;

use crate::RenderOptions;
use crate::backend::ConfluenceBackend;
use crate::images::{ImageAttachments, ImageNames};

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Render markdown to Confluence storage format, writing each diagram and
/// each local image the page shows into `output_dir` as an attachment.
///
/// Returns the rendered result and the attachment filenames, sorted. The list
/// is exact — every name comes from a diagram or image this call wrote — so a
/// stale file already sitting in `output_dir` is never listed. This function
/// does not remove one; [`render`](fn@crate::render) wipes the directory's
/// attachments before calling it, because the bundle is also read by whoever
/// uploads it.
pub(crate) fn render_page(
    markdown: &str,
    providers: &Providers,
    output_dir: &Path,
    opts: &RenderOptions,
) -> (RenderResult, Vec<String>) {
    let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new()
        .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>);
    if opts.extract_title {
        renderer = renderer.with_title_dropped();
    }

    let mut images = None;
    if let Some(local) = &opts.images {
        renderer = renderer
            .with_source_path(local.source_path.clone())
            .with_file_resolver(local.files.clone());
        if let Some(includes) = &local.includes {
            renderer = renderer.with_include_reader(SharedIncludes(Arc::clone(includes)));
        }
        if let Some(storage) = &local.storage {
            let attachments = Arc::new(ImageAttachments::new(Arc::clone(storage)));
            renderer = renderer.with_image_src(ImageNames(Arc::clone(&attachments)));
            images = Some(attachments);
        }
    }

    let pass = renderer.begin(markdown);
    let requests: Vec<DiagramRequest> = pass.requests().iter().map(force_png).collect();
    let ctx = ResolveContext {
        model: opts.model.as_deref(),
        record_read: None,
    };
    let mut resolutions = providers.resolve(&requests, &ctx);
    let written = write_attachments(&mut resolutions, output_dir);
    let mut result = pass.finish(&resolutions);
    // First, so a provider's warnings precede the failure the pass reports.
    result.warnings.splice(0..0, written.warnings);
    let mut attachments = written.attachments;
    for name in attribute_values(&result.html, r#"<ri:attachment ri:filename=""#) {
        if attachments.contains(&name) {
            continue;
        }
        let Some((path, content)) = images.as_ref().and_then(|images| images.take(&name)) else {
            result.warnings.push(format!(
                "attachment '{name}' is not in the bundle; upload it to the page yourself"
            ));
            continue;
        };
        match std::fs::write(output_dir.join(&name), content) {
            Ok(()) => attachments.push(name),
            Err(error) => result
                .warnings
                .push(format!("image `{path}`: writing {name} failed: {error}")),
        }
    }
    attachments.sort();
    if let Some(local) = &opts.images {
        let linkable = ProjectFiles::new(|path| file_content_type(path).is_some());
        // The markup no longer says which file a link came from, so each is
        // placed from every folder it could have been written in.
        let sources: Vec<&str> = iter::once(local.source_path.as_str())
            .chain(result.includes.iter().map(String::as_str))
            .collect();
        for href in attribute_values(&result.html, r#"<a href=""#) {
            if sources
                .iter()
                .any(|source| matches!(image_file(&linkable, Some(source), &href), Some(Ok(_))))
            {
                result
                    .warnings
                    .push(format!("link `{href}` is not attached to Confluence pages"));
            }
        }
    }

    // The title H1 is no ToC entry, but kept on the page it is a heading.
    let kept_title = !opts.extract_title && result.title.is_some();
    if opts.prepend_toc && (kept_title || !result.toc.is_empty()) {
        result.html.insert_str(0, TOC_MACRO);
    }
    (result, attachments)
}

/// The options' reader, for a renderer that owns the reader it is given.
struct SharedIncludes(Arc<dyn IncludeReader>);

impl IncludeReader for SharedIncludes {
    fn read(&self, path: &str) -> Result<String, IncludeError> {
        self.0.read(path)
    }
}

/// The value of each attribute that ends `prefix` in `xhtml`, unescaped,
/// once, in page order.
fn attribute_values(xhtml: &str, prefix: &str) -> Vec<String> {
    // A code block is CDATA, and the markup it shows is not a reference. A
    // body containing `]]>` is split into consecutive sections, which this
    // skips as one.
    let markup = xhtml.split("<![CDATA[").enumerate().map(|(i, part)| {
        if i == 0 {
            part
        } else {
            part.split_once("]]>").map_or("", |(_, after)| after)
        }
    });
    let mut values = Vec::new();
    for value in markup.flat_map(|part| part.split(prefix).skip(1)) {
        let Some(end) = value.find('"') else { continue };
        let value =
            unescape(&value[..end]).map_or_else(|_| value[..end].to_owned(), Cow::into_owned);
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

/// The same request, rendered as PNG whatever the fence asked for.
///
/// PNG is the only format this crate can publish: `write_attachments` writes
/// only PNG bytes, and the backend can emit `<ri:attachment>` only from a
/// written reference — a diagram left in any other format becomes an error
/// figure. The digest is computed over the format, so the filename depends on
/// this too.
///
/// Appended rather than replacing what the author wrote, so their attributes
/// still reach the provider and still get diagnosed. A provider reads the last
/// `format` it sees, so appending wins the format while leaving a typo'd
/// `format=bogus`, or an unknown key, to warn — replacing the list would
/// publish those mistakes in silence, including under `--strict`.
fn force_png(request: &DiagramRequest) -> DiagramRequest {
    let mut attrs = request.attrs.clone();
    attrs.push(("format".to_owned(), "png".to_owned()));
    DiagramRequest {
        attrs,
        ..request.clone()
    }
}

/// Write each resolved diagram into `output_dir` and point its resolution at
/// the file, returning the names written.
///
/// The name is `diagram_<digest[..12]>.png`. That digest is published output —
/// Confluence keys attachments by filename, so changing how it is derived
/// re-uploads every diagram on every page — and it comes from the provider,
/// which computes it over the *prepared* source and the output format.
///
/// A resolution with no PNG bytes, or whose write fails, becomes an error, so
/// the fence becomes an error figure and the rest of the page still publishes:
/// losing one diagram is not a reason to lose the page.
fn write_attachments(resolutions: &mut Resolutions, dir: &Path) -> Written {
    let mut written = Written::default();
    for (number, resolution) in (1..).zip(resolutions.iter_mut()) {
        let Ok(resolved) = resolution else { continue };
        match write_attachment(resolved, dir) {
            Ok(name) => written.attachments.push(name),
            Err(message) => {
                written.warnings.extend(
                    resolved
                        .warnings
                        .drain(..)
                        .map(|warning| format!("diagram {number}: {warning}")),
                );
                *resolution = Err(DiagramError {
                    message,
                    transient: false,
                });
            }
        }
    }

    written.attachments.sort();
    // Two fences with identical source resolve to the same digest and so to one
    // file. Uploading that name twice is at best redundant and at worst an
    // error from Confluence.
    written.attachments.dedup();
    written
}

fn write_attachment(resolved: &mut Resolved, dir: &Path) -> Result<String, String> {
    let Asset::Inline(DiagramContent::Png(bytes)) = &resolved.asset else {
        return Err("the provider returned no PNG bytes to attach".to_owned());
    };

    // Twelve characters, or the whole digest when it is shorter: the length
    // is the provider's to choose (`Resolved::digest` promises only lowercase
    // hex), so slicing would panic on one that returns fewer. `KrokiProvider`
    // returns 64.
    let name = format!(
        "diagram_{}.png",
        resolved.digest.get(..12).unwrap_or(&resolved.digest)
    );
    std::fs::write(dir.join(&name), bytes)
        .map_err(|error| format!("writing {name} failed: {error}"))?;
    resolved.asset = Asset::Reference(name.clone());
    Ok(name)
}

#[derive(Default)]
struct Written {
    /// The filenames written, sorted and deduplicated.
    attachments: Vec<String>,
    /// The provider warnings of each resolution turned into an error, which
    /// the pass reports only for a success.
    warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use rw_diagrams::{DiagramProvider, DiagramSource};
    use rw_renderer::ProjectFiles;
    use rw_storage::MockStorage;

    use super::*;
    use crate::LocalImages;

    fn request(attrs: &[(&str, &str)]) -> DiagramRequest {
        DiagramRequest {
            language: "plantuml".to_owned(),
            source: DiagramSource::Inline("A -> B".to_owned()),
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            id: None,
        }
    }

    fn resolved_png(digest: &str) -> Resolved {
        Resolved {
            asset: Asset::Inline(DiagramContent::Png(Vec::from([0x89, b'P', b'N', b'G']))),
            size: None,
            digest: digest.to_owned(),
            warnings: Vec::new(),
        }
    }

    // ── forcing PNG ───────────────────────────────────────────────────────

    /// A fence that asked for SVG is still published as PNG, because the
    /// appended pair is the last `format` a provider reads.
    #[test]
    fn forcing_png_overrides_the_format_the_author_asked_for() {
        assert_eq!(
            force_png(&request(&[("format", "svg")])).attrs,
            [
                ("format".to_owned(), "svg".to_owned()),
                ("format".to_owned(), "png".to_owned()),
            ],
        );
    }

    /// The author's attributes survive the override, so their mistakes still
    /// reach the provider that diagnoses them. Replacing the list instead would
    /// publish a typo'd `format` or an unknown key in silence — and pass
    /// `--strict`, which exists to catch exactly that.
    #[test]
    fn forcing_png_keeps_the_authors_attributes_so_they_are_still_diagnosed() {
        let forced = force_png(&request(&[("zz", "1"), ("format", "bogus")]));
        assert_eq!(
            forced.attrs,
            [
                ("zz".to_owned(), "1".to_owned()),
                ("format".to_owned(), "bogus".to_owned()),
                ("format".to_owned(), "png".to_owned()),
            ],
        );
    }

    /// Only `attrs` changes: the source and language are what the digest is
    /// computed over.
    #[test]
    fn forcing_png_leaves_the_rest_of_the_request_alone() {
        let original = request(&[("format", "svg")]);
        let forced = force_png(&original);
        assert_eq!(forced.language, original.language);
        assert_eq!(forced.source, original.source);
        assert_eq!(forced.id, original.id);
    }

    // ── writing attachments ───────────────────────────────────────────────

    #[test]
    fn an_attachment_is_named_from_the_first_twelve_digest_characters() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut resolutions = Vec::from([Ok(resolved_png(&"a".repeat(64)))]);

        let attachments = write_attachments(&mut resolutions, dir.path()).attachments;

        assert_eq!(attachments, ["diagram_aaaaaaaaaaaa.png"]);
        assert!(dir.path().join("diagram_aaaaaaaaaaaa.png").exists());
    }

    /// A digest shorter than twelve characters is used whole rather than
    /// sliced. `Resolved::digest` leaves the length to the provider — its own
    /// doc example is six characters — so a slice would panic on a provider
    /// that is within its rights.
    #[test]
    fn a_digest_shorter_than_twelve_characters_is_used_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut resolutions = Vec::from([Ok(resolved_png("abc123"))]);

        let attachments = write_attachments(&mut resolutions, dir.path()).attachments;

        assert_eq!(attachments, ["diagram_abc123.png"]);
        assert!(dir.path().join("diagram_abc123.png").exists());
    }

    /// The backend can only emit `<ri:attachment>` from a reference, so a
    /// resolution left inline would publish a diagram-shaped hole.
    #[test]
    fn a_written_diagram_becomes_a_reference_to_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut resolutions = Vec::from([Ok(resolved_png(&"b".repeat(64)))]);

        write_attachments(&mut resolutions, dir.path());

        assert_eq!(
            resolutions[0].as_ref().expect("resolved").asset,
            Asset::Reference("diagram_bbbbbbbbbbbb.png".to_owned()),
        );
    }

    /// Two fences with the same source share a digest, so they share a file.
    /// The list must name that file once, or the publisher uploads the same
    /// name twice.
    #[test]
    fn two_fences_sharing_a_digest_yield_one_attachment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let digest = "c".repeat(64);
        let mut resolutions = Vec::from([Ok(resolved_png(&digest)), Ok(resolved_png(&digest))]);

        let attachments = write_attachments(&mut resolutions, dir.path()).attachments;

        assert_eq!(attachments, ["diagram_cccccccccccc.png"]);
    }

    /// A directory that cannot be written to fails the diagram, not the page:
    /// the entry becomes an error the pass turns into an error figure, and
    /// everything else still publishes.
    #[test]
    fn a_write_failure_replaces_the_resolution_with_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A directory where the PNG wants to be: the write cannot succeed, on
        // any platform, without depending on file permissions.
        std::fs::create_dir(dir.path().join("diagram_dddddddddddd.png")).expect("blocking dir");

        let mut resolutions = Vec::from([Ok(resolved_png(&"d".repeat(64)))]);

        let attachments = write_attachments(&mut resolutions, dir.path()).attachments;

        assert!(attachments.is_empty(), "got: {attachments:?}");
        let error = resolutions[0].as_ref().expect_err("the write failed");
        assert!(
            error
                .message
                .starts_with("writing diagram_dddddddddddd.png failed: "),
            "the message should name the file: {}",
            error.message,
        );
        assert!(
            !error.transient,
            "the same directory fails the same way every time",
        );
    }

    /// The error replaces the diagram, not what the provider said about its
    /// source: an unknown attribute is still a mistake to fix.
    #[test]
    fn a_write_failure_keeps_the_providers_warnings() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("diagram_gggggggggggg.png")).expect("blocking dir");
        let mut resolutions = Vec::from([
            Ok(resolved_png(&"f".repeat(64))),
            Ok(Resolved {
                warnings: vec!["unknown attribute 'zz'".to_owned()],
                ..resolved_png(&"g".repeat(64))
            }),
        ]);

        let written = write_attachments(&mut resolutions, dir.path());

        assert_eq!(written.warnings, ["diagram 2: unknown attribute 'zz'"]);
    }

    /// The backend can reference only a written PNG, so anything else must
    /// become an error, which is what `render_page` warns about.
    #[test]
    fn a_resolution_without_png_bytes_becomes_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svg = Resolved {
            asset: Asset::Inline(DiagramContent::Svg("<svg/>".to_owned())),
            ..resolved_png("e")
        };
        let mut resolutions = Vec::from([Ok(svg)]);

        let attachments = write_attachments(&mut resolutions, dir.path()).attachments;

        assert!(attachments.is_empty(), "got: {attachments:?}");
        let error = resolutions[0].as_ref().expect_err("nothing was written");
        assert_eq!(
            error.message,
            "the provider returned no PNG bytes to attach"
        );
    }

    // ── the page ──────────────────────────────────────────────────────────

    fn render(markdown: &str) -> RenderResult {
        let dir = tempfile::tempdir().expect("tempdir");
        render_page(
            markdown,
            &Providers::empty(),
            dir.path(),
            &RenderOptions::default(),
        )
        .0
    }

    /// A code block showing storage markup is text, not a reference.
    #[test]
    fn an_attachment_shown_in_a_code_block_is_not_warned_about() {
        let result = render(concat!(
            "```xml\n",
            "<ac:image><ri:attachment ri:filename=\"diagram.png\" /></ac:image>\n",
            "]]>\n",
            "<ri:attachment ri:filename=\"after-split.png\" />\n",
            "```\n",
        ));
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    /// Answers every request with an SVG and a warning, so the forced PNG is
    /// never honoured.
    struct SvgOnly;

    impl DiagramProvider for SvgOnly {
        fn handles(&self, language: &str) -> bool {
            language == "plantuml"
        }

        fn resolve(
            &self,
            requests: &[&DiagramRequest],
            _ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            requests
                .iter()
                .map(|_| {
                    Ok(Resolved {
                        asset: Asset::Inline(DiagramContent::Svg("<svg/>".to_owned())),
                        warnings: vec!["unknown attribute 'zz'".to_owned()],
                        ..resolved_png("a")
                    })
                })
                .collect()
        }
    }

    #[test]
    fn a_diagram_that_cannot_be_attached_still_reports_its_provider_warnings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (result, attachments) = render_page(
            "```plantuml {zz=1}\nA -> B\n```\n",
            &Providers::empty().with(Arc::new(SvgOnly)),
            dir.path(),
            &RenderOptions::default(),
        );
        assert!(attachments.is_empty(), "got: {attachments:?}");
        assert_eq!(
            result.warnings,
            [
                "diagram 1: unknown attribute 'zz'",
                "diagram 1: the provider returned no PNG bytes to attach",
            ],
        );
    }

    /// Answers the one diagram file the page names with PNG bytes.
    struct PngFiles;

    impl DiagramProvider for PngFiles {
        fn handles(&self, language: &str) -> bool {
            language == "plantuml"
        }

        fn resolve(
            &self,
            requests: &[&DiagramRequest],
            _ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            requests
                .iter()
                .map(|request| {
                    assert_eq!(
                        request.source,
                        DiagramSource::File("docs/flows/pay.puml".to_owned())
                    );
                    Ok(resolved_png("0123456789abcdef"))
                })
                .collect()
        }
    }

    #[test]
    fn a_lone_diagram_file_image_becomes_an_attached_diagram() {
        let dir = tempfile::tempdir().expect("tempdir");
        let opts = RenderOptions {
            images: Some(LocalImages {
                files: ProjectFiles::new(|_| true),
                source_path: "docs/guide.md".to_owned(),
                storage: Some(Arc::new(MockStorage::new())),
                project_dir: PathBuf::new(),
                includes: None,
            }),
            ..RenderOptions::default()
        };
        let (result, attachments) = render_page(
            "![Payment flow](flows/pay.puml)\n",
            &Providers::empty().with(Arc::new(PngFiles)),
            dir.path(),
            &opts,
        );
        assert_eq!(
            result.html,
            r#"<ac:image ac:alt="Payment flow"><ri:attachment ri:filename="diagram_0123456789ab.png" /></ac:image>"#
        );
        assert_eq!(attachments, ["diagram_0123456789ab.png"]);
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    /// Without [`LocalImages`](crate::LocalImages) the bundle carries no
    /// local image, so the page references an attachment it does not have.
    #[test]
    fn each_local_image_warns_once_that_it_is_not_bundled() {
        let result = render(concat!(
            "![a](images/a%20b.png) ![again](other/a%20b.png)\n",
            "![amp](x&amp;y.png) ![remote](https://example.com/r.png)\n",
        ));
        assert_eq!(
            result.warnings,
            [
                "attachment 'a b.png' is not in the bundle; upload it to the page yourself",
                "attachment 'x&y.png' is not in the bundle; upload it to the page yourself",
            ],
        );
    }

    /// Confluence rejects a storage body that is not XML.
    #[test]
    fn raw_html_void_elements_render_as_well_formed_xml() {
        let html = render(concat!(
            "Line one<br>line two<BR/>three<wbr>four\n\n",
            "<hr>\n\n",
            "<img src=\"a.png\" alt=\"x>y\">\n\n",
            "Inline <img src=\"b.png\"\ntitle=\"t\"> split.\n\n",
            "![local](images/a%20b.png) ![data](data:image/png;base64,iVBORw0KGgo=)\n",
        ))
        .html;

        let wrapped = format!(r#"<root xmlns:ac="urn:ac" xmlns:ri="urn:ri">{html}</root>"#);
        let mut reader = Reader::from_str(&wrapped);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("{error}: {html}"),
            }
        }
    }
}
