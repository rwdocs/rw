//! Markdown to Confluence page renderer.
//!
//! This module provides [`PageRenderer`] for converting `CommonMark` documents
//! to Confluence XHTML storage format.
//!
//! # Features
//!
//! - GitHub Flavored Markdown support (tables, strikethrough, task lists)
//! - Title extraction from first H1 heading
//! - Table of contents macro prepending
//! - Diagrams resolved through the caller's providers and written out as
//!   attachments, so the markup can reference them by filename
//!
//! # Usage
//!
//! Create a `PageRenderer` with builder methods (`prepend_toc`, `extract_title`),
//! then call `render(markdown, providers, output_dir)` to produce Confluence
//! XHTML plus the names of the attachments it wrote.

use std::path::Path;
use std::sync::Arc;

use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramRequest, DiagramRouter, Providers, Resolutions,
    ResolveContext,
};
use rw_renderer::{MarkdownRenderer, RenderResult, TocEntry};

use crate::backend::ConfluenceBackend;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Renders markdown to Confluence XHTML storage format.
///
/// Note: This is distinct from `rw_site::PageRenderer` which renders
/// markdown to HTML for the web server. Both are "page renderers" but for
/// different output formats.
#[derive(Debug)]
pub(crate) struct PageRenderer {
    prepend_toc: bool,
    extract_title: bool,
}

impl Default for PageRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PageRenderer {
    /// Create a new renderer with default settings.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            prepend_toc: false,
            extract_title: false,
        }
    }

    /// Enable or disable prepending a table of contents macro.
    #[must_use]
    pub(crate) fn prepend_toc(mut self, enabled: bool) -> Self {
        self.prepend_toc = enabled;
        self
    }

    /// Enable or disable extracting the first H1 as page title.
    #[must_use]
    pub(crate) fn extract_title(mut self, enabled: bool) -> Self {
        self.extract_title = enabled;
        self
    }

    /// Prepend TOC macro if enabled and there are headings.
    fn maybe_prepend_toc(&self, html: String, toc: &[TocEntry]) -> String {
        if self.prepend_toc && !toc.is_empty() {
            format!("{TOC_MACRO}{html}")
        } else {
            html
        }
    }

    /// Render markdown to Confluence storage format, writing each diagram out
    /// as an attachment along the way.
    ///
    /// Returns the rendered result and the attachment filenames, sorted. The
    /// list is exact — every name comes from a diagram this call resolved — so
    /// a stale file already sitting in `output_dir` is never listed. This
    /// method does not remove one; [`crate::render`] wipes the directory's
    /// PNGs before calling it, because the bundle is also read by whoever
    /// uploads it.
    ///
    /// With no `output_dir` nothing is written and no attachments come back:
    /// the resolutions keep their inline bytes, which storage format has no way
    /// to reference, so their fences render as nothing.
    #[must_use]
    pub(crate) fn render(
        &self,
        markdown_text: &str,
        providers: &Providers,
        output_dir: Option<&Path>,
    ) -> (RenderResult, Vec<String>) {
        let renderer = self.create_renderer(providers);
        let pass = renderer.begin(markdown_text);
        let requests: Vec<DiagramRequest> = pass.requests().iter().map(force_png).collect();
        let mut resolutions = providers.resolve(&requests, &ResolveContext::default());
        let attachments = write_attachments(&mut resolutions, &requests, output_dir);
        let result = pass.finish(&resolutions);

        (
            RenderResult {
                html: self.maybe_prepend_toc(result.html, &result.toc),
                title: result.title,
                toc: result.toc,
                warnings: result.warnings,
                section_refs: result.section_refs,
            },
            attachments,
        )
    }

    /// Build the settings-only renderer.
    fn create_renderer(&self, providers: &Providers) -> MarkdownRenderer<ConfluenceBackend> {
        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        // With no provider configured there is no router either, so every
        // diagram fence stays an ordinary code block.
        if !providers.is_empty() {
            renderer = renderer
                .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>);
        }
        renderer
    }
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
    output_dir: Option<&Path>,
) -> Vec<String> {
    let Some(dir) = output_dir else {
        return Vec::new();
    };

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

    /// Render with no diagram service and no bundle — the shape every
    /// markup-only test below wants.
    fn render(markdown: &str) -> RenderResult {
        PageRenderer::new()
            .render(markdown, &Providers::empty(), None)
            .0
    }

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

        let attachments = write_attachments(&mut resolutions, &requests, Some(dir.path()));

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

        let attachments = write_attachments(&mut resolutions, &requests, Some(dir.path()));

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

        write_attachments(&mut resolutions, &requests, Some(dir.path()));

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

        let attachments = write_attachments(&mut resolutions, &requests, Some(dir.path()));

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

        let attachments = write_attachments(&mut resolutions, &requests, Some(dir.path()));

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

    /// Without a bundle there is nowhere to write, so the bytes stay inline and
    /// nothing is claimed as an attachment.
    #[test]
    fn without_an_output_dir_nothing_is_written_and_nothing_is_claimed() {
        let requests = [request(0, &[])];
        let mut resolutions =
            Resolutions::from([(requests[0].key, Ok(resolved_png(&"e".repeat(64))))]);

        let attachments = write_attachments(&mut resolutions, &requests, None);

        assert!(attachments.is_empty());
        assert!(matches!(
            resolutions[&requests[0].key]
                .as_ref()
                .expect("resolved")
                .asset,
            Asset::Inline(DiagramContent::Png(_)),
        ));
    }

    // ── markup ────────────────────────────────────────────────────────────

    #[test]
    fn test_status_directive_renders_confluence_macro() {
        let result = render(":status[On Track]{color=green}");
        assert!(
            result.html.contains(r#"ac:name="status""#),
            "got: {}",
            result.html
        );
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="colour">Green</ac:parameter>"#),
            "got: {}",
            result.html
        );
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="title">On Track</ac:parameter>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_status_directive_unknown_color_is_grey() {
        let result = render(":status[X]{color=mauve}");
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="colour">Grey</ac:parameter>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn tabs_render_as_bold_label_sections() {
        let r = render(
            "::::tabs\n\n:::tab[macOS]\n\nAlpha\n\n:::\n\n:::tab[Linux]\n\nBeta\n\n:::\n\n::::",
        );
        assert!(
            r.html.contains("<p><strong>macOS</strong></p>"),
            "got: {}",
            r.html
        );
        assert!(r.html.contains("Alpha"), "got: {}", r.html);
        assert!(
            r.html.contains("<p><strong>Linux</strong></p>"),
            "got: {}",
            r.html
        );
        assert!(
            !r.html.contains("::::tabs"),
            "literal syntax leaked: {}",
            r.html
        );
        assert!(
            !r.html.contains("role=\"tablist\""),
            "html chrome leaked: {}",
            r.html
        );
    }
}
