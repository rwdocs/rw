//! Page rendering pipeline.
//!
//! Contains the internal [`PageRenderer`] that handles markdown-to-HTML
//! conversion, page caching, and diagram processing. Also defines the public
//! result and configuration types used by [`Site`](crate::Site).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use parking_lot::Mutex;
use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_diagrams::{DiagramRouter, FileStamp, Providers, ResolveContext, diagram_language};
use rw_error::error_chain;
use rw_kroki::KrokiProvider;
use rw_parser::{Rewrite, rewrite};
use rw_plantuml::{DiagramFile, read_diagram_file, search_text};
use rw_renderer::{
    HtmlBackend, IncludeError, IncludeReader, IncludedFile, LinkResolver, MAX_INCLUDED_BYTES,
    MarkdownRenderer, RenderBackend, SearchDocumentBackend, TitleResolver, TocEntry, escape_html,
    image_file,
};
use rw_sections::SectionAnchor;

use crate::api::PageMeta;
use crate::path::{PageIncludes, included_file};
use crate::site_state::SiteState;
use rw_meta::Meta;
use rw_storage::{
    Document, IncludeRefusal, MAX_FILE_SIZE, PageText, Storage, StorageError, StorageErrorKind,
    download_content_type, file_content_type, is_attachment, mtime_to_datetime,
};
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
    /// The project root on disk, which diagram files (`![alt](flow.puml)`)
    /// are read from. It must be the root the storage places files in. When
    /// `None` and `kroki_url` is set, a diagram file renders as an error
    /// figure. When set, an include directory that resolves outside it at
    /// render time is skipped.
    pub project_dir: Option<PathBuf>,
}

