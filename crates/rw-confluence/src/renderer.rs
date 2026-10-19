//! Markdown to Confluence storage format, with diagrams written out as
//! attachments so the markup can reference them by filename.

use std::path::Path;
use std::sync::Arc;

use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramRequest, DiagramRouter, Providers, Resolutions,
    ResolveContext,
};
use rw_renderer::{MarkdownRenderer, RenderResult};

use crate::RenderOptions;
use crate::backend::ConfluenceBackend;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Render markdown to Confluence storage format, writing each diagram into
/// `output_dir` as an attachment along the way.
///
/// Returns the rendered result and the attachment filenames, sorted. The list
/// is exact — every name comes from a diagram this call resolved — so a stale
/// file already sitting in `output_dir` is never listed. This function does not
/// remove one; [`render`](fn@crate::render) wipes the directory's PNGs before
/// calling it, because the bundle is also read by whoever uploads it.
pub(crate) fn render_page(
    markdown: &str,
    providers: &Providers,
    output_dir: &Path,
    opts: &RenderOptions,
) -> (RenderResult, Vec<String>) {
    let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new();
    if opts.extract_title {
        renderer = renderer.with_title_extraction();
    }
    // With no provider configured there is no router either, so every
    // diagram fence stays an ordinary code block.
    if !providers.is_empty() {
        renderer =
            renderer.with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>);
    }

    let pass = renderer.begin(markdown);
    let requests: Vec<DiagramRequest> = pass.requests().iter().map(force_png).collect();
    let mut resolutions = providers.resolve(&requests, &ResolveContext::default());
    let attachments = write_attachments(&mut resolutions, &requests, output_dir);
    let mut result = pass.finish(&resolutions);
    // The pass leaves a failure to its error figure, which only a reader of the
    // published page would see. The label must match the one the pass puts on
    // that figure, so the operator can find it.
    for request in &requests {
        if let Some(Err(error)) = resolutions.get(&request.key) {
            let label = u32::from(request.key);
            result.warnings.push(format!("diagram {label}: {error}"));
        }
    }

    if opts.prepend_toc && !result.toc.is_empty() {
        result.html.insert_str(0, TOC_MACRO);
    }
    (result, attachments)
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
/// A write failure replaces the whole resolution with an error, so the fence
/// becomes an error figure and the rest of the page still publishes: losing one
/// diagram is not a reason to lose the page.
fn write_attachments(
    resolutions: &mut Resolutions,
    requests: &[DiagramRequest],
    dir: &Path,
) -> Vec<String> {
    // Walked in document order rather than over the map, whose iteration order
    // is arbitrary: the sort below then transforms a known order into another
    // known one, instead of hiding an arbitrary one.
    let mut attachments = Vec::new();
    for request in requests {
        let Some(resolution) = resolutions.get_mut(&request.key) else {
            continue;
        };
        let Ok(resolved) = resolution else { continue };
        let Asset::Inline(DiagramContent::Png(bytes)) = &resolved.asset else {
            continue;
        };

        // Twelve characters, or the whole digest when it is shorter: the length
        // is the provider's to choose (`Resolved::digest` promises only lowercase
        // hex), so slicing would panic on one that returns fewer. `KrokiProvider`
        // returns 64.
        let name = format!(
            "diagram_{}.png",
            resolved.digest.get(..12).unwrap_or(&resolved.digest)
        );
        if let Err(error) = std::fs::write(dir.join(&name), bytes) {
            *resolution = Err(DiagramError {
                message: format!("writing {name} failed: {error}"),
                transient: false,
            });
            continue;
        }
        resolved.asset = Asset::Reference(name.clone());
        attachments.push(name);
    }

    attachments.sort();
    // Two fences with identical source resolve to the same digest and so to one
    // file. Uploading that name twice is at best redundant and at worst an
    // error from Confluence.
    attachments.dedup();
    attachments
}

#[cfg(test)]
mod tests {
    use rw_diagrams::{RequestKey, Resolved};

    use super::*;

    fn request(key: u32, attrs: &[(&str, &str)]) -> DiagramRequest {
        DiagramRequest {
            key: RequestKey::from(key),
            language: "plantuml".to_owned(),
            source: "A -> B".to_owned(),
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
            force_png(&request(0, &[("format", "svg")])).attrs,
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
        let forced = force_png(&request(0, &[("zz", "1"), ("format", "bogus")]));
        assert_eq!(
            forced.attrs,
            [
                ("zz".to_owned(), "1".to_owned()),
                ("format".to_owned(), "bogus".to_owned()),
                ("format".to_owned(), "png".to_owned()),
            ],
        );
    }

    /// Only `attrs` changes: the key routes the resolution back to its fence,
    /// and the source and language are what the digest is computed over.
    #[test]
    fn forcing_png_leaves_the_rest_of_the_request_alone() {
        let original = request(7, &[("format", "svg")]);
        let forced = force_png(&original);
        assert_eq!(forced.key, original.key);
        assert_eq!(forced.language, original.language);
        assert_eq!(forced.source, original.source);
        assert_eq!(forced.id, original.id);
    }

    // ── writing attachments ───────────────────────────────────────────────

    #[test]
    fn an_attachment_is_named_from_the_first_twelve_digest_characters() {
        let dir = tempfile::tempdir().expect("tempdir");
        let requests = [request(0, &[])];
        let mut resolutions =
            Resolutions::from([(requests[0].key, Ok(resolved_png(&"a".repeat(64))))]);

        let attachments = write_attachments(&mut resolutions, &requests, dir.path());

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
        let requests = [request(0, &[])];
        let mut resolutions = Resolutions::from([(requests[0].key, Ok(resolved_png("abc123")))]);

        let attachments = write_attachments(&mut resolutions, &requests, dir.path());

        assert_eq!(attachments, ["diagram_abc123.png"]);
        assert!(dir.path().join("diagram_abc123.png").exists());
    }

    /// The backend can only emit `<ri:attachment>` from a reference, so a
    /// resolution left inline would publish a diagram-shaped hole.
    #[test]
    fn a_written_diagram_becomes_a_reference_to_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let requests = [request(0, &[])];
        let mut resolutions =
            Resolutions::from([(requests[0].key, Ok(resolved_png(&"b".repeat(64))))]);

        write_attachments(&mut resolutions, &requests, dir.path());

        assert_eq!(
            resolutions[&requests[0].key]
                .as_ref()
                .expect("resolved")
                .asset,
            Asset::Reference("diagram_bbbbbbbbbbbb.png".to_owned()),
        );
    }

    /// Two fences with the same source share a digest, so they share a file.
    /// The list must name that file once, or the publisher uploads the same
    /// name twice.
    #[test]
    fn two_fences_sharing_a_digest_yield_one_attachment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let requests = [request(0, &[]), request(1, &[])];
        let digest = "c".repeat(64);
        let mut resolutions = Resolutions::from([
            (requests[0].key, Ok(resolved_png(&digest))),
            (requests[1].key, Ok(resolved_png(&digest))),
        ]);

        let attachments = write_attachments(&mut resolutions, &requests, dir.path());

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

        let requests = [request(0, &[])];
        let mut resolutions =
            Resolutions::from([(requests[0].key, Ok(resolved_png(&"d".repeat(64))))]);

        let attachments = write_attachments(&mut resolutions, &requests, dir.path());

        assert!(attachments.is_empty(), "got: {attachments:?}");
        let error = resolutions[&requests[0].key]
            .as_ref()
            .expect_err("the write failed");
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
}
