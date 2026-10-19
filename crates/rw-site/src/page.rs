//! Page rendering pipeline.
//!
//! Contains the internal [`PageRenderer`] that handles markdown-to-HTML
//! conversion, page caching, and diagram processing. Also defines the public
//! result and configuration types used by [`Site`](crate::Site).

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::Mutex;
use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_diagrams::{DiagramRouter, FileStamp, Providers, ResolveContext};
use rw_kroki::KrokiProvider;
use rw_parser::rewrite_fences;
use rw_plantuml::search_text;
use rw_renderer::{
    HtmlBackend, MarkdownRenderer, RenderBackend, SearchDocumentBackend, TocEntry, escape_html,
};
use rw_sections::SectionAnchor;

use crate::site::SiteTitleResolver;
use crate::site_state::SiteState;
use rw_meta::Meta;
use rw_storage::{Document, Storage, StorageError, StorageErrorKind};
use serde::{Deserialize, Serialize};

/// Controls how [`Site`](crate::Site) renders markdown pages.
///
/// # Examples
///
/// ```
/// use rw_site::PageRendererConfig;
///
/// // Default: no diagram rendering
/// let config = PageRendererConfig::default();
/// assert!(config.kroki_url.is_none());
/// ```
///
/// ```
/// use rw_site::PageRendererConfig;
///
/// // Enable diagram rendering via a Kroki instance
/// let config = PageRendererConfig {
///     kroki_url: Some("https://kroki.io".to_owned()),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct PageRendererConfig {
    /// Base URL of a [Kroki](https://kroki.io) instance for rendering diagrams.
    ///
    /// When `None`, fenced code blocks for diagram languages (`PlantUML`,
    /// Mermaid, etc.) are rendered as syntax-highlighted code instead of images.
    pub kroki_url: Option<String>,
    /// Directories to search when resolving `PlantUML` `!include` directives.
    /// Defaults to empty (no include resolution).
    pub include_dirs: Vec<PathBuf>,
}

/// One segment of the breadcrumb trail leading to a page.
///
/// Breadcrumbs always start with a "Home" entry (path `""`) and include
/// each ancestor page up to (but not including) the current page.
///
/// # Examples
///
/// A page at `"domain/billing/overview"` produces breadcrumbs:
///
/// ```text
/// Home → Domain → Billing
/// ```
///
/// where "Home" has `path ""`, "Domain" has `path "domain"`, and
/// "Billing" has `path "domain/billing"`.
#[derive(Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title for this breadcrumb segment.
    pub title: String,
    /// URL path without leading slash. Empty string for the site root.
    pub path: String,
    /// Section ref of the nearest enclosing section — always resolvable, since
    /// the implicit root matches any path. Pair with [`subpath`](Self::subpath)
    /// and the page response's section-ancestry map (see
    /// [`Sections::ancestry_for`](rw_sections::Sections::ancestry_for)) to resolve a
    /// host URL for this crumb.
    pub section_ref: String,
    /// This crumb's path relative to `section_ref`'s scope root (empty when the
    /// crumb path is exactly that section's root).
    pub subpath: String,
}

/// Output of rendering a single page via [`Site::render`](crate::Site::render).
///
/// Contains the rendered HTML, resolved metadata, table of contents, and
/// breadcrumb trail needed to display a page.
#[derive(Debug)]
pub struct PageRenderResult {
    /// Rendered HTML body content.
    pub html: String,
    /// Holds resolved metadata from the site snapshot. Navigation derives its
    /// values from this metadata, while fresh and cached renders clone the same
    /// allocation. Its title is always non-empty.
    pub meta: Arc<Meta>,
    /// Headings found in the page, used to build a "table of contents" sidebar.
    pub toc: Vec<TocEntry>,
    /// Non-fatal issues encountered during rendering (e.g., unresolved
    /// `!include` directives). Intended for logging or developer tooling,
    /// not for display to end users.
    pub warnings: Vec<String>,
    /// `true` when the HTML was served from cache rather than re-rendered.
    pub from_cache: bool,
    /// `false` for [virtual pages](crate#virtual-pages) that have no markdown source.
    pub has_content: bool,
    /// Source file modification time as a Unix timestamp (seconds since
    /// epoch). Stored as `f64` for sub-second precision and JavaScript
    /// interoperability.
    pub source_mtime: f64,
    /// Ancestor trail from "Home" to the parent of this page.
    /// See [`BreadcrumbItem`] for the structure.
    pub breadcrumbs: Vec<BreadcrumbItem>,
    /// Ancestry chains for the sections this page is connected to: its own
    /// enclosing section, each section it references via prose links and
    /// diagram `$link`s, and each breadcrumb section —
    /// mapped to that section's chain of [`SectionAnchor`]s. Each chain starts
    /// with the section itself (empty subpath), then its ancestors nearest-first
    /// with the root last. The enclosing section's ref always has an entry, so
    /// a host can resolve the page's section context without another lookup.
    /// Rebuilt from live sections on every render, so it is identical for
    /// cached and freshly-rendered pages.
    pub section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
}

/// Page-response metadata that is derived rather than declared, built by
/// [`Site::page_meta_fields`](crate::Site::page_meta_fields) so the HTTP API
/// and `@rwdocs/core` report identical values.
#[derive(Debug, PartialEq, Eq)]
pub struct PageMetaFields {
    /// URL path with a leading slash.
    pub path: String,
    /// The site path for a page with markdown, empty for a
    /// [virtual page](crate#virtual-pages). The homepage's site path is itself
    /// empty, so its `source_file` is empty too.
    pub source_file: String,
    /// RFC 3339 source modification time; the Unix epoch when unknown.
    pub last_modified: String,
    /// Section ref of the page's enclosing section.
    pub section_ref: String,
    /// Page path relative to that section's root.
    pub subpath: String,
}

