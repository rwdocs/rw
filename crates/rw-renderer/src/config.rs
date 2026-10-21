//! Settings for [`MarkdownRenderer`](crate::MarkdownRenderer).
//!
//! [`RenderConfig`] lives as long as the renderer; per-render state lives on
//! `Walker`.

use std::sync::Arc;

use rw_diagrams::DiagramRouter;
use rw_sections::Sections;

use crate::include::IncludeReader;

/// Resolves page paths to their display titles for wikilink rendering.
///
/// When a wikilink like `[[domain:billing::overview]]` has no explicit display
/// text, the renderer calls this trait to look up a human-readable title.
/// If the resolver returns `None`, there is no such page: the wikilink renders
/// as broken, with a warning.
///
/// # Examples
///
/// ```
/// use rw_renderer::TitleResolver;
///
/// struct MapResolver(std::collections::HashMap<String, String>);
///
/// impl TitleResolver for MapResolver {
///     fn resolve_title(&self, path: &str) -> Option<String> {
///         self.0.get(path).cloned()
///     }
/// }
///
/// let mut titles = std::collections::HashMap::new();
/// titles.insert("domains/billing/overview".into(), "Billing Overview".into());
/// let resolver = MapResolver(titles);
///
/// assert_eq!(
///     resolver.resolve_title("domains/billing/overview"),
///     Some("Billing Overview".into()),
/// );
/// assert_eq!(resolver.resolve_title("unknown/page"), None);
/// ```
pub trait TitleResolver: Send + Sync {
    /// Returns the display title for a page at `path`, or `None` if no page
    /// is there.
    ///
    /// `path` is an absolute path without leading slash
    /// (e.g., `"domains/billing/overview"`).
    fn resolve_title(&self, path: &str) -> Option<String>;
}

/// Finds the page a markdown file is published as, for links to that file.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// use rw_renderer::{HtmlBackend, LinkResolver, MarkdownRenderer};
///
/// struct Pages(HashMap<&'static str, &'static str>);
///
/// impl LinkResolver for Pages {
///     fn page_url(&self, path: &str) -> Option<String> {
///         self.0.get(path).map(|url| (*url).to_owned())
///     }
/// }
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("guide/intro.md")
///     .with_link_resolver(Pages(HashMap::from([("guide/setup/index.md", "guide/setup")])));
///
/// let html = renderer.render("[setup](setup/index.md) [api](../api.md)").html;
/// assert!(html.contains(r#"href="/guide/setup""#));
/// assert!(html.contains(r#"href="../api.md""#));
/// ```
pub trait LinkResolver: Send + Sync {
    /// Returns the url path (no leading slash) a link to the markdown file
    /// `path` leads to, or `None` to leave the link as written.
    ///
    /// `path` is percent-decoded and normalized, in the space the [source
    /// path](crate::MarkdownRenderer::with_source_path) is in, or starts with
    /// `/` for a link written from the site root. A link that climbs above
    /// that space is left as written without asking.
    fn page_url(&self, path: &str) -> Option<String>;
}

/// Why a [`FileResolver`] places no file. The page warns with the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRefusal {
    /// The path leaves the project.
    OutsideProject,
    /// The file is not of a type the caller takes.
    NotAllowed,
}

/// Places the file a local image names in the project.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, ProjectFiles};
///
/// // A project that takes PNGs only.
/// let project = ProjectFiles::new(|path| path.ends_with(".png"));
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("guide/intro.md")
///     .with_file_resolver(project)
///     .render("![a](img/a.png) ![b](/logo.png) ![c](../../c.png) ![d](https://example.com/d.png)");
///
/// assert_eq!(Vec::from_iter(&result.files), ["guide/img/a.png", "logo.png"]);
/// assert_eq!(result.warnings, ["image `../../c.png` is outside the project"]);
/// assert!(result.html.contains(r#"src="img/a.png""#));
/// ```
pub trait FileResolver: Send + Sync {
    /// Returns the path of the file a local image names, relative to the
    /// project root and normalized, or why it refuses the file.
    ///
    /// `path` is the image's path relative to the project root,
    /// percent-decoded, with `.` and `..` folded, and starting with `../`
    /// when it leaves the project.
    fn project_file(&self, path: &str) -> Result<String, FileRefusal>;
}

/// Picks the `src` an image renders with once a [`FileResolver`] has placed
/// its file.
///
/// Called once per occurrence, including images the backend then leaves out
/// of its output.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, ImageSrc, MarkdownRenderer, ProjectFiles};
///
/// struct Flat;
///
/// impl ImageSrc for Flat {
///     fn image_src(&self, path: &str, _src: &str) -> Option<String> {
///         Some(path.replace('/', "_"))
///     }
/// }
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("guide/intro.md")
///     .with_file_resolver(ProjectFiles::new(|path| path.ends_with(".png")))
///     .with_image_src(Flat)
///     .render("![a](img/a.png)");
///
/// assert!(result.html.contains(r#"src="guide_img_a.png""#));
/// ```
pub trait ImageSrc: Send + Sync {
    /// The `src` for the image written as `src`, whose file is at `path` in
    /// the project, or `None` to keep `src` as written.
    fn image_src(&self, path: &str, src: &str) -> Option<String>;
}

/// Configuration for [`MarkdownRenderer`](crate::MarkdownRenderer), built up
/// via its `with_*` builders.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
pub(crate) struct RenderConfig {
    /// The page's path, which picks the current section for `[[::path]]`
    /// wikilinks.
    pub(crate) base_path: Option<String>,
    /// The page's source file; relative markdown links and images resolve
    /// against its directory.
    pub(crate) source_path: Option<String>,
    /// `[[wikilink]]` parsing enabled.
    pub(crate) wikilinks: bool,
    /// Leave the title H1 out of the output.
    pub(crate) drop_title: bool,
    /// Section registry for wikilink resolution and link annotation.
    pub(crate) sections: Option<Arc<Sections>>,
    /// Title resolver for wikilink display text.
    pub(crate) title_resolver: Option<Box<dyn TitleResolver>>,
    /// Recognises which fence languages are diagrams.
    pub(crate) diagram_router: Option<Arc<dyn DiagramRouter>>,
    /// Maps link targets to page urls; without it links stay as written.
    pub(crate) link_resolver: Option<Box<dyn LinkResolver>>,
    /// Places local images in the project; without it none is resolved.
    pub(crate) file_resolver: Option<Box<dyn FileResolver>>,
    /// Gives resolved images their `src`; without it their `src` stays as
    /// written.
    pub(crate) image_src: Option<Box<dyn ImageSrc>>,
    /// Reads included files; without it every `::include` warns.
    pub(crate) include_reader: Option<Box<dyn IncludeReader>>,
}