/// One segment of the breadcrumb trail leading to a page.
///
/// Breadcrumbs always start with a "Home" entry (path `"/"`) and include
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
/// where "Home" has `path "/"`, "Domain" has `path "/domain"`, and
/// "Billing" has `path "/domain/billing"`.
#[derive(Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title for this breadcrumb segment.
    pub title: String,
    /// URL path with a leading slash.
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
    /// The `meta` of the page's API response, from the same snapshot as the
    /// rest of the result.
    pub response_meta: PageMeta,
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
    /// Project-relative paths of the files the page's local images name;
    /// refused images are not listed.
    pub files: BTreeSet<String>,
    /// Project-relative paths of the markdown files the page includes, at any
    /// depth; includes that failed are not listed.
    pub includes: BTreeSet<String>,
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
    /// See [`Site::with_deferred_diagrams`](crate::Site::with_deferred_diagrams).
    pub(crate) diagrams_deferred: bool,
    diagram_config_fingerprint: u64,
    /// Where files are read from, so a project path maps to the path
    /// [`read_files`](Self::read_files) holds for it.
    project_dir: Option<PathBuf>,
    /// Every file a render in this process probed, found or not, as probed.
    read_files: Mutex<HashSet<PathBuf>>,
    /// Every project file a render in this process listed in
    /// [`PageRenderResult::files`], ASCII-lowercased: on a case-insensitive
    /// filesystem, `img/A.png` names the file a watcher reports as
    /// `img/a.png`.
    shown_files: Mutex<HashSet<String>>,
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
            Some(url) => {
                let mut kroki = KrokiProvider::new(url)
                    .include_dirs(&config.include_dirs)
                    .with_cache(cache.bucket("diagrams"));
                if let Some(dir) = &config.project_dir {
                    kroki = kroki
                        .project_dir(dir.clone())
                        .confine_includes_to(dir.clone());
                }
                Providers::empty().with(Arc::new(kroki))
            }
            None => Providers::empty(),
        };
        Self {
            storage,
            page_bucket: cache.bucket("pages"),
            include_dirs: config.include_dirs,
            providers: Arc::new(providers),
            diagrams_deferred: false,
            diagram_config_fingerprint,
            project_dir: config.project_dir,
            read_files: Mutex::default(),
            shown_files: Mutex::default(),
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
        let mut body = if document.has_content {
            self.render_content(document, snapshot)?
        } else {
            self.render_virtual(document)
        };
        self.shown_files
            .lock()
            .extend(body.files.iter().map(|file| file.to_ascii_lowercase()));
        // Not cached with the page: an image can appear or vanish without the
        // page changing.
        for file in &body.files {
            if let Err(missing) = self.storage.find_file(file) {
                let noun = if download_content_type(file).is_some() {
                    "linked file"
                } else {
                    "image"
                };
                body.warnings.push(format!("{noun} `{file}` {missing}"));
            }
        }

        // Ancestry for the sections this page is connected to: its own
        // enclosing section, the content and diagram-link refs collected during
        // render (`section_refs`), and each breadcrumb's `section_ref`. Built
        // here — not in the cache-hit path — so cached and freshly-rendered pages
        // carry the same map, rebuilt from live sections every render.
        //
        // Include the page's enclosing section even when a section-root landing
        // page has no links: breadcrumbs exclude the current page, so neither
        // referenced sections nor crumbs would otherwise add it.
        let (section_ref, subpath) = snapshot.section_location(&document.path);
        let refs = body
            .section_refs
            .iter()
            .map(String::as_str)
            .chain([section_ref.as_str()])
            .chain(breadcrumbs.iter().map(|c| c.section_ref.as_str()));
        let section_ancestry = snapshot.sections().ancestry_for(refs);

        let response_meta = PageMeta {
            title: document.meta.title.clone(),
            path: format!("/{}", document.path),
            source_file: if document.has_content {
                document.path.clone()
            } else {
                String::new()
            },
            last_modified: mtime_to_datetime(body.source_mtime).to_rfc3339(),
            description: document.meta.description.clone(),
            kind: document.meta.kind.clone(),
            section_ref,
            subpath,
            document_id: snapshot.document_id(&document.path),
        };

        Ok(PageRenderResult {
            html: body.html,
            meta: Arc::clone(&document.meta),
            toc: body.toc,
            warnings: body.warnings,
            from_cache: body.from_cache,
            has_content: document.has_content,
            response_meta,
            breadcrumbs,
            section_ancestry,
            files: body.files,
            includes: body.includes,
        })
    }

    fn render_content(
        &self,
        document: &Document,
        snapshot: &Arc<SiteState>,
    ) -> Result<RenderedBody, RenderError> {
        let path = document.path.as_str();
        let source_mtime = self.storage.mtime(path).map_err(RenderError::from)?;

        // Etag combines the page's own source mtime and content version, the
        // snapshot's resolution fingerprint (a cross-page change — another
        // page's title/description/section that this render resolves —
        // invalidates this page even though its own file is unchanged), the
        // diagram-config fingerprint (a `kroki_url`/`include_dirs` change
        // invalidates every page so a page rendered under a broken diagram
        // config is not served stale). `mtime` (f64) and the fingerprints
        // never contain ':', and S3's content version is a hex hash, so the
        // ':' delimiter stays unambiguous. The files a page reads are not
        // known until it renders, so they are checked against the cached
        // entry instead.
        let etag = format!(
            "{source_mtime}:{}:{}:{}",
            self.storage.content_version(path).unwrap_or_default(),
            snapshot.resolution_fingerprint(),
            self.diagram_config_fingerprint,
        );

        if let Some(cached) = self.page_bucket.get_json::<CachedPage>(path, &etag)
            && cached.reads.iter().all(CachedRead::is_current)
        {
            self.note_reads(cached.reads.iter().map(|read| &read.path));
            return Ok(RenderedBody {
                html: cached.html,
                toc: cached.toc,
                warnings: cached.warnings,
                from_cache: true,
                source_mtime,
                section_refs: cached.section_refs,
                files: cached.files,
                includes: cached.includes,
            });
        }

        let PageText { text, includes } = self.storage.read(path)?;
        let reads = Arc::new(RenderReads::default());
        let renderer = self.create_renderer(document, snapshot, &reads, includes);
        let pass = renderer.begin(&text);
        let resolve_ctx = ResolveContext {
            model: Some(&**snapshot),
            record_read: Some(&|stamp| reads.stamps.lock().push(stamp)),
        };
        let resolutions = self.providers.resolve(pass.requests(), &resolve_ctx);
        let transient = reads.transient_include.load(Ordering::Relaxed)
            || resolutions
                .iter()
                .any(|r| matches!(r, Err(error) if error.transient));
        let result = pass.finish(&resolutions);
        let mut reads = mem::take(&mut *reads.stamps.lock());
        reads.sort();
        reads.dedup();
        self.note_reads(reads.iter().map(|read| &read.path));

        // A transient diagram failure (Kroki unreachable, a 5xx, or a retryable
        // 4xx) is not persisted, so the page re-renders and can recover once
        // Kroki is back — mirroring the diagram bucket, which caches only
        // successes. A deterministic failure (e.g. Kroki 400 on malformed
        // source) is not transient and still caches, so it does not re-hit
        // Kroki every request. An include that failed to read for a reason its
        // stamp can't see, such as a permission error, is transient too.
        if !transient {
            let reads: Vec<CachedRead> = reads.into_iter().map(Into::into).collect();
            self.page_bucket.set_json(
                path,
                &etag,
                &CachedPageRef {
                    html: &result.html,
                    toc: &result.toc,
                    warnings: &result.warnings,
                    section_refs: &result.section_refs,
                    reads: &reads,
                    files: &result.files,
                    includes: &result.includes,
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
            files: result.files,
            includes: result.includes,
        })
    }

    fn note_reads<'a>(&self, paths: impl Iterator<Item = &'a PathBuf>) {
        self.read_files.lock().extend(paths.cloned());
    }

    /// Whether a render in this process probed any of `changed`, or a file
    /// under one of them.
    pub(crate) fn has_read_file_under(&self, changed: &[PathBuf]) -> bool {
        let changed: Vec<PathBuf> = changed.iter().filter_map(|path| on_disk(path)).collect();
        let reads: Vec<PathBuf> = self.read_files.lock().iter().cloned().collect();
        reads
            .iter()
            .filter_map(|read| on_disk(read))
            .any(|read| changed.iter().any(|changed| read.starts_with(changed)))
    }

    /// Whether a render in this process listed the project file `path` in
    /// [`PageRenderResult::files`], found or not, ignoring ASCII case.
    pub(crate) fn has_shown_file(&self, path: &str) -> bool {
        self.shown_files.lock().contains(&path.to_ascii_lowercase())
    }

    /// Whether a render in this process listed the project file `path` in
    /// [`PageRenderResult::files`], or probed it, ignoring ASCII case in the
    /// file name.
    pub(crate) fn has_used_file(&self, path: &str) -> bool {
        self.has_shown_file(path) || self.has_read_file(path)
    }

    /// Whether a render in this process listed a project file under `dir` in
    /// [`PageRenderResult::files`], ignoring ASCII case, or probed `dir` or a
    /// file under it.
    pub(crate) fn has_used_file_under(&self, dir: &Path) -> bool {
        self.has_read_file_under(&[dir.to_path_buf()]) || self.has_shown_file_under(dir)
    }

    fn has_shown_file_under(&self, dir: &Path) -> bool {
        let Some(dir) = self
            .project_dir
            .as_deref()
            .and_then(|project| dir.strip_prefix(project).ok())
            .and_then(Path::to_str)
        else {
            return false;
        };
        let dir = dir.to_ascii_lowercase();
        // By component, so `img` does not take in `img2/a.png`.
        self.shown_files
            .lock()
            .iter()
            .any(|file| Path::new(file).starts_with(&dir))
    }

    fn has_read_file(&self, path: &str) -> bool {
        let Some(changed) = self.project_dir.as_ref().map(|dir| dir.join(path)) else {
            return false;
        };
        let (Some(name), Some(dir)) = (changed.file_name(), changed.parent()) else {
            return false;
        };
        // Matching names first keeps canonicalizing off the path of every
        // unrelated image. Ignoring case: on a case-insensitive filesystem a
        // page's `Flow.puml` is the file a watcher reports as `flow.puml`.
        let same_name: Vec<PathBuf> = self
            .read_files
            .lock()
            .iter()
            .filter(|read| {
                read.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .cloned()
            .collect();
        if same_name.is_empty() {
            return false;
        }
        let Ok(dir) = dir.canonicalize() else {
            return false;
        };
        same_name
            .iter()
            .filter_map(|read| read.parent()?.canonicalize().ok())
            .any(|parent| parent == dir)
    }

    fn render_virtual(&self, document: &Document) -> RenderedBody {
        RenderedBody {
            html: format!("<h1>{}</h1>\n", escape_html(&document.meta.title)),
            toc: Vec::new(),
            warnings: Vec::new(),
            from_cache: false,
            source_mtime: self.storage.mtime(&document.path).unwrap_or(0.0),
            section_refs: BTreeSet::new(),
            files: BTreeSet::new(),
            includes: BTreeSet::new(),
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

        let PageText { text, includes } = self.storage.read(&document.path)?;
        let includes = SearchIncludes {
            includes: StorageIncludes {
                page_includes: PageIncludes {
                    storage: Arc::clone(&self.storage),
                    published: includes,
                },
                // Search documents are not cached, so nothing reads the stamps.
                reads: Arc::default(),
            },
            include_dirs: self.include_dirs.clone(),
            snapshot: Arc::clone(snapshot),
            project_dir: self.project_dir.clone(),
        };
        let stripped = includes.search_text(&text, document.source.as_deref());

        // Not `with_title_dropped`: the title comes from the site snapshot, so
        // the H1's words would be in neither the title nor the text whenever
        // the two differ.
        //
        // No diagram router, so every fence — diagram or not — reaches
        // `SearchDocumentBackend::code_block` and is indexed as plain text.
        let renderer = Self::configure_renderer_settings(
            MarkdownRenderer::<SearchDocumentBackend>::new().with_include_reader(includes),
            document,
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
        document: &Document,
        snapshot: &Arc<SiteState>,
        reads: &Arc<RenderReads>,
        includes: Option<BTreeMap<String, String>>,
    ) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_include_reader(StorageIncludes {
                page_includes: PageIncludes {
                    storage: Arc::clone(&self.storage),
                    published: includes,
                },
                reads: Arc::clone(reads),
            });
        if !self.diagrams_deferred {
            renderer = renderer
                .with_diagram_languages(Arc::clone(&self.providers) as Arc<dyn DiagramRouter>);
        }

        // `/_api/files` is `rw serve`'s route, in every host's HTML: an
        // embedded viewer moves it under its own API base.
        renderer = renderer
            .with_file_resolver(snapshot.project_files().clone())
            .with_files_base(
                "/_api/files",
                |path| file_content_type(path).is_some(),
                is_attachment,
            );

        Self::configure_renderer_settings(renderer, document, snapshot)
    }

    /// Apply settings shared between renderer creation paths: where the page
    /// sits, sections, wikilinks/title resolver.
    fn configure_renderer_settings<B: RenderBackend>(
        mut renderer: MarkdownRenderer<B>,
        document: &Document,
        snapshot: &Arc<SiteState>,
    ) -> MarkdownRenderer<B> {
        renderer = renderer.with_base_path(&document.path);
        if let Some(source) = &document.source {
            renderer = renderer.with_source_path(source);
        }
        renderer
            .with_sections(Arc::clone(snapshot.sections()))
            .with_link_resolver(SnapshotResolver(Arc::clone(snapshot)))
            .with_wikilinks()
            .with_title_resolver(SnapshotResolver(Arc::clone(snapshot)))
    }
}

/// Owns the snapshot for the renderer's `'static` resolver bounds.
struct SnapshotResolver(Arc<SiteState>);

impl LinkResolver for SnapshotResolver {
    fn page_url(&self, path: &str) -> Option<String> {
        self.0.link_target(path)
    }
}

impl TitleResolver for SnapshotResolver {
    fn resolve_title(&self, path: &str) -> Option<String> {
        Some(self.0.get_page(path)?.meta.title.clone())
    }
}

/// The files one page render read, shared with its [`StorageIncludes`],
/// which the renderer keeps for `'static`.
#[derive(Default)]
struct RenderReads {
    stamps: Mutex<Vec<FileStamp>>,
    transient_include: AtomicBool,
}

// Copies of storage's file limit in crates that do not depend on rw-storage.
const _: () = assert!(MAX_INCLUDED_BYTES as u64 == MAX_FILE_SIZE);
const _: () = assert!(rw_plantuml::MAX_FILE_SIZE == MAX_FILE_SIZE);

/// Reads a page's `::include`d files and stamps each file it probes, so the
/// cached page notices when one changes, appears or goes.
struct StorageIncludes {
    page_includes: PageIncludes,
    reads: Arc<RenderReads>,
}

impl IncludeReader for StorageIncludes {
    fn read(&self, path: &str) -> Result<IncludedFile, IncludeError> {
        let record = |file: &Path| {
            self.reads
                .stamps
                .lock()
                .push(FileStamp::of(file.to_path_buf()));
        };
        self.page_includes
            .read(path, Some(&record))
            .map_err(|error| {
                include_error(&error, || {
                    self.reads.transient_include.store(true, Ordering::Relaxed);
                })
            })
    }
}

/// Reads `::include`d files from the project, for a one-off render with no
/// page cache to keep fresh.
pub struct ProjectIncludes(pub Arc<dyn Storage>);

impl IncludeReader for ProjectIncludes {
    fn read(&self, path: &str) -> Result<IncludedFile, IncludeError> {
        self.0
            .read_include(path, None)
            .map(included_file)
            .map_err(|error| include_error(&error, || {}))
    }
}

/// Why [`Storage::read_include`] failed, as the page warns it. `transient`
/// runs for a failure that may not recur, such as an I/O error.
fn include_error(error: &StorageError, transient: impl FnOnce()) -> IncludeError {
    let refusal = error
        .source()
        .and_then(|source| source.downcast_ref::<IncludeRefusal>());
    match (refusal, error.kind) {
        (Some(IncludeRefusal::OutsideProject), _) => IncludeError::OutsideProject,
        (Some(IncludeRefusal::NotMarkdown), _) => IncludeError::NotMarkdown,
        (Some(IncludeRefusal::TooLarge), _) => IncludeError::TooLarge,
        (Some(IncludeRefusal::NotUtf8), _) => IncludeError::NotUtf8,
        (Some(IncludeRefusal::Misspelled { on_disk }), _) => {
            transient();
            IncludeError::Misspelled {
                on_disk: on_disk.clone(),
            }
        }
        (Some(IncludeRefusal::NotAFile), _) | (None, StorageErrorKind::NotFound) => {
            IncludeError::NotFound
        }
        (Some(refusal), _) => IncludeError::Other(refusal.to_string()),
        (None, _) => {
            transient();
            IncludeError::Other(
                error
                    .source()
                    .map_or_else(|| error.kind.to_string(), error_chain),
            )
        }
    }
}

/// A page's `::include`d files as the search index wants them, each with
/// [`search_text`](Self::search_text) applied.
struct SearchIncludes {
    includes: StorageIncludes,
    include_dirs: Vec<PathBuf>,
    snapshot: Arc<SiteState>,
    project_dir: Option<PathBuf>,
}

impl SearchIncludes {
    /// Diagram fences and files in `markdown`, read from the project file
    /// `source_path`, reduced to their text before the walk rather than during
    /// it: the index wants the words in a diagram, not a picture of one, and
    /// no provider has to run to get them. Parsed as the walk parses it,
    /// wikilinks on.
    fn search_text<'t>(&self, markdown: &'t str, source_path: Option<&str>) -> Cow<'t, str> {
        rewrite(
            markdown,
            true,
            SearchDocumentBackend::TOKENIZE_DIRECTIVES,
            &mut SearchText {
                include_dirs: &self.include_dirs,
                snapshot: &self.snapshot,
                project_dir: self.project_dir.as_deref(),
                source_path,
            },
        )
    }
}