/// Plain text representation of a page for search indexing.
///
/// Produced by [`Site::render_search_document()`](crate::Site::render_search_document).
/// Contains whitespace-separated tokens suitable for full-text search engines.
#[derive(Debug, Clone)]
pub struct SearchDocument {
    /// Uses [`Meta::title`] as the display title.
    pub title: String,
    /// Plain text content with whitespace-separated tokens.
    pub text: String,
}

/// Reasons why [`Site::render`](crate::Site::render) can fail.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The page exists in the site structure but its markdown source file
    /// is missing from storage.
    #[error("Content not found: {0}")]
    FileNotFound(String),
    /// No page with this URL path exists in the site structure. The path
    /// may be misspelled or the site may need a reload.
    #[error("Page not found: {0}")]
    PageNotFound(String),
    /// The storage backend itself failed (e.g., S3 connectivity issues,
    /// permission errors).
    #[error("Storage error")]
    Storage(#[source] StorageError),
}

impl From<StorageError> for RenderError {
    fn from(e: StorageError) -> Self {
        match e.kind {
            StorageErrorKind::NotFound => Self::FileNotFound(
                e.path
                    .as_deref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
            _ => Self::Storage(e),
        }
    }
}

/// Fingerprint of the diagram configuration that affects rendered output.
///
/// Folded into the page-cache etag so that changing `kroki_url` (including
/// unset→set) or `include_dirs` invalidates cached pages — otherwise a
/// page rendered while diagrams were misconfigured would be served from cache
/// even after the config is fixed.
///
/// Returns a `u64` (rendered as decimal digits in the etag, preserving the
/// `:`-delimited etag contract). Uses `DefaultHasher` for the same reason
/// `compute_resolution_fingerprint` does.
fn diagram_config_fingerprint(kroki_url: Option<&str>, include_dirs: &[PathBuf]) -> u64 {
    let mut hasher = DefaultHasher::new();
    // `Option<&str>` hashes `None` and `Some(_)` distinctly, so presence and
    // value are both captured.
    kroki_url.hash(&mut hasher);
    // Order is significant (include search order), so do not sort.
    include_dirs.hash(&mut hasher);
    hasher.finish()
}

/// Fingerprint of the document fields that set where its relative links
/// point. Neither is in the source file or its mtime: `mv foo.md foo/index.md`
/// flips only `is_dir`.
fn link_base_fingerprint(document: &Document) -> u64 {
    let mut hasher = DefaultHasher::new();
    document.is_dir.hash(&mut hasher);
    document.origin.hash(&mut hasher);
    hasher.finish()
}

/// Page rendering pipeline.
///
/// Handles markdown-to-HTML conversion with caching and diagram processing.
/// Renders one page against the snapshot it is handed; reloading is
/// [`Site`](crate::Site)'s job.
pub(crate) struct PageRenderer {
    storage: Arc<dyn Storage>,
    page_bucket: Box<dyn CacheBucket>,
    /// Kept alongside [`providers`](Self::providers) because the search path
    /// resolves `PlantUML` `!include`s itself, without going through a
    /// provider.
    include_dirs: Vec<PathBuf>,
    /// The diagram providers this site renders through, built once: a provider
    /// owns an HTTP agent whose connection pool should outlive any one page.
    /// Everything that varies per render travels in [`ResolveContext`] instead.
    providers: Arc<Providers>,
    diagram_config_fingerprint: u64,
}

impl PageRenderer {
    /// Create a new page renderer.
    // `cache` is only borrowed from: no field holds it, the buckets taken off
    // it do. It still arrives by value because taking `&Arc<dyn Cache>` would
    // move this same lint onto `Site::new`, which has nowhere to hand
    // ownership either.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(
        storage: Arc<dyn Storage>,
        cache: Arc<dyn Cache>,
        config: PageRendererConfig,
    ) -> Self {
        let diagram_config_fingerprint =
            diagram_config_fingerprint(config.kroki_url.as_deref(), &config.include_dirs);
        let providers = match &config.kroki_url {
            Some(url) => Providers::empty().with(Arc::new(
                KrokiProvider::new(url)
                    .include_dirs(&config.include_dirs)
                    .with_cache(cache.bucket("diagrams")),
            )),
            None => Providers::empty(),
        };
        Self {
            storage,
            page_bucket: cache.bucket("pages"),
            include_dirs: config.include_dirs,
            providers: Arc::new(providers),
            diagram_config_fingerprint,
        }
    }

    /// Render a page with full pipeline: mtime, cache check, render, cache write.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::FileNotFound` if source file doesn't exist.
    pub(crate) fn render(
        &self,
        document: &Document,
        breadcrumbs: Vec<BreadcrumbItem>,
        snapshot: &Arc<SiteState>,
    ) -> Result<PageRenderResult, RenderError> {
        let body = if document.has_content {
            self.render_content(document, snapshot)?
        } else {
            self.render_virtual(document)
        };

        // Ancestry for the sections this page is connected to: its own
        // enclosing section, the content and diagram-link refs collected during
        // render (`section_refs`), and each breadcrumb's `section_ref`. Built
        // here — not in the cache-hit path — so cached and freshly-rendered pages
        // carry the same map, rebuilt from live sections every render.
        //
        // Include the page's enclosing section even when a section-root landing
        // page has no links: breadcrumbs exclude the current page, so neither
        // referenced sections nor crumbs would otherwise add it.
        let own_ref = snapshot
            .sections()
            .find(&document.path)
            .map(|sp| sp.section.to_string());
        let refs = body
            .section_refs
            .iter()
            .map(String::as_str)
            .chain(own_ref.as_deref())
            .chain(breadcrumbs.iter().map(|c| c.section_ref.as_str()));
        let section_ancestry = snapshot.sections().ancestry_for(refs);

        Ok(PageRenderResult {
            html: body.html,
            meta: Arc::clone(&document.meta),
            toc: body.toc,
            warnings: body.warnings,
            from_cache: body.from_cache,
            has_content: document.has_content,
            source_mtime: body.source_mtime,
            breadcrumbs,
            section_ancestry,
        })
    }

