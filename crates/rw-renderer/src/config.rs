//! Settings for [`MarkdownRenderer`](crate::MarkdownRenderer).
//!
//! [`RenderConfig`] holds the configuration the renderer was built with —
//! base paths, the wikilink flag, sections, title resolver. It is non-generic
//! on purpose: none of the read-only helpers reference the backend type `B`.
//! Settings live for the lifetime of the renderer; per-render scratch state
//! lives on `Walker` and is freshly constructed for every call.

use std::sync::Arc;

use rw_diagrams::DiagramRouter;
use rw_sections::Sections;

/// Resolves page paths to their display titles for wikilink rendering.
///
/// When a wikilink like `[[domain:billing::overview]]` has no explicit display
/// text, the renderer calls this trait to look up a human-readable title.
/// If the resolver returns `None`, the renderer falls back to the last path
/// segment.
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
    /// Returns the display title for a page at `path`, or `None` if unknown.
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
///     fn page_url(&self, source_path: &str) -> Option<String> {
///         self.0.get(source_path).map(|url| (*url).to_owned())
///     }
/// }
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("guide/intro.md")
///     .with_link_resolver(Pages(HashMap::from([("guide/setup/index.md", "guide/setup")])));
///
/// let html = renderer.render("[setup](setup/index.md) [api](../api.md)").html;
/// assert!(html.contains(r#"href="/guide/setup""#));
/// assert!(html.contains(r#"href="/api""#));
/// ```
pub trait LinkResolver: Send + Sync {
    /// Returns the url path (no leading slash) of the page published from
    /// `source_path`, or `None` when no page is.
    ///
    /// `source_path` is relative to the source directory and normalized; it
    /// starts with `../` when the link leaves that directory.
    fn page_url(&self, source_path: &str) -> Option<String>;
}

/// The path of a local image, as a [`FileResolver`] gets it: percent-decoded,
/// with `.` and `..` folded, and starting with `../` when it leaves where it
/// starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRef<'a> {
    /// Relative to the source directory: the image's path, resolved against
    /// the page's source file.
    Source(&'a str),
    /// Relative to the project root: the image's path was written with a
    /// leading `/`.
    Project(&'a str),
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
/// use rw_renderer::{FileRef, FileRefusal, FileResolver, HtmlBackend, MarkdownRenderer};
///
/// /// A project that is its own source directory and takes PNGs only.
/// struct Project;
///
/// impl FileResolver for Project {
///     fn project_file(&self, file: FileRef<'_>) -> Result<String, FileRefusal> {
///         let (FileRef::Source(path) | FileRef::Project(path)) = file;
///         if path.starts_with("../") {
///             Err(FileRefusal::OutsideProject)
///         } else if path.ends_with(".png") {
///             Ok(path.to_owned())
///         } else {
///             Err(FileRefusal::NotAllowed)
///         }
///     }
/// }
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("guide/intro.md")
///     .with_file_resolver(Project)
///     .render("![a](img/a.png) ![b](/logo.png) ![c](../../c.png) ![d](https://example.com/d.png)");
///
/// assert_eq!(Vec::from_iter(&result.files), ["guide/img/a.png", "logo.png"]);
/// assert_eq!(result.warnings, ["image `../../c.png` is outside the project"]);
/// assert!(result.html.contains(r#"src="img/a.png""#));
/// ```
pub trait FileResolver: Send + Sync {
    /// Returns the path of the file `file` names, relative to the project root
    /// and normalized, or why it refuses the file.
    fn project_file(&self, file: FileRef<'_>) -> Result<String, FileRefusal>;
}

/// Configuration for [`MarkdownRenderer`](crate::MarkdownRenderer).
///
/// Built up via the renderer's `with_*` builders and read by both the
/// renderer's pipeline methods and the `Walker` during event processing.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RenderConfig {
    /// The page's url path (leading `/`), which picks the current section for
    /// `[[::path]]` wikilinks.
    pub(crate) base_path: Option<String>,
    /// The page's source file, relative to the source directory; relative
    /// markdown links resolve against its directory.
    pub(crate) source_path: Option<String>,
    /// Origin prefix (with trailing slash) for files outside `source_dir`.
    /// Set by [`with_origin`](crate::MarkdownRenderer::with_origin).
    pub(crate) origin_prefix: Option<String>,
    /// `[[wikilink]]` parsing enabled.
    pub(crate) wikilinks: bool,
    /// Extract title from the H1 that opens the page.
    pub(crate) extract_title: bool,
    /// Section registry for wikilink resolution and link annotation.
    pub(crate) sections: Option<Arc<Sections>>,
    /// Title resolver for wikilink display text.
    pub(crate) title_resolver: Option<Box<dyn TitleResolver>>,
    /// Recognises which fence languages are diagrams. The renderer holds only
    /// this, never `Providers`, so it structurally cannot resolve one.
    pub(crate) diagram_router: Option<Arc<dyn DiagramRouter>>,
    /// Maps link targets to page urls; without it links stay as written.
    pub(crate) link_resolver: Option<Box<dyn LinkResolver>>,
    /// Places local images in the project; without it none is resolved.
    pub(crate) file_resolver: Option<Box<dyn FileResolver>>,
    /// Url prefix resolved images are served under; without it their `src`
    /// stays as written.
    pub(crate) files_base: Option<String>,
}

impl RenderConfig {
    /// Defaults: no wikilinks, no title extraction.
    pub(crate) fn new() -> Self {
        Self {
            base_path: None,
            source_path: None,
            origin_prefix: None,
            wikilinks: false,
            extract_title: false,
            sections: None,
            title_resolver: None,
            diagram_router: None,
            link_resolver: None,
            file_resolver: None,
            files_base: None,
        }
    }
}