impl IncludeReader for SearchIncludes {
    fn read(&self, path: &str) -> Result<IncludedFile, IncludeError> {
        let file = self.includes.read(path)?;
        Ok(IncludedFile {
            text: self.search_text(&file.text, Some(path)).into_owned(),
            ..file
        })
    }
}

/// A page's diagrams as the search index wants them: a diagram file becomes a
/// fence of its source, and each fence is reduced as [`search_text`] reduces it.
struct SearchText<'a> {
    include_dirs: &'a [PathBuf],
    snapshot: &'a SiteState,
    project_dir: Option<&'a Path>,
    source_path: Option<&'a str>,
}

impl SearchText<'_> {
    fn text(&self, language: &str, source: &str, file: Option<&DiagramFile<'_>>) -> Option<String> {
        search_text(
            language,
            source,
            file,
            self.include_dirs,
            self.project_dir,
            Some(self.snapshot),
        )
    }
}

impl Rewrite for SearchText<'_> {
    fn fence(&mut self, language: &str, body: &str) -> Option<String> {
        self.text(language, body, None)
    }

    fn lone_image(&mut self, dest: &str, _alt: &str) -> Option<(String, String)> {
        let path = image_file(self.snapshot.project_files(), self.source_path, dest)?.ok()?;
        let language = diagram_language(&path)?;
        let content = read_diagram_file(self.project_dir?, &path, None).ok()?;
        // `rewrite` offers no fence it wrote to `fence`.
        let text = self
            .text(language, &content.text, Some(&content.file()))
            .unwrap_or(content.text);
        Some((language.to_owned(), text))
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
    section_refs: BTreeSet<String>,
    files: BTreeSet<String>,
    includes: BTreeSet<String>,
}