    fn render_content(
        &self,
        document: &Document,
        snapshot: &Arc<SiteState>,
    ) -> Result<RenderedBody, RenderError> {
        let path = document.path.as_str();
        let source_mtime = self.storage.mtime(path).map_err(RenderError::from)?;

        // Etag combines the page's own source mtime, the snapshot's resolution
        // fingerprint (a cross-page change — another page's title/description/
        // section that this render resolves — invalidates this page even though
        // its own file is unchanged), the diagram-config fingerprint (a
        // `kroki_url`/`include_dirs` change invalidates every page so a
        // page rendered under a broken diagram config is not served stale),
        // and the link-base fingerprint. `mtime` (f64) never contains ':', and
        // every fingerprint is decimal digits, so the ':' delimiter stays
        // unambiguous. Include files are not known until the page renders, so
        // they are checked against the cached entry instead.
        let etag = format!(
            "{source_mtime}:{}:{}:{}",
            snapshot.resolution_fingerprint(),
            self.diagram_config_fingerprint,
            link_base_fingerprint(document),
        );

        if let Some(cached) = self.page_bucket.get_json::<CachedPage>(path, &etag)
            && cached.reads.iter().all(CachedRead::is_current)
        {
            return Ok(RenderedBody {
                html: cached.html,
                toc: cached.toc,
                warnings: Vec::new(),
                from_cache: true,
                source_mtime,
                section_refs: cached.section_refs,
            });
        }

        let markdown_text = self.storage.read(path)?;
        let renderer =
            self.create_renderer(path, document.origin.as_deref(), document.is_dir, snapshot);
        let pass = renderer.begin(&markdown_text);
        let reads = Mutex::new(Vec::new());
        let resolve_ctx = ResolveContext {
            model: Some(&**snapshot),
            record_read: Some(&|stamp| reads.lock().push(stamp)),
        };
        let resolutions = self.providers.resolve(pass.requests(), &resolve_ctx);
        let transient = resolutions
            .values()
            .any(|r| matches!(r, Err(error) if error.transient));
        let result = pass.finish(&resolutions);

        // A transient diagram failure (Kroki unreachable, a 5xx, or a retryable
        // 4xx) is not persisted, so the page re-renders and can recover once
        // Kroki is back — mirroring the diagram bucket, which caches only
        // successes. A deterministic failure (e.g. Kroki 400 on malformed
        // source) is not transient and still caches, so it does not re-hit
        // Kroki every request.
        if !transient {
            let mut reads = reads.into_inner();
            reads.sort();
            reads.dedup();
            let reads: Vec<CachedRead> = reads.into_iter().map(Into::into).collect();
            self.page_bucket.set_json(
                path,
                &etag,
                &CachedPageRef {
                    html: &result.html,
                    toc: &result.toc,
                    section_refs: &result.section_refs,
                    reads: &reads,
                },
            );
        }

        Ok(RenderedBody {
            html: result.html,
            toc: result.toc,
            warnings: result.warnings,
            from_cache: false,
            source_mtime,
            section_refs: result.section_refs,
        })
    }

    fn render_virtual(&self, document: &Document) -> RenderedBody {
        RenderedBody {
            html: format!("<h1>{}</h1>\n", escape_html(&document.meta.title)),
            toc: Vec::new(),
            warnings: Vec::new(),
            from_cache: false,
            source_mtime: self.storage.mtime(&document.path).unwrap_or(0.0),
            section_refs: BTreeSet::new(),
        }
    }

    pub(crate) fn render_search_document(
        &self,
        document: &Document,
        snapshot: &Arc<SiteState>,
    ) -> Result<Option<SearchDocument>, RenderError> {
        if !document.has_content {
            return Ok(None);
        }

        let markdown_text = self.storage.read(&document.path)?;

        // Diagram fences are reduced to their text before the walk rather than
        // during it: the index wants the words in a diagram, not a picture of
        // one, and no provider has to run to get them.
        let stripped = rewrite_fences(&markdown_text, |lang, src| {
            search_text(lang, src, &self.include_dirs, Some(&**snapshot))
        });

        // No title extraction. The title comes from the site snapshot, and
        // `SearchDocumentBackend::TITLE_AS_METADATA` would otherwise pull the
        // first H1 out of the indexed body — leaving its words in neither the
        // title nor the text whenever the two differ.
        //
        // No diagram router either, so every fence — diagram or not — reaches
        // `SearchDocumentBackend::code_block` and is indexed as plain text.
        let renderer = Self::configure_renderer_settings(
            MarkdownRenderer::<SearchDocumentBackend>::new(),
            snapshot,
        );
        let result = renderer.render(&stripped);

        Ok(Some(SearchDocument {
            title: document.meta.title.clone(),
            text: result.html,
        }))
    }

    fn create_renderer(
        &self,
        base_path: &str,
        origin: Option<&str>,
        is_dir: bool,
        snapshot: &Arc<SiteState>,
    ) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(format!("/{base_path}"))
            .with_is_dir(is_dir);

        if let Some(origin) = origin {
            renderer = renderer.with_origin(origin);
        }

        // The first H1 is always title-extracted. Nothing reads the extracted
        // title — the page's title comes from the site snapshot — but in HTML
        // mode (`TITLE_AS_METADATA = false`) this flag is what keeps that H1
        // out of the ToC (`rw-renderer`'s `HeadingAccumulator::complete_heading`).
        // Drop it and every page grows a duplicate top ToC entry;
        // `test_render_page_toc_generation` is the guard.
        renderer = renderer.with_title_extraction();

        // With no provider configured there is no router either, so every fence
        // stays an ordinary code block.
        if !self.providers.is_empty() {
            renderer = renderer
                .with_diagram_languages(Arc::clone(&self.providers) as Arc<dyn DiagramRouter>);
        }

        Self::configure_renderer_settings(renderer, snapshot)
    }

    /// Apply settings shared between renderer creation paths: sections,
    /// wikilinks/title resolver.
    fn configure_renderer_settings<B: RenderBackend>(
        renderer: MarkdownRenderer<B>,
        snapshot: &Arc<SiteState>,
    ) -> MarkdownRenderer<B> {
        renderer
            .with_sections(Arc::clone(snapshot.sections()))
            .with_readme_links_to_root(snapshot.readme_links_to_root())
            .with_wikilinks()
            .with_title_resolver(SiteTitleResolver {
                snapshot: Arc::clone(snapshot),
            })
    }
}

/// The part of a [`PageRenderResult`] that depends on how the page was produced
/// (fresh, cached, or virtual); [`PageRenderer::render`] adds the rest.
struct RenderedBody {
    html: String,
    toc: Vec<TocEntry>,
    warnings: Vec<String>,
    from_cache: bool,
    source_mtime: f64,
    /// Only the ancestry built from these is public.
    section_refs: BTreeSet<String>,
}

/// Cached page data for deserialization (owned).
#[derive(Deserialize)]
struct CachedPage {
    html: String,
    toc: Vec<TocEntry>,
    section_refs: BTreeSet<String>,
    reads: Vec<CachedRead>,
}

/// Borrowed view of cached page data for serialization (zero-copy).
#[derive(Serialize)]
struct CachedPageRef<'a> {
    html: &'a str,
    toc: &'a [TocEntry],
    section_refs: &'a BTreeSet<String>,
    reads: &'a [CachedRead],
}

/// A [`FileStamp`] as the page cache stores it, so the on-disk format does not
/// follow `rw-diagrams`' type.
#[derive(Serialize, Deserialize)]
struct CachedRead {
    path: PathBuf,
    modified: Option<SystemTime>,
}

impl CachedRead {
    fn is_current(&self) -> bool {
        FileStamp::of(self.path.clone()).modified == self.modified
    }
}