/// Cached page data for deserialization (owned).
#[derive(Deserialize)]
struct CachedPage {
    html: String,
    toc: Vec<TocEntry>,
    warnings: Vec<String>,
    section_refs: BTreeSet<String>,
    reads: Vec<CachedRead>,
    files: BTreeSet<String>,
    includes: BTreeSet<String>,
}

/// Borrowed view of cached page data for serialization (zero-copy).
#[derive(Serialize)]
struct CachedPageRef<'a> {
    html: &'a str,
    toc: &'a [TocEntry],
    warnings: &'a [String],
    section_refs: &'a BTreeSet<String>,
    reads: &'a [CachedRead],
    files: &'a BTreeSet<String>,
    includes: &'a BTreeSet<String>,
}

/// `path` with its deepest existing directory canonical, so the spellings a
/// probe and a file watcher use for one file compare equal. Not the whole
/// path: a probe that missed, or a file or directory since moved, has no
/// canonical path.
fn on_disk(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?;
    let parent = path.parent()?;
    let dir = parent.canonicalize().ok().or_else(|| on_disk(parent))?;
    Some(dir.join(name))
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
    use std::fs::{self, File};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, SystemTime};

    use rw_cache::NullCache;
    use rw_diagrams::{
        Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, DiagramSource,
        Resolved,
    };
    use rw_plantuml::prepare_diagram_source;
    use rw_storage::MockStorage;

    use crate::test_support::{doc, snapshot, snapshot_in};

    use super::*;
    use std::assert_matches;
    use std::slice;

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
                .map(|request| {
                    let DiagramSource::Inline(source) = &request.source else {
                        panic!("the walker requests only fences");
                    };
                    match source.trim() {
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
                                prepare_diagram_source(source, &self.include_dirs, None, ctx.model);
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
            diagrams_deferred: false,
            diagram_config_fingerprint: 0,
            project_dir: None,
            read_files: Mutex::default(),
            shown_files: Mutex::default(),
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
    fn a_misspelled_include_names_the_file_on_disk_and_is_transient() {
        let on_disk = "docs/_partials/note.md".to_owned();
        let error = StorageError::from(IncludeRefusal::Misspelled {
            on_disk: on_disk.clone(),
        });
        let transient = AtomicBool::new(false);

        assert_eq!(
            include_error(&error, || transient.store(true, Ordering::Relaxed)),
            IncludeError::Misspelled { on_disk }
        );
        assert!(transient.load(Ordering::Relaxed));
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
    fn a_rendered_page_has_read_its_includes_and_their_missing_candidates() {
        let includes = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        write_include(fallback.path(), "common.iuml", "Alice -> Bob", 1000);
        write_include(includes.path(), "unrelated.iuml", "Carol -> Dave", 1000);
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer_including(
            diagram_storage("!include common.iuml"),
            &cache,
            &[includes.path().to_path_buf(), fallback.path().to_path_buf()],
        );
        let has_read = |path: PathBuf| renderer.has_read_file_under(slice::from_ref(&path));

        assert!(!has_read(fallback.path().join("common.iuml")));
        renderer
            .render(
                &doc("diag", "title: Diagram", true),
                vec![],
                &empty_snapshot(),
            )
            .unwrap();

        assert!(has_read(fallback.path().join("common.iuml")));
        assert!(has_read(includes.path().join("common.iuml")), "a miss");
        assert!(has_read(fallback.path().to_path_buf()), "its directory");
        assert!(!has_read(includes.path().join("unrelated.iuml")));
        let canonical = fallback.path().canonicalize().unwrap();
        assert!(has_read(canonical.join("common.iuml")));
    }

    #[test]
    fn a_read_under_a_renamed_directory_is_read_under_its_old_path() {
        let includes = tempfile::tempdir().unwrap();
        fs::create_dir(includes.path().join("sub")).unwrap();
        write_include(includes.path(), "sub/common.iuml", "Alice -> Bob", 1000);
        let (_dir, cache) = file_cache();
        let (renderer, _) = stub_renderer_including(
            diagram_storage("!include sub/common.iuml"),
            &cache,
            &[includes.path().to_path_buf()],
        );
        renderer
            .render(
                &doc("diag", "title: Diagram", true),
                vec![],
                &empty_snapshot(),
            )
            .unwrap();

        fs::rename(includes.path().join("sub"), includes.path().join("moved")).unwrap();

        assert!(renderer.has_read_file_under(&[includes.path().join("sub")]));
    }

    #[test]
    fn a_cache_hit_has_read_its_includes() {
        let includes = tempfile::tempdir().unwrap();
        write_include(includes.path(), "common.iuml", "Alice -> Bob", 1000);
        let (_dir, cache) = file_cache();
        let page = doc("diag", "title: Diagram", true);
        let render = || {
            let (renderer, _) = stub_renderer_including(
                diagram_storage("!include common.iuml"),
                &cache,
                &[includes.path().to_path_buf()],
            );
            let result = renderer.render(&page, vec![], &empty_snapshot()).unwrap();
            (renderer, result.from_cache)
        };
        render();

        let (renderer, from_cache) = render();

        assert!(from_cache);
        assert!(renderer.has_read_file_under(&[includes.path().join("common.iuml")]));
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
    fn test_render_page_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = doc("missing", "title: Missing", true);
        let result = renderer.render(&page, vec![], &empty_snapshot());

        assert_matches!(result, Err(RenderError::FileNotFound(_)));
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

    /// A leaf page `guide/intro` with a relative, a `/`, a non-image and an
    /// escaping image. Only the relative one's file exists.
    fn image_storage() -> MockStorage {
        MockStorage::new()
            .with_file(
                "guide/intro",
                "Intro",
                "# Intro\n\n![a](img/a.png) ![b](/b.svg) ![c](c.bmp) ![d](../../../d.png)",
            )
            .with_mtime("guide/intro", 1000.0)
            .with_project_file("docs/guide/img/a.png", b"png".as_slice())
    }

    fn image_page() -> Document {
        Document {
            source: Some("docs/guide/intro.md".to_owned()),
            ..doc("guide/intro", "title: Intro", true)
        }
    }

    #[test]
    fn page_images_name_project_files() {
        let renderer = create_renderer(image_storage());
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));

        let result = renderer.render(&page, vec![], &snapshot).unwrap();
        assert_eq!(
            Vec::from_iter(&result.files),
            ["b.svg", "docs/guide/img/a.png"]
        );
        assert_eq!(
            result.warnings,
            [
                "image `c.bmp` is not a supported image type",
                "image `../../../d.png` is outside the project",
                "image `b.svg` not found",
            ]
        );
        assert_eq!(
            result.html,
            "<h1 id=\"intro\">Intro</h1>\
             <p><img src=\"/_api/files/docs/guide/img/a.png\" alt=\"a\"> \
             <img src=\"/_api/files/b.svg\" alt=\"b\"> <img src=\"c.bmp\" alt=\"c\"> \
             <img src=\"../../../d.png\" alt=\"d\"></p>"
        );
    }

    #[test]
    fn search_renders_resolve_no_images() {
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));

        let result = PageRenderer::configure_renderer_settings(
            MarkdownRenderer::<SearchDocumentBackend>::new(),
            &page,
            &snapshot,
        )
        .render("![a](img/a.png) ![c](c.bmp) ![d](../../../d.png)");
        assert!(result.files.is_empty(), "{:?}", result.files);
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn cache_hit_keeps_the_pages_files() {
        let (_dir, cache) = file_cache();
        let renderer = PageRenderer::new(
            Arc::new(image_storage()),
            cache,
            PageRendererConfig::default(),
        );
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));

        let fresh = renderer.render(&page, vec![], &snapshot).unwrap();
        let cached = renderer.render(&page, vec![], &snapshot).unwrap();
        assert!(cached.from_cache);
        assert!(!cached.files.is_empty());
        assert_eq!(cached.files, fresh.files);
        assert_eq!(cached.warnings, fresh.warnings);
    }

    #[test]
    fn cache_hit_checks_its_files_again() {
        let (_dir, cache) = file_cache();
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));
        let missing = PageRenderer::new(
            Arc::new(image_storage()),
            Arc::clone(&cache),
            PageRendererConfig::default(),
        );
        missing.render(&page, vec![], &snapshot).unwrap();

        let found = PageRenderer::new(
            Arc::new(image_storage().with_project_file("b.svg", b"<svg/>".as_slice())),
            cache,
            PageRendererConfig::default(),
        );
        let cached = found.render(&page, vec![], &snapshot).unwrap();
        assert!(cached.from_cache);
        assert_eq!(
            cached.warnings,
            [
                "image `c.bmp` is not a supported image type",
                "image `../../../d.png` is outside the project",
            ]
        );
    }

    #[test]
    fn a_rendered_page_has_shown_its_files_found_or_not() {
        let renderer = create_renderer(image_storage());
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));
        assert!(!renderer.has_shown_file("docs/guide/img/a.png"));

        renderer.render(&page, vec![], &snapshot).unwrap();

        assert!(renderer.has_shown_file("docs/guide/img/a.png"));
        assert!(renderer.has_shown_file("docs/guide/IMG/A.png"));
        assert!(renderer.has_shown_file("b.svg"));
        assert!(!renderer.has_shown_file("c.bmp"));
        assert!(!renderer.has_shown_file("docs/guide/img/b.png"));
    }

    #[test]
    fn a_cache_hit_has_shown_its_files() {
        let (_dir, cache) = file_cache();
        let page = image_page();
        let snapshot = Arc::new(snapshot_in("docs", slice::from_ref(&page)));
        let render = || {
            let renderer = PageRenderer::new(
                Arc::new(image_storage()),
                Arc::clone(&cache),
                PageRendererConfig::default(),
            );
            let result = renderer.render(&page, vec![], &snapshot).unwrap();
            (renderer, result.from_cache)
        };
        render();

        let (renderer, from_cache) = render();

        assert!(from_cache);
        assert!(renderer.has_shown_file("docs/guide/img/a.png"));
    }

    /// An S3 republish that changes only an `!include`d file moves the page
    /// bundle's version but not the page's mtime.
    #[test]
    fn a_new_content_version_invalidates_the_cached_page() {
        let (_dir, cache) = file_cache();
        let page = doc("guide", "title: Guide", true);
        let render = |body: &str, version: &str| {
            let storage = MockStorage::new()
                .with_file("guide", "Guide", format!("# Guide\n\n{body}"))
                .with_mtime("guide", 1000.0)
                .with_content_version("guide", version);
            PageRenderer::new(
                Arc::new(storage),
                Arc::clone(&cache),
                PageRendererConfig::default(),
            )
            .render(&page, vec![], &empty_snapshot())
            .unwrap()
        };

        render("published v1", "v1");
        let republished = render("published v2", "v2");
        assert!(!republished.from_cache);
        assert!(
            republished.html.contains("published v2"),
            "{}",
            republished.html
        );
        assert!(render("published v2", "v2").from_cache);
    }

    #[test]
    fn moving_the_source_dir_in_the_project_invalidates_cached_pages() {
        let (_dir, cache) = file_cache();
        let renderer = PageRenderer::new(
            Arc::new(image_storage()),
            cache,
            PageRendererConfig::default(),
        );
        let page = image_page();
        let render_in = |source_dir| {
            let snapshot = Arc::new(snapshot_in(source_dir, slice::from_ref(&page)));
            renderer.render(&page, vec![], &snapshot).unwrap()
        };

        render_in("docs");
        let moved = render_in("site/docs");
        assert!(!moved.from_cache);
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
    /// the source tells the two renders apart.
    #[test]
    fn moving_a_page_into_a_directory_index_invalidates_its_cached_html() {
        let (_dir, cache) = file_cache();
        let storage = MockStorage::new()
            .with_file("foo", "Foo", "# Foo\n\n[Bar](bar.md)")
            .with_mtime("foo", 1000.0);
        let renderer = PageRenderer::new(Arc::new(storage), cache, PageRendererConfig::default());
        let mut page = doc("foo", "title: Foo", true);

        page.source = Some("foo.md".to_owned());
        let leaf = renderer
            .render(&page, vec![], &Arc::new(snapshot(slice::from_ref(&page))))
            .unwrap();
        assert!(leaf.html.contains(r#"href="/bar""#), "{}", leaf.html);

        page.source = Some("foo/index.md".to_owned());
        let index = renderer
            .render(&page, vec![], &Arc::new(snapshot(slice::from_ref(&page))))
            .unwrap();
        assert!(!index.from_cache);
        assert!(index.html.contains(r#"href="/foo/bar""#), "{}", index.html);
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
    fn search_document_resolves_current_section_wikilinks() {
        let storage = MockStorage::new()
            .with_file("billing/guide", "Guide", "# Guide\n\nSee [[::rates]].")
            .with_mtime("billing/guide", 1000.0);
        let renderer = create_renderer(storage);
        let snapshot = Arc::new(snapshot(&[
            doc("billing", "kind: domain", true),
            doc("billing/guide", "title: Guide", true),
            doc("billing/rates", "title: Billing Rates", true),
        ]));
        let page = doc("billing/guide", "title: Guide", true);

        let text = renderer
            .render_search_document(&page, &snapshot)
            .unwrap()
            .unwrap()
            .text;

        assert!(text.contains("See Billing Rates."), "{text}");
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
            path: "/billing".to_owned(),
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

        // Production always prepends a Home crumb (path "/"), whose nearest
        // section is the implicit root — so unlike the Home-less list above, the
        // root chain is a top-level key in every real response. This models that
        // shape so a regression dropping the root chain would be caught.
        let breadcrumbs = vec![
            BreadcrumbItem {
                title: "Home".to_owned(),
                path: "/".to_owned(),
                section_ref: "section:default/root".to_owned(),
                subpath: String::new(),
            },
            BreadcrumbItem {
                title: "Billing".to_owned(),
                path: "/billing".to_owned(),
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

    #[test]
    fn render_ancestry_always_keys_the_page_section() {
        let anchor = |section_ref: &str, subpath: &str| SectionAnchor {
            section_ref: section_ref.to_owned(),
            subpath: subpath.to_owned(),
        };
        for (documents, path, expected) in [
            (
                vec![doc("guide", "title: Guide", true)],
                "guide",
                vec![anchor("section:default/root", "")],
            ),
            (
                vec![
                    doc("billing", "kind: domain", true),
                    doc("billing/api", "title: API", true),
                ],
                "billing/api",
                vec![
                    anchor("domain:default/billing", ""),
                    anchor("section:default/root", "billing"),
                ],
            ),
        ] {
            let snapshot = Arc::new(snapshot(&documents));
            let storage = MockStorage::new()
                .with_file(path, "Page", "# Page\n\nNo links.")
                .with_mtime(path, 1000.0);
            let page = documents.iter().find(|d| d.path == path).unwrap();

            let result = create_renderer(storage)
                .render(page, Vec::new(), &snapshot)
                .unwrap();

            assert_eq!(
                result.section_ancestry[&result.response_meta.section_ref], expected,
                "{path}"
            );
        }
    }

    #[test]
    fn render_includes_own_chain_for_a_section_root_named_with_reserved_chars() {
        for path in ["q?x", "sys#1", "100%"] {
            let snapshot = domain_snapshot(path);
            let storage = MockStorage::new()
                .with_file(path, "Landing", "# Landing\n\nNo links.")
                .with_mtime(path, 1000.0);
            let renderer = create_renderer(storage);
            let page = doc(path, "kind: domain", true);

            let result = renderer.render(&page, Vec::new(), &snapshot).unwrap();

            let own_ref = format!("domain:default/{path}");
            assert_eq!(
                result.section_ancestry.keys().collect::<Vec<_>>(),
                [&own_ref],
                "{path:?}"
            );
        }
    }
}