impl From<FileStamp> for CachedRead {
    fn from(stamp: FileStamp) -> Self {
        Self {
            path: stamp.path,
            modified: stamp.modified,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, SystemTime};

    use rw_cache::NullCache;
    use rw_diagrams::{
        Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, Resolved,
    };
    use rw_plantuml::prepare_diagram_source;
    use rw_storage::MockStorage;

    use crate::test_support::{doc, snapshot};

    use super::*;
    use std::assert_matches;

    fn create_renderer(storage: MockStorage) -> PageRenderer {
        let config = PageRendererConfig::default();
        PageRenderer::new(Arc::new(storage), Arc::new(NullCache), config)
    }

    // ── The diagram path, through a stub provider ─────────────────────────

    /// One `plantuml` fence with prose on either side, so a test can see both
    /// what a fence became and where it landed.
    const DIAGRAM_PAGE: &str = "# Diagram\n\nbefore\n\n```plantuml\nA -> B\n```\n\nafter\n";

    /// A provider that answers from the fence source without a network.
    ///
    /// The body is a script: `flaky` fails transiently, `fail` fails
    /// deterministically, and anything else resolves to an SVG echoing the
    /// source, so a test can see which fence filled which hole.
    struct StubProvider {
        /// Whether the last `resolve` was handed a site model. Shared with the
        /// test, which cannot reach the provider once it is behind an `Arc<dyn
        /// DiagramProvider>`.
        saw_model: Arc<AtomicBool>,
        include_dirs: Vec<PathBuf>,
    }

    impl DiagramProvider for StubProvider {
        fn handles(&self, language: &str) -> bool {
            language == "plantuml"
        }

        fn resolve(
            &self,
            requests: &[&DiagramRequest],
            ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            self.saw_model.store(ctx.model.is_some(), Ordering::SeqCst);
            requests
                .iter()
                .map(|request| match request.source.trim() {
                    "flaky" => Err(DiagramError {
                        message: "kroki is unreachable".to_owned(),
                        transient: true,
                    }),
                    "fail" => Err(DiagramError {
                        message: "kroki said no".to_owned(),
                        transient: false,
                    }),
                    source => {
                        // Keep metadata resolution real; only replace the remote
                        // renderer with an SVG echo of its prepared input.
                        let prepared =
                            prepare_diagram_source(source, &self.include_dirs, ctx.model);
                        if let Some(record) = ctx.record_read {
                            prepared.reads.into_iter().for_each(record);
                        }
                        Ok(Resolved {
                            asset: Asset::Inline(DiagramContent::Svg(format!(
                                "<svg>{}</svg>",
                                prepared.source
                            ))),
                            size: None,
                            digest: "0".to_owned(),
                            warnings: prepared.warnings,
                        })
                    }
                })
                .collect()
        }
    }

    /// A renderer whose diagram fences route to a [`StubProvider`], plus the
    /// flag recording whether that provider was handed a site model.
    ///
    /// Built field-by-field rather than through [`PageRenderer::new`], which
    /// only ever builds a Kroki provider. What `new` wires is covered by the
    /// tests that go through it.
    fn stub_renderer(
        storage: MockStorage,
        cache: &Arc<dyn Cache>,
    ) -> (PageRenderer, Arc<AtomicBool>) {
        stub_renderer_including(storage, cache, &[])
    }

    fn stub_renderer_including(
        storage: MockStorage,
        cache: &Arc<dyn Cache>,
        include_dirs: &[PathBuf],
    ) -> (PageRenderer, Arc<AtomicBool>) {
        let saw_model = Arc::new(AtomicBool::new(false));
        let provider = StubProvider {
            saw_model: Arc::clone(&saw_model),
            include_dirs: include_dirs.to_vec(),
        };
        let renderer = PageRenderer {
            storage: Arc::new(storage),
            page_bucket: cache.bucket("pages"),
            include_dirs: include_dirs.to_vec(),
            providers: Arc::new(Providers::empty().with(Arc::new(provider))),
            diagram_config_fingerprint: 0,
        };
        (renderer, saw_model)
    }

    fn diagram_storage(body: &str) -> MockStorage {
        MockStorage::new()
            .with_file(
                "diag",
                "Diagram",
                format!("# Diagram\n\n```plantuml\n{body}\n```\n"),
            )
            .with_mtime("diag", 1000.0)
    }

    fn file_cache() -> (tempfile::TempDir, Arc<dyn Cache>) {
        let dir = tempfile::tempdir().unwrap();
        let cache: Arc<dyn Cache> =
            Arc::new(rw_cache::FileCache::new(dir.path().join("cache"), "1.0.0"));
        (dir, cache)
    }

    fn empty_snapshot() -> Arc<SiteState> {
        Arc::new(snapshot(&[]))
    }

    /// A snapshot whose only section is a `domain` page at `path`.
    fn domain_snapshot(path: &str) -> Arc<SiteState> {
        Arc::new(snapshot(&[doc(path, "kind: domain", true)]))
    }

    #[test]
    fn unresolved_metadata_include_warning_reaches_page_response() {
        let cache: Arc<dyn Cache> = Arc::new(NullCache);
        let (renderer, _) =
            stub_renderer(diagram_storage("!include systems/sys_missing.iuml"), &cache);
        let page = doc("diag", "title: Diagram", true);
        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].contains("systems/sys_missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn ambiguous_includes_warn_on_render_and_search_but_not_html_cache_hits() {
        use crate::test_support::{assert_candidate_order, capture_warnings};

        let snapshot = Arc::new(snapshot(&[
            doc(
                "a/shared",
                "kind: system\nnamespace: billing\ntitle: Shared system",
                true,
            ),
            doc(
                "b/shared",
                "kind: system\nnamespace: shipping\ntitle: Shared system",
                true,
            ),
        ]));
        let (_dir, cache) = file_cache();
        let storage = diagram_storage("!include systems/sys_shared.iuml")
            .with_file("quiet", "Quiet", "# Quiet\n\nUnrelated prose")
            .with_mtime("quiet", 1000.0);
        let (renderer, _) = stub_renderer(storage, &cache);
        let page = doc("diag", "title: Diagram", true);

        let (fresh, logs) = capture_warnings(|| renderer.render(&page, vec![], &snapshot).unwrap());
        assert!(!fresh.from_cache);
        assert!(
            fresh.html.contains("System(sys_shared, \"Shared system\""),
            "{}",
            fresh.html
        );
        assert!(
            fresh.warnings.is_empty(),
            "logs must not become response warnings"
        );
        assert_eq!(logs.lines().count(), 1, "{logs}");
        assert_candidate_order(
            &logs,
            &[
                ("system:billing/shared", "/a/shared"),
                ("system:shipping/shared", "/b/shared"),
            ],
        );

        let (cached, logs) =
            capture_warnings(|| renderer.render(&page, vec![], &snapshot).unwrap());
        assert!(cached.from_cache);
        assert_eq!(cached.html, fresh.html);
        assert_eq!(cached.warnings, [] as [String; 0]);
        assert!(logs.is_empty(), "{logs}");

        let (search, logs) = capture_warnings(|| {
            renderer
                .render_search_document(&page, &snapshot)
                .unwrap()
                .unwrap()
        });
        assert!(search.text.contains("Shared system"), "{}", search.text);
        assert!(!search.text.contains("!include"), "{}", search.text);
        assert_eq!(logs.lines().count(), 1, "{logs}");
        assert_candidate_order(
            &logs,
            &[
                ("system:billing/shared", "/a/shared"),
                ("system:shipping/shared", "/b/shared"),
            ],
        );

        let quiet_page = doc("quiet", "title: Quiet", true);
        let (quiet, logs) =
            capture_warnings(|| renderer.render(&quiet_page, vec![], &snapshot).unwrap());
        assert!(!quiet.from_cache);
        assert!(quiet.html.contains("Unrelated prose"));
        assert!(logs.is_empty(), "{logs}");
        let (search, logs) = capture_warnings(|| {
            renderer
                .render_search_document(&quiet_page, &snapshot)
                .unwrap()
                .unwrap()
        });
        assert!(search.text.contains("Unrelated prose"));
        assert!(logs.is_empty(), "{logs}");
    }

    /// Sets a distinct mtime too: the stamp is mtime-only, and a write in the
    /// same tick as the render would otherwise be invisible to it.
    fn write_include(dir: &Path, name: &str, body: &str, mtime_secs: u64) {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs))
            .unwrap();
    }

    #[test]
    fn editing_an_included_file_invalidates_the_cached_page() {
        let includes = tempfile::tempdir().unwrap();
        write_include(includes.path(), "common.iuml", "Alice -> Bob", 1000);
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer_including(
            diagram_storage("!include common.iuml"),
            &cache,
            &[includes.path().to_path_buf()],
        );
        let page = doc("diag", "title: Diagram", true);

        let first = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(first.html.contains("Alice -> Bob"), "{}", first.html);
        let unchanged = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(unchanged.from_cache);

        write_include(includes.path(), "common.iuml", "Carol -> Dave", 2000);
        let edited = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!edited.from_cache);
        assert!(edited.html.contains("Carol -> Dave"), "{}", edited.html);
    }

    #[test]
    fn creating_a_missing_include_invalidates_the_cached_page() {
        let includes = tempfile::tempdir().unwrap();
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer_including(
            diagram_storage("!include common.iuml"),
            &cache,
            &[includes.path().to_path_buf()],
        );
        let page = doc("diag", "title: Diagram", true);

        let missing = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            missing.warnings[0].contains("not found"),
            "{:?}",
            missing.warnings
        );
        let still_missing = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(still_missing.from_cache);

        write_include(includes.path(), "common.iuml", "Alice -> Bob", 1000);
        let created = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!created.from_cache);
        assert!(created.html.contains("Alice -> Bob"), "{}", created.html);
    }

    #[test]
    fn a_resolved_diagram_is_spliced_where_its_fence_stood() {
        let storage = MockStorage::new()
            .with_file("diag", "Diagram", DIAGRAM_PAGE)
            .with_mtime("diag", 1000.0);
        let cache: Arc<dyn Cache> = Arc::new(NullCache);
        let (renderer, _) = stub_renderer(storage, &cache);
        let page = doc("diag", "title: Diagram", true);

        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        // The stub echoes the prepared source, so PlantUML config precedes the body.
        let opening = r#"<figure class="diagram" data-diagram-id="diagram-0"><rw-diagram><svg>"#;
        let closing = "A -> B</svg></rw-diagram></figure>";
        assert!(
            result.html.contains(opening) && result.html.contains(closing),
            "the resolution did not become a diagram figure: {}",
            result.html,
        );
        // Position, not just presence: a fill written to the wrong hole would
        // still leave the figure somewhere on the page.
        let figure = result.html.find("<figure").expect("the diagram figure");
        let before = result
            .html
            .find("<p>before</p>")
            .expect("the prose above the fence");
        let after = result
            .html
            .find("<p>after</p>")
            .expect("the prose below the fence");
        assert!(
            before < figure && figure < after,
            "the figure landed outside the fence's position: {}",
            result.html,
        );
    }

    #[test]
    fn a_transient_provider_failure_is_not_cached() {
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer(diagram_storage("flaky"), &cache);
        let page = doc("diag", "title: Diagram", true);

        let first = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!first.from_cache);
        assert!(
            first.html.contains("diagram-error"),
            "a failed diagram shows as an error figure: {}",
            first.html,
        );

        let second = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            !second.from_cache,
            "a transient failure must re-render, so the page can recover",
        );
    }

    /// The other half of the contract: a failure that will fail identically
    /// every time is cached, so a broken diagram does not re-hit the provider
    /// on every request.
    #[test]
    fn a_deterministic_provider_failure_is_cached() {
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer(diagram_storage("fail"), &cache);
        let page = doc("diag", "title: Diagram", true);

        let first = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!first.from_cache);
        assert!(
            first.html.contains("diagram-error"),
            "a failed diagram shows as an error figure: {}",
            first.html,
        );

        let second = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            second.from_cache,
            "a deterministic failure must cache, or every request re-renders it",
        );
        assert_eq!(second.html, first.html);
    }

    #[test]
    fn without_a_diagram_service_a_diagram_fence_stays_a_code_block() {
        // `PageRendererConfig::default()` leaves `kroki_url` unset, so no
        // provider is configured and the renderer gets no router at all.
        let renderer = create_renderer(diagram_storage("A -> B"));
        let page = doc("diag", "title: Diagram", true);

        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert!(
            result
                .html
                .contains(r#"<pre><code class="language-plantuml">A -&gt; B"#),
            "the fence should render as ordinary code: {}",
            result.html,
        );
        assert!(
            !result.html.contains("<figure"),
            "no diagram service means no diagram figure: {}",
            result.html,
        );
    }

    #[test]
    fn the_site_model_reaches_the_provider_through_the_resolve_context() {
        let cache: Arc<dyn Cache> = Arc::new(NullCache);
        let (renderer, saw_model) = stub_renderer(diagram_storage("A -> B"), &cache);
        let page = doc("diag", "title: Diagram", true);

        renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            saw_model.load(Ordering::SeqCst),
            "the snapshot's site model did not reach the provider",
        );
    }

    #[test]
    fn test_render_page_returns_html() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = doc("test", "title: Hello", true);
        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.meta.title, "Hello");
        assert!(!result.from_cache);
        assert!(result.has_content);
    }

    #[test]
    fn test_render_readme_with_origin_resolves_links_correctly() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home\n\n[Guide](docs/guide.md)")
            .with_mtime("", 1000.0);
        let renderer = create_renderer(storage);

        let mut page = doc("", "title: Home", true);
        page.origin = Some("docs".to_owned());
        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert!(
            result.html.contains(r#"href="/guide""#),
            "Expected href=\"/guide\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_render_page_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = doc("missing", "title: Missing", true);
        let result = renderer.render(&page, vec![], &empty_snapshot());

        assert_matches!(result, Err(RenderError::FileNotFound(_)));
    }

    #[test]
    fn test_render_virtual_page() {
        let storage = MockStorage::new().with_mtime("my-domain", 1000.0);
        let renderer = create_renderer(storage);

        let page = doc("my-domain", "title: My Domain", false);
        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert_eq!(result.html, "<h1>My Domain</h1>\n");
        assert_eq!(result.meta.title, "My Domain");
        assert!(!result.has_content);
        assert_eq!(result.toc, [] as [rw_renderer::TocEntry; 0]);
    }

    #[test]
    fn attrs_use_current_canonical_meta_on_fresh_cached_and_virtual_pages() {
        let (_temp, cache) = file_cache();
        let storage = MockStorage::new()
            .with_file("page", "Page", "# Page\n\nUnchanged body")
            .with_mtime("page", 1000.0);
        let renderer = PageRenderer::new(Arc::new(storage), cache, PageRendererConfig::default());
        let mut before = doc("page", "title: Page", true);
        Arc::make_mut(&mut before.meta)
            .attrs
            .insert("owner".into(), serde_json::json!("old"));
        let mut after = before.clone();
        Arc::make_mut(&mut after.meta)
            .attrs
            .insert("owner".into(), serde_json::json!("new"));
        assert!(!Arc::ptr_eq(&before.meta, &after.meta));
        let snapshot = empty_snapshot();
        let first = renderer.render(&before, vec![], &snapshot).unwrap();
        let second = renderer.render(&after, vec![], &snapshot).unwrap();
        assert!(!first.from_cache);
        assert!(second.from_cache);
        assert_eq!(first.html, second.html);
        assert!(Arc::ptr_eq(&first.meta, &before.meta));
        assert_eq!(first.meta.attrs["owner"], serde_json::json!("old"));
        assert!(Arc::ptr_eq(&second.meta, &after.meta));
        assert_eq!(second.meta.attrs["owner"], serde_json::json!("new"));

        after.has_content = false;
        let virtual_page = renderer.render(&after, vec![], &snapshot).unwrap();
        assert!(!virtual_page.has_content);
        assert!(Arc::ptr_eq(&virtual_page.meta, &after.meta));
        assert_eq!(virtual_page.meta.attrs["owner"], serde_json::json!("new"));
    }

    #[test]
    fn test_render_page_cache_hit() {
        let (_dir, cache) = file_cache();

        let storage = MockStorage::new()
            .with_file("test", "Cached", "# Cached\n\nContent")
            .with_mtime("test", 1000.0);

        let config = PageRendererConfig::default();
        let renderer = PageRenderer::new(Arc::new(storage), cache, config);
        let page = doc("test", "title: Cached", true);

        let result1 = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!result1.from_cache);

        let result2 = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(result2.from_cache);
        assert_eq!(result1.html, result2.html);
    }

    #[test]
    fn cache_hit_preserves_referenced_section_refs() {
        let (_dir, cache) = file_cache();

        // Outside the linked section, so only the link can bring it in.
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\n[billing](/domains/billing)")
            .with_mtime("guide", 1000.0);
        let snapshot = domain_snapshot("domains/billing");

        let config = PageRendererConfig::default();
        let renderer = PageRenderer::new(Arc::new(storage), cache, config);
        let page = doc("guide", "title: Guide", true);

        let fresh = renderer.render(&page, vec![], &snapshot).unwrap();
        assert!(!fresh.from_cache);

        let cached = renderer.render(&page, vec![], &snapshot).unwrap();
        assert!(cached.from_cache);

        assert!(
            cached
                .section_ancestry
                .contains_key("domain:default/billing"),
            "{:?}",
            cached.section_ancestry
        );
        assert_eq!(cached.section_ancestry, fresh.section_ancestry);
    }

    #[test]
    fn diagram_config_fingerprint_distinguishes_inputs() {
        use std::path::PathBuf;

        let base = diagram_config_fingerprint(None, &[]);

        // Presence of kroki_url matters (unset vs set).
        assert_ne!(base, diagram_config_fingerprint(Some("http://k"), &[]));
        // Value of kroki_url matters (switching servers).
        assert_ne!(
            diagram_config_fingerprint(Some("http://a"), &[]),
            diagram_config_fingerprint(Some("http://b"), &[]),
        );
        // include_dirs matter.
        assert_ne!(
            base,
            diagram_config_fingerprint(None, &[PathBuf::from("/inc")]),
        );
        // Stable for identical inputs.
        assert_eq!(base, diagram_config_fingerprint(None, &[]));
    }

    #[test]
    fn changing_kroki_url_invalidates_cached_page() {
        let (_dir, cache) = file_cache();
        let storage: Arc<dyn rw_storage::Storage> = Arc::new(
            MockStorage::new()
                .with_file("test", "Doc", "# Doc\n\nProse only.")
                .with_mtime("test", 1000.0),
        );
        let page = doc("test", "title: Doc", true);

        // Render + cache with kroki_url = None.
        let cfg_none = PageRendererConfig::default();
        let renderer_a = PageRenderer::new(Arc::clone(&storage), Arc::clone(&cache), cfg_none);
        let r1 = renderer_a.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!r1.from_cache);

        // A new renderer with a different kroki_url over the SAME cache/storage
        // must miss (etag changed via the diagram-config fingerprint).
        let cfg_some = PageRendererConfig {
            kroki_url: Some("http://kroki.example".to_owned()),
            ..PageRendererConfig::default()
        };
        let renderer_b = PageRenderer::new(Arc::clone(&storage), Arc::clone(&cache), cfg_some);
        let r2 = renderer_b.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            !r2.from_cache,
            "changing kroki_url should invalidate the cached page"
        );

        // Same config now hits.
        let r3 = renderer_b.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(r3.from_cache);
    }

    /// `mv foo.md foo/index.md` keeps both the URL path and the mtime, so only
    /// `is_dir` tells the two renders apart.
    #[test]
    fn moving_a_page_into_a_directory_index_invalidates_its_cached_html() {
        let (_dir, cache) = file_cache();
        let storage = MockStorage::new()
            .with_file("foo", "Foo", "# Foo\n\n[Bar](bar.md)")
            .with_mtime("foo", 1000.0);
        let renderer = PageRenderer::new(Arc::new(storage), cache, PageRendererConfig::default());
        let mut page = doc("foo", "title: Foo", true);

        page.is_dir = false;
        let leaf = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(leaf.html.contains(r#"href="/bar""#), "{}", leaf.html);

        page.is_dir = true;
        let index = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!index.from_cache);
        assert!(index.html.contains(r#"href="/foo/bar""#), "{}", index.html);
    }

    #[test]
    fn changing_a_pages_origin_invalidates_its_cached_html() {
        let (_dir, cache) = file_cache();
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home\n\n[Guide](docs/guide.md)")
            .with_mtime("", 1000.0);
        let renderer = PageRenderer::new(Arc::new(storage), cache, PageRendererConfig::default());
        let mut page = doc("", "title: Home", true);

        let plain = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            plain.html.contains(r#"href="/docs/guide""#),
            "{}",
            plain.html
        );

        page.origin = Some("docs".to_owned());
        let from_docs = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!from_docs.from_cache);
        assert!(
            from_docs.html.contains(r#"href="/guide""#),
            "{}",
            from_docs.html
        );
    }

    #[test]
    fn a_page_rendered_without_a_diagram_service_is_cached() {
        let (_dir, cache) = file_cache();
        // kroki_url = None → diagram fence falls back to a code block, no error.
        let storage = diagram_storage("@startuml\nA -> B\n@enduml");
        let page = doc("diag", "title: Diagram", true);

        let renderer = PageRenderer::new(Arc::new(storage), cache, PageRendererConfig::default());
        let r1 = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(!r1.from_cache);
        let r2 = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
        assert!(
            r2.from_cache,
            "a page with no transient failure should cache"
        );
    }

    #[test]
    fn test_render_page_toc_generation() {
        let storage = MockStorage::new()
            .with_file("test", "Title", "# Title\n\n## Section 1\n\n## Section 2")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = doc("test", "title: Title", true);
        let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();

        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[1].title, "Section 2");
    }

    #[test]
    fn test_render_search_document() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld **bold** and `code`.")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = doc("test", "title: Hello", true);
        let result = renderer
            .render_search_document(&page, &empty_snapshot())
            .unwrap();

        let doc = result.unwrap();
        assert_eq!(doc.title, "Hello");
        assert!(doc.text.contains("World"));
        assert!(doc.text.contains("bold"));
        assert!(doc.text.contains("code"));
        assert!(!doc.text.contains('<'));
    }

    #[test]
    fn test_render_search_document_virtual_page_returns_none() {
        let storage = MockStorage::new().with_mtime("virtual", 1000.0);
        let renderer = create_renderer(storage);

        let page = doc("virtual", "title: Virtual", false);
        let result = renderer
            .render_search_document(&page, &empty_snapshot())
            .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_render_search_document_uses_resolved_page_title() {
        let storage = MockStorage::new()
            .with_file("test", "Page Title", "# H1 Title\n\nContent")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = doc("test", "title: Page Title", true);
        let result = renderer
            .render_search_document(&page, &empty_snapshot())
            .unwrap()
            .unwrap();

        assert_eq!(result.title, "Page Title");
        assert!(
            result.text.contains("H1 Title"),
            "the H1 must be indexed as body text, got: {}",
            result.text
        );
    }

    #[test]
    fn test_render_search_document_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = doc("missing", "title: Missing", true);
        let result = renderer.render_search_document(&page, &empty_snapshot());

        assert_matches!(result, Err(RenderError::FileNotFound(_)));
    }

    #[test]
    fn render_populates_ancestry_only_for_referenced_refs() {
        let snapshot = domain_snapshot("billing");
        let storage = MockStorage::new()
            .with_file("billing", "Billing", "# Billing\n\nText")
            .with_mtime("billing", 1000.0);
        let renderer = create_renderer(storage);
        let page = doc("billing", "title: Billing", true);

        // A breadcrumb into the billing section makes that ref reachable; the
        // implicit root section is not referenced here, so it must NOT be a key —
        // the response carries only the referenced chains, not the whole site.
        let breadcrumbs = vec![BreadcrumbItem {
            title: "Billing".to_owned(),
            path: "billing".to_owned(),
            section_ref: "domain:default/billing".to_owned(),
            subpath: String::new(),
        }];
        let result = renderer.render(&page, breadcrumbs, &snapshot).unwrap();

        assert_eq!(
            result.section_ancestry["domain:default/billing"],
            vec![
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: String::new(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "billing".to_owned(),
                },
            ]
        );
        // Reachable-only: root is in billing's chain but not itself referenced.
        assert!(!result.section_ancestry.contains_key("section:default/root"));
        assert_eq!(result.section_ancestry.len(), 1);
    }

    #[test]
    fn render_includes_root_chain_when_home_crumb_present() {
        let snapshot = domain_snapshot("billing");
        let storage = MockStorage::new()
            .with_file("billing", "Billing", "# Billing\n\nText")
            .with_mtime("billing", 1000.0);
        let renderer = create_renderer(storage);
        let page = doc("billing", "title: Billing", true);

        // Production always prepends a Home crumb (path ""), whose nearest
        // section is the implicit root — so unlike the Home-less list above, the
        // root chain is a top-level key in every real response. This models that
        // shape so a regression dropping the root chain would be caught.
        let breadcrumbs = vec![
            BreadcrumbItem {
                title: "Home".to_owned(),
                path: String::new(),
                section_ref: "section:default/root".to_owned(),
                subpath: String::new(),
            },
            BreadcrumbItem {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                section_ref: "domain:default/billing".to_owned(),
                subpath: String::new(),
            },
        ];
        let result = renderer.render(&page, breadcrumbs, &snapshot).unwrap();

        // The Home crumb's root ref is now a top-level key, mapped to the root's
        // own single-rung chain.
        assert_eq!(
            result.section_ancestry["section:default/root"],
            vec![SectionAnchor {
                section_ref: "section:default/root".to_owned(),
                subpath: String::new(),
            }]
        );
        // The billing crumb's chain is unchanged (itself, then root).
        assert_eq!(
            result.section_ancestry["domain:default/billing"],
            vec![
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: String::new(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "billing".to_owned(),
                },
            ]
        );
        assert_eq!(result.section_ancestry.len(), 2);
    }
}
