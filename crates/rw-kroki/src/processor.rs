//! Code block processor for diagram languages.
//!
//! This module provides [`DiagramProcessor`], which implements the
//! [`CodeBlockProcessor`] trait for extracting diagram code blocks during rendering.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rw_renderer::{CodeBlockProcessor, ExtractedCodeBlock, FenceAttrs, Fills, ProcessResult};
use ureq::Agent;

use crate::cache::DiagramKey;
use crate::consts::{DEFAULT_DPI, DEFAULT_TIMEOUT};
use crate::html_embed::{annotate_svg_links, scale_svg_dimensions, strip_google_fonts_import};
use crate::kroki::{
    DiagramError, DiagramRequest, create_agent, render_all, render_all_png_data_uri_partial,
    render_all_svg_partial,
};
use crate::language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};
use crate::meta_includes::MetaIncludeSource;
use crate::output::{DiagramOutput, RenderedDiagramInfo, TagGenerator};
use crate::plantuml::{PrepareResult, prepare_diagram_source, resolve_includes};
use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_sections::Sections;

/// Configuration for diagram processing (immutable after setup).
///
/// Separated from mutable state to allow borrowing config while mutating state,
/// avoiding unnecessary clones in Rust's ownership model.
struct ProcessorConfig {
    /// Kroki server URL for rendering diagrams (required).
    kroki_url: String,
    /// Directories to search for `PlantUML` `!include` files.
    include_dirs: Vec<PathBuf>,
    /// DPI for diagram rendering (default: 192).
    dpi: u32,
    /// HTTP timeout for Kroki requests (default: 30 seconds).
    timeout: Duration,
    /// Cache for diagram rendering (defaults to no-op cache).
    cache: Box<dyn CacheBucket>,
    /// Output mode for diagram rendering.
    output: DiagramOutput,
    /// HTTP agent for connection pooling (reused across render calls).
    agent: Agent,
    /// Optional metadata source for resolving virtual `PlantUML` includes.
    meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    /// Sections for annotating SVG links with section ref data attributes.
    sections: Option<Arc<Sections>>,
}

/// Code block processor for diagram languages.
///
/// Extracts diagram code blocks (`PlantUML`, Mermaid, `GraphViz`, etc.) and defers
/// them during rendering. Deferred holes are filled with rendered diagrams via
/// [`fills`](CodeBlockProcessor::fills).
///
/// # Configuration
///
/// Create the processor with a required Kroki URL, then configure using builder methods:
/// - [`include_dirs`](Self::include_dirs): Set directories for `PlantUML` `!include` resolution
/// - [`dpi`](Self::dpi): Set DPI for diagram rendering (default: 192)
///
/// # Example
///
/// ```no_run
/// use rw_kroki::DiagramProcessor;
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
///
/// let processor = DiagramProcessor::new("https://kroki.io")
///     .dpi(192);
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
/// let pipeline = Pipeline::new().with_processor(processor);
///
/// // render auto-calls fills() on all processors
/// let result = renderer.render(markdown, pipeline);
/// ```
pub struct DiagramProcessor {
    /// Configuration (immutable after setup).
    config: ProcessorConfig,
    /// Extracted code blocks (accumulated during processing).
    extracted: Vec<ExtractedCodeBlock>,
    /// Warnings (accumulated during processing).
    warnings: Vec<String>,
    /// Whether any diagram hit a transient render failure (network error, Kroki
    /// 5xx, or a retryable 4xx — see
    /// [`DiagramErrorKind::is_transient`](crate::kroki::DiagramErrorKind::is_transient))
    /// during [`fills`](CodeBlockProcessor::fills). Consumed via
    /// [`has_transient_error`](CodeBlockProcessor::has_transient_error).
    has_transient_error: bool,
    /// Canonical section refs referenced by diagram `$link`s during
    /// [`fills`](CodeBlockProcessor::fills). Consumed via
    /// [`section_refs`](CodeBlockProcessor::section_refs).
    section_refs: BTreeSet<String>,
}

impl DiagramProcessor {
    /// Create a new diagram processor with the given Kroki server URL.
    ///
    /// # Arguments
    ///
    /// * `kroki_url` - Kroki server URL for rendering diagrams (e.g., `"https://kroki.io"`)
    ///
    /// # Example
    ///
    /// ```
    /// # use rw_kroki::DiagramProcessor;
    /// let processor = DiagramProcessor::new("https://kroki.io");
    /// ```
    #[must_use]
    pub fn new(kroki_url: impl Into<String>) -> Self {
        Self {
            config: ProcessorConfig {
                kroki_url: kroki_url.into(),
                include_dirs: Vec::new(),
                dpi: DEFAULT_DPI,
                timeout: DEFAULT_TIMEOUT,
                cache: rw_cache::NullCache.bucket("diagrams"),
                output: DiagramOutput::default(),
                agent: create_agent(DEFAULT_TIMEOUT),
                meta_include_source: None,
                sections: None,
            },
            extracted: Vec::new(),
            warnings: Vec::new(),
            has_transient_error: false,
            section_refs: BTreeSet::new(),
        }
    }

    /// Set directories to search for `PlantUML` `!include` files.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::path::PathBuf;
    /// # use rw_kroki::DiagramProcessor;
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .include_dirs(&[PathBuf::from("docs"), PathBuf::from("includes")]);
    /// ```
    #[must_use]
    pub fn include_dirs(mut self, dirs: &[PathBuf]) -> Self {
        self.config.include_dirs = dirs.to_vec();
        self
    }

    /// Set DPI for diagram rendering.
    ///
    /// Default is 192 (2x for retina displays). Set to 96 for standard resolution.
    ///
    /// # Example
    ///
    /// ```
    /// # use rw_kroki::DiagramProcessor;
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .dpi(96); // Standard resolution
    /// ```
    #[must_use]
    pub fn dpi(mut self, dpi: u32) -> Self {
        self.config.dpi = dpi;
        self
    }

    /// Set HTTP timeout for Kroki requests.
    ///
    /// Default is 30 seconds. Increase for slow networks or large diagrams.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// # use rw_kroki::DiagramProcessor;
    ///
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .timeout(Duration::from_secs(60)); // 60 second timeout
    /// ```
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self.config.agent = create_agent(timeout);
        self
    }

    /// Set the diagram cache for content-based caching.
    ///
    /// When a cache is provided, [`fills`](CodeBlockProcessor::fills) will:
    /// 1. Compute a content hash for each diagram
    /// 2. Check the cache for a hit before rendering via Kroki
    /// 3. Store newly rendered diagrams in the cache
    ///
    /// # Example
    ///
    /// ```
    /// use rw_cache::{Cache, NullCache};
    /// use rw_kroki::DiagramProcessor;
    ///
    /// let cache = NullCache;
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .with_cache(cache.bucket("diagrams"));
    /// ```
    #[must_use]
    pub fn with_cache(mut self, cache: Box<dyn CacheBucket>) -> Self {
        self.config.cache = cache;
        self
    }

    /// Set the output mode for diagram rendering.
    ///
    /// Default is [`DiagramOutput::Inline`].
    ///
    /// # Example
    ///
    /// ```
    /// use rw_kroki::{DiagramProcessor, DiagramOutput};
    ///
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .output(DiagramOutput::Inline);
    /// ```
    #[must_use]
    pub fn output(mut self, output: DiagramOutput) -> Self {
        self.config.output = output;
        self
    }

    /// Set metadata source for resolving virtual `PlantUML` includes.
    ///
    /// When set, `!include systems/sys_*.iuml` paths are resolved from page
    /// metadata instead of the filesystem. Filesystem includes still work as fallback.
    #[must_use]
    pub fn with_meta_include_source(mut self, source: Arc<dyn MetaIncludeSource>) -> Self {
        self.config.meta_include_source = Some(source);
        self
    }

    /// Set sections for annotating SVG links with `data-section-ref` attributes.
    #[must_use]
    pub fn with_sections(mut self, sections: Arc<Sections>) -> Self {
        if sections.is_empty() {
            self.config.sections = None;
        } else {
            self.config.sections = Some(sections);
        }
        self
    }

    /// Annotate SVG links if sections are configured, recording each resolved
    /// ref in `refs`.
    fn annotate_links(config: &ProcessorConfig, svg: &str, refs: &mut BTreeSet<String>) -> String {
        match &config.sections {
            Some(sections) => annotate_svg_links(svg, sections, refs),
            None => svg.to_owned(),
        }
    }

    /// Build the HTML for an inlined SVG diagram figure.
    ///
    /// The SVG is wrapped in `<rw-diagram>`, which the viewer upgrades into a
    /// shadow root. Kroki generators emit ids that are unique only within one SVG
    /// (Vega hard-codes `clip0, clip1, …`; Mermaid roots every SVG on `container`),
    /// so without a per-diagram tree scope a `url(#clip1)` reference resolves
    /// document-wide to whichever diagram came first — silently painting one
    /// diagram with another's clip paths.
    ///
    /// Only the inline SVG path is wrapped. PNG figures hold an `<img>` with no ids
    /// to collide, and error figures hold a `<pre>`.
    fn svg_figure(id_attr: &str, svg: &str) -> String {
        format!(r#"<figure class="diagram"{id_attr}><rw-diagram>{svg}</rw-diagram></figure>"#)
    }

    /// Prepare diagram source for rendering.
    ///
    /// For `PlantUML` diagrams, this resolves `!include` directives and injects config.
    /// For other diagram types, returns the source as-is.
    fn prepare_source(config: &ProcessorConfig, diagram: &ExtractedDiagram) -> PrepareResult {
        if diagram.language.needs_plantuml_preprocessing() {
            prepare_diagram_source(
                &diagram.source,
                &config.include_dirs,
                config.dpi,
                config.meta_include_source.as_deref(),
            )
        } else {
            PrepareResult {
                source: diagram.source.clone(),
                warnings: Vec::new(),
            }
        }
    }
}

impl CodeBlockProcessor for DiagramProcessor {
    fn process(
        &mut self,
        language: &str,
        attrs: &FenceAttrs,
        source: &str,
        index: usize,
    ) -> ProcessResult {
        let Some(diagram_language) = DiagramLanguage::parse(language) else {
            return ProcessResult::PassThrough;
        };

        // Parse format attribute with validation
        let format = attrs.map.get("format").map_or(DiagramFormat::default(), |value| {
            DiagramFormat::parse(value).unwrap_or_else(|| {
                self.warnings.push(format!(
                    "diagram {index}: unknown format value '{value}', using default 'svg' (valid: svg, png)"
                ));
                DiagramFormat::default()
            })
        });

        // Warn about unknown attributes
        for key in attrs.map.keys().filter(|k| *k != "format") {
            self.warnings.push(format!(
                "diagram {index}: unknown attribute '{key}' ignored (valid: format)"
            ));
        }

        // Only `format` and `endpoint` are read downstream (to_extracted_diagram),
        // so store just those instead of cloning the whole brace map — its other
        // keys were already warned about above and are never read again.
        let stored_attrs = HashMap::from([
            ("format".to_owned(), format.as_str().to_owned()),
            (
                "endpoint".to_owned(),
                diagram_language.kroki_endpoint().to_owned(),
            ),
        ]);

        // Extract the code block
        self.extracted.push(ExtractedCodeBlock::new(
            index,
            language.to_owned(),
            source.to_owned(),
            attrs.id.clone(),
            stored_attrs,
        ));

        ProcessResult::Deferred
    }

    fn fills(&mut self, fills: &mut Fills) {
        // Reset up front so these two fields are fully determined by this
        // render rather than carried over from a previous one. This runs before
        // the early return deliberately, and the renderer calls `fills` on every
        // processor regardless of whether any hole was reserved.
        //
        // NOTE: `self.extracted` and `self.warnings` are NOT cleared here (or
        // anywhere else) — an instance must not be reused across renders, or
        // the next render's fills/warnings will be mixed with this one's.
        self.has_transient_error = false;
        self.section_refs.clear();

        let diagrams = to_extracted_diagrams(&self.extracted);
        if diagrams.is_empty() {
            return;
        }

        self.has_transient_error = match &self.config.output {
            DiagramOutput::Inline => Self::resolve_inline(
                &self.config,
                &mut self.warnings,
                &mut self.section_refs,
                fills,
                &diagrams,
            ),
            DiagramOutput::Files {
                output_dir,
                tag_generator,
            } => Self::resolve_files(
                &self.config,
                &mut self.warnings,
                fills,
                &diagrams,
                output_dir,
                tag_generator,
            ),
        };
    }

    fn extracted(&self) -> &[ExtractedCodeBlock] {
        &self.extracted
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn has_transient_error(&self) -> bool {
        self.has_transient_error
    }

    fn section_refs(&self) -> &BTreeSet<String> {
        &self.section_refs
    }

    fn bundle(&mut self, language: &str, source: &str) -> Option<String> {
        let lang = DiagramLanguage::parse(language)?;
        if !lang.needs_plantuml_preprocessing() {
            return None;
        }
        let mut warnings = Vec::new();
        let resolved = resolve_includes(
            source,
            &self.config.include_dirs,
            None, // Skip meta includes — resolved at request time
            0,
            &mut warnings,
        );
        self.warnings.extend(warnings);
        if resolved == source {
            None
        } else {
            Some(resolved)
        }
    }
}

/// Data needed to cache a rendered diagram.
struct CacheInfo {
    index: usize,
    source: String,
    endpoint: &'static str,
    format: &'static str,
}

impl CacheInfo {
    fn key(&self, dpi: u32) -> DiagramKey<'_> {
        DiagramKey {
            source: &self.source,
            endpoint: self.endpoint,
            format: self.format,
            dpi,
        }
    }
}

/// Diagram rendering implementation.
///
/// These methods take `&ProcessorConfig` and `&mut Vec<String>` separately,
/// allowing immutable config access while mutating warnings (idiomatic Rust pattern).
impl DiagramProcessor {
    /// Resolve diagrams in inline output mode.
    ///
    /// Checks cache first, renders only cache misses via Kroki, and collects
    /// the section refs resolved from diagram links into `refs`.
    fn resolve_inline(
        config: &ProcessorConfig,
        warnings: &mut Vec<String>,
        refs: &mut BTreeSet<String>,
        fills: &mut Fills,
        diagrams: &[ExtractedDiagram],
    ) -> bool {
        // Resolve each diagram's id: explicit `{#id}`, else `diagram-<n>` where n
        // is the diagram's zero-based position among diagrams on the page.
        // Number by `pos` (the enumerate index over the diagrams-only slice), NOT
        // `d.index` (the block's position among ALL code blocks): a page that
        // interleaves non-diagram code blocks would otherwise get sparse ids like
        // `diagram-0`, `diagram-3`, breaking the sequential numbering docs promise.
        let resolved: Vec<(usize, String)> = diagrams
            .iter()
            .enumerate()
            .map(|(pos, d)| {
                (
                    d.index,
                    d.id.clone().unwrap_or_else(|| format!("diagram-{pos}")),
                )
            })
            .collect();

        // Warn on duplicate resolved ids (auto ids are unique by construction;
        // this catches explicit-vs-explicit and explicit-vs-auto collisions).
        let mut seen: HashMap<&str, usize> = HashMap::with_capacity(resolved.len());
        for (idx, id) in &resolved {
            if let Some(&first) = seen.get(id.as_str()) {
                warnings.push(format!(
                    "diagram {idx}: duplicate id '{id}' (already used by diagram {first})"
                ));
            } else {
                seen.insert(id, *idx);
            }
        }

        let id_by_index: HashMap<usize, String> = resolved.into_iter().collect();

        // Collect all figures for single-pass fill
        let mut figures = Figures::with_capacity(diagrams.len());
        figures.set_ids(id_by_index);

        // Prepare all diagrams
        let prepared: Vec<_> = diagrams
            .iter()
            .map(|diagram| {
                let prepare_result = Self::prepare_source(config, diagram);
                warnings.extend(prepare_result.warnings);
                (diagram, prepare_result.source)
            })
            .collect();

        // Separate cache hits from misses
        let mut svg_to_render: Vec<(DiagramRequest, CacheInfo)> = Vec::new();
        let mut png_to_render: Vec<(DiagramRequest, CacheInfo)> = Vec::new();

        for (diagram, source) in prepared {
            let endpoint = diagram.language.kroki_endpoint();
            let format = diagram.format.as_str();
            let key = DiagramKey {
                source: &source,
                endpoint,
                format,
                dpi: config.dpi,
            };
            let hash = key.compute_hash();

            // Etag is empty: diagrams use content-addressed hashing (the key
            // IS the hash), so etag validation is unnecessary. Version-level
            // invalidation is handled by FileCache's VERSION file.
            if let Some(cached_content) = config.cache.get_string(&hash, "") {
                // Cache hit: add figure directly
                let id_attr = figures.id_attr(diagram.index);
                let figure = match diagram.format {
                    DiagramFormat::Svg => {
                        let annotated = Self::annotate_links(config, &cached_content, refs);
                        Self::svg_figure(&id_attr, &annotated)
                    }
                    DiagramFormat::Png => {
                        format!(
                            r#"<figure class="diagram"{id_attr}><img src="{cached_content}" alt="diagram"></figure>"#
                        )
                    }
                };
                figures.add(diagram.index, figure);
            } else {
                // Cache miss: clone for CacheInfo, move into DiagramRequest
                let cache_info = CacheInfo {
                    index: diagram.index,
                    source: source.clone(),
                    endpoint,
                    format,
                };
                let request = DiagramRequest::new(diagram.index, source, diagram.language);

                match diagram.format {
                    DiagramFormat::Svg => svg_to_render.push((request, cache_info)),
                    DiagramFormat::Png => png_to_render.push((request, cache_info)),
                }
            }
        }

        // Render cache misses and collect figures
        let svg_transient = Self::render_and_cache_svg(config, &mut figures, refs, svg_to_render);
        let png_transient = Self::render_and_cache_png(config, &mut figures, png_to_render);

        figures.into_fills(fills);

        svg_transient || png_transient
    }

    /// Render SVG diagrams, cache results, collect figures, and record
    /// section refs resolved from each diagram's links into `refs`.
    fn render_and_cache_svg(
        config: &ProcessorConfig,
        figures: &mut Figures,
        refs: &mut BTreeSet<String>,
        to_render: Vec<(DiagramRequest, CacheInfo)>,
    ) -> bool {
        if to_render.is_empty() {
            return false;
        }

        let (requests, cache_map) = extract_requests_and_cache_info(to_render);

        let result = render_all_svg_partial(&requests, &config.kroki_url, &config.agent);
        for r in result.rendered {
            let clean_svg = strip_google_fonts_import(r.svg.trim());
            let scaled_svg = scale_svg_dimensions(&clean_svg, config.dpi);

            if let Some(info) = cache_map.get(&r.index) {
                let hash = info.key(config.dpi).compute_hash();
                config.cache.set_string(&hash, "", &scaled_svg);
            }

            let annotated = Self::annotate_links(config, &scaled_svg, refs);
            let id_attr = figures.id_attr(r.index);
            let figure = Self::svg_figure(&id_attr, &annotated);
            figures.add(r.index, figure);
        }
        figures.add_errors(result.errors)
    }

    /// Render PNG diagrams, cache results, and collect figures.
    fn render_and_cache_png(
        config: &ProcessorConfig,
        figures: &mut Figures,
        to_render: Vec<(DiagramRequest, CacheInfo)>,
    ) -> bool {
        if to_render.is_empty() {
            return false;
        }

        let (requests, cache_map) = extract_requests_and_cache_info(to_render);

        let result = render_all_png_data_uri_partial(&requests, &config.kroki_url, &config.agent);
        for r in result.rendered {
            if let Some(info) = cache_map.get(&r.index) {
                let hash = info.key(config.dpi).compute_hash();
                config.cache.set_string(&hash, "", &r.data_uri);
            }

            let id_attr = figures.id_attr(r.index);
            let figure = format!(
                r#"<figure class="diagram"{id_attr}><img src="{}" alt="diagram"></figure>"#,
                r.data_uri
            );
            figures.add(r.index, figure);
        }
        figures.add_errors(result.errors)
    }

    /// Resolve diagrams in file-based output mode.
    ///
    /// Renders diagrams to PNG files and fills holes with custom tags.
    fn resolve_files(
        config: &ProcessorConfig,
        warnings: &mut Vec<String>,
        fills: &mut Fills,
        diagrams: &[ExtractedDiagram],
        output_dir: &std::path::Path,
        tag_generator: &TagGenerator,
    ) -> bool {
        // Collect all figures for single-pass fill
        let mut figures = Figures::with_capacity(diagrams.len());

        // Prepare all diagrams
        let diagram_requests: Vec<_> = diagrams
            .iter()
            .map(|d| {
                let prepare_result = Self::prepare_source(config, d);
                warnings.extend(prepare_result.warnings);
                DiagramRequest::new(d.index, prepare_result.source, d.language)
            })
            .collect();

        let server_url = config.kroki_url.trim_end_matches('/');

        let result = render_all(
            &diagram_requests,
            server_url,
            output_dir,
            config.dpi,
            &config.agent,
        );
        for r in result.rendered {
            let info = RenderedDiagramInfo::new(r.filename, r.width, r.height);
            let tag = tag_generator(&info, config.dpi);
            figures.add(r.index, tag);
        }
        let transient = figures.add_errors(result.errors);

        figures.into_fills(fills);

        transient
    }
}

/// Extract requests and build index-to-cache-info mapping from render queue.
fn extract_requests_and_cache_info(
    to_render: Vec<(DiagramRequest, CacheInfo)>,
) -> (Vec<DiagramRequest>, HashMap<usize, CacheInfo>) {
    let (requests, cache_infos): (Vec<_>, Vec<_>) = to_render.into_iter().unzip();
    let cache_map = cache_infos
        .into_iter()
        .map(|info| (info.index, info))
        .collect();
    (requests, cache_map)
}

/// Render the optional `data-diagram-id` attribute (leading space included), or
/// an empty string when there is no id. The value is HTML-attribute-escaped.
fn diagram_id_attr(id: Option<&str>) -> String {
    use rw_renderer::escape_html;
    id.map_or(String::new(), |id| {
        format!(r#" data-diagram-id="{}""#, escape_html(id))
    })
}

/// Collects one rendered figure per diagram code-block index, then hands them
/// to the renderer as hole fills.
struct Figures {
    map: HashMap<usize, String>,
    /// Resolved id per diagram code-block index (explicit or `diagram-<n>`).
    /// Populated only on the Inline path (`resolve_inline`); the
    /// Files/Confluence path builds figures via a caller-supplied `tag_generator`
    /// with no `data-diagram-id` insertion point, so it never resolves ids and
    /// this map stays empty there.
    id_by_index: HashMap<usize, String>,
}

impl Figures {
    #[cfg(test)]
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            id_by_index: HashMap::new(),
        }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            id_by_index: HashMap::new(),
        }
    }

    /// Attach the resolved id map (explicit or auto `diagram-<n>`) computed by
    /// the caller.
    fn set_ids(&mut self, ids: HashMap<usize, String>) {
        self.id_by_index = ids;
    }

    /// Resolved id for a diagram code-block index, if one was set via
    /// [`set_ids`](Self::set_ids).
    fn id_for(&self, index: usize) -> Option<&str> {
        self.id_by_index.get(&index).map(String::as_str)
    }

    /// The `data-diagram-id` attribute string for a diagram index (leading
    /// space included), or empty when no id was set — e.g. the Files path.
    fn id_attr(&self, index: usize) -> String {
        diagram_id_attr(self.id_for(index))
    }

    /// Add a figure for a diagram code-block index.
    fn add(&mut self, index: usize, content: String) {
        self.map.insert(index, content);
    }

    /// Add an error message for a diagram code-block index.
    fn add_error(&mut self, index: usize, error_msg: &str) {
        use rw_renderer::escape_html;

        let id_attr = self.id_attr(index);
        let error_figure = format!(
            r#"<figure class="diagram diagram-error"{id_attr}><pre>Diagram rendering failed: {}</pre></figure>"#,
            escape_html(error_msg)
        );
        self.add(index, error_figure);
    }

    /// Record every render error as an error figure and report whether any was
    /// transient (a network / 5xx / retryable-4xx failure a retry could
    /// recover). The caller uses the return value to decline to cache a page
    /// that hit a transient failure — see
    /// [`DiagramErrorKind::is_transient`](crate::kroki::DiagramErrorKind::is_transient).
    fn add_errors(&mut self, errors: Vec<DiagramError>) -> bool {
        let transient = errors.iter().any(|e| e.kind.is_transient());
        for e in errors {
            self.add_error(e.index, &e.to_string());
        }
        transient
    }

    /// Move every collected figure into `fills`, keyed by code-block index.
    ///
    /// The renderer reserved a hole per deferred block under that same index,
    /// so each figure lands at the offset its fence occupied.
    fn into_fills(self, fills: &mut Fills) {
        for (index, content) in self.map {
            let key = u32::try_from(index).expect("code block index exceeds hole key width");
            fills.set(key, content);
        }
    }
}

/// Convert an [`ExtractedCodeBlock`] to an [`ExtractedDiagram`].
///
/// Returns `None` if the code block is not a diagram type.
#[must_use]
pub(crate) fn to_extracted_diagram(block: &ExtractedCodeBlock) -> Option<ExtractedDiagram> {
    let language = DiagramLanguage::parse(&block.language)?;
    let format = block
        .attrs()
        .get("format")
        .and_then(|s| DiagramFormat::parse(s))
        .unwrap_or_default();

    Some(ExtractedDiagram {
        source: block.source.clone(),
        index: block.index,
        language,
        format,
        id: block.id().map(str::to_owned),
    })
}

/// Convert multiple [`ExtractedCodeBlock`]s to [`ExtractedDiagram`]s.
///
/// Filters out non-diagram blocks automatically.
#[must_use]
pub(crate) fn to_extracted_diagrams(blocks: &[ExtractedCodeBlock]) -> Vec<ExtractedDiagram> {
    blocks.iter().filter_map(to_extracted_diagram).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render markdown through a [`DiagramProcessor`] pointed at an
    /// unreachable Kroki (connecting to a closed loopback port fails fast in
    /// both sandboxed and unsandboxed runs). Diagram renders fail, but the
    /// resulting error `<figure>`s still carry `data-diagram-id`, which is
    /// all these id-focused tests need.
    fn render_diagrams(markdown: &str) -> String {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        let processor = DiagramProcessor::new("http://127.0.0.1:1");
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(markdown, Pipeline::new().with_processor(processor));
        result.html
    }

    /// Same as [`render_diagrams`] but returns the collected warnings.
    fn render_diagrams_warnings(markdown: &str) -> Vec<String> {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        let processor = DiagramProcessor::new("http://127.0.0.1:1");
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(markdown, Pipeline::new().with_processor(processor));
        result.warnings
    }

    /// Same as [`render_diagrams`] but in `DiagramOutput::Files` mode, to
    /// confirm the Files/Confluence output path never emits `data-diagram-id`.
    fn render_diagrams_files(markdown: &str) -> String {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        let tag_generator: TagGenerator = Arc::new(|info: &RenderedDiagramInfo, _dpi: u32| {
            format!(r#"<img src="{}" alt="diagram">"#, info.filename())
        });
        let processor = DiagramProcessor::new("http://127.0.0.1:1").output(DiagramOutput::Files {
            output_dir: std::env::temp_dir(),
            tag_generator,
        });
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(markdown, Pipeline::new().with_processor(processor));
        result.html
    }

    #[test]
    fn explicit_id_emitted_on_figure() {
        let html = render_diagrams("```mermaid {#architecture}\nA-->B\n```\n");
        assert!(html.contains(r#"data-diagram-id="architecture""#), "{html}");
    }

    #[test]
    fn auto_id_when_unset() {
        let html = render_diagrams("```mermaid\nA-->B\n```\n");
        assert!(html.contains(r#"data-diagram-id="diagram-0""#), "{html}");
    }

    #[test]
    fn two_unannotated_diagrams_get_sequential_ids() {
        let html = render_diagrams("```mermaid\nA-->B\n```\n\n```mermaid\nC-->D\n```\n");
        assert!(html.contains(r#"data-diagram-id="diagram-0""#), "{html}");
        assert!(html.contains(r#"data-diagram-id="diagram-1""#), "{html}");
    }

    #[test]
    fn explicit_id_html_escaped() {
        let html = render_diagrams("```mermaid {#a\"b&c}\nA-->B\n```\n");
        assert!(
            html.contains(r#"data-diagram-id="a&quot;b&amp;c""#),
            "{html}"
        );
    }

    #[test]
    fn bare_id_is_ignored_and_gets_auto_id() {
        let markdown = "```mermaid id=foo\nA-->B\n```\n";
        let html = render_diagrams(markdown);
        assert!(!html.contains(r#"data-diagram-id="foo""#), "{html}");
        assert!(html.contains(r#"data-diagram-id="diagram-0""#), "{html}");
        // A bare token outside the braces is dropped by the parser, so it never
        // reaches the kroki attrs map and raises no "unknown attribute" warning.
        let warnings = render_diagrams_warnings(markdown);
        assert!(
            !warnings.iter().any(|w| w.contains("unknown attribute")),
            "{warnings:?}"
        );
    }

    #[test]
    fn duplicate_ids_warn() {
        let warnings = render_diagrams_warnings(
            "```mermaid {#dup}\nA-->B\n```\n\n```mermaid {#dup}\nC-->D\n```\n",
        );
        assert!(
            warnings.iter().any(|w| w.contains("duplicate id 'dup'")),
            "{warnings:?}"
        );
    }

    #[test]
    fn files_path_emits_no_diagram_id() {
        let html = render_diagrams_files("```mermaid {#x}\nA-->B\n```\n");
        assert!(!html.contains("data-diagram-id"), "{html}");
    }

    /// Render with a cache that hits on every lookup, returning `cached` as the
    /// stored content. This drives the *success* figure branch (cache hit) —
    /// which `render_diagrams` never reaches, since its Kroki is unreachable and
    /// only the error figure is produced.
    fn render_diagrams_cached(markdown: &str, cached: &str) -> String {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        struct AlwaysHit(Vec<u8>);
        impl CacheBucket for AlwaysHit {
            fn get(&self, _key: &str, _etag: &str) -> Option<Vec<u8>> {
                Some(self.0.clone())
            }
            fn set(&self, _key: &str, _etag: &str, _value: &[u8]) {}
        }

        let processor = DiagramProcessor::new("http://127.0.0.1:1")
            .with_cache(Box::new(AlwaysHit(cached.as_bytes().to_vec())));
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(markdown, Pipeline::new().with_processor(processor));
        result.html
    }

    #[test]
    fn collects_referenced_section_refs_from_diagram_links() {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
        use rw_sections::{Namespace, Section, Sections};
        use std::collections::{BTreeSet, HashMap};

        struct AlwaysHit(Vec<u8>);
        impl CacheBucket for AlwaysHit {
            fn get(&self, _key: &str, _etag: &str) -> Option<Vec<u8>> {
                Some(self.0.clone())
            }
            fn set(&self, _key: &str, _etag: &str, _value: &[u8]) {}
        }

        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));

        // The cached SVG carries an internal link into the billing section, so
        // annotation resolves it and the processor must report the ref.
        let cached_svg = r#"<svg id="real"><a href="/domains/billing/api">API</a></svg>"#;
        let processor = DiagramProcessor::new("http://127.0.0.1:1")
            .with_cache(Box::new(AlwaysHit(cached_svg.as_bytes().to_vec())))
            .with_sections(sections);

        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "```mermaid\nA-->B\n```\n",
            Pipeline::new().with_processor(processor),
        );

        // The input registers exactly one section and one diagram link, so the
        // set is fully determined — an exact match also catches a stale ref
        // leaking in if the accumulator were not cleared per render.
        assert_eq!(
            result.section_refs,
            BTreeSet::from(["domain:default/billing".to_owned()])
        );
    }

    #[test]
    fn referenced_section_refs_union_prose_and_diagram() {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
        use rw_sections::{Namespace, Section, Sections};
        use std::collections::{BTreeSet, HashMap};

        struct AlwaysHit(Vec<u8>);
        impl CacheBucket for AlwaysHit {
            fn get(&self, _key: &str, _etag: &str) -> Option<Vec<u8>> {
                Some(self.0.clone())
            }
            fn set(&self, _key: &str, _etag: &str, _value: &[u8]) {}
        }

        let sections = Arc::new(Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    namespace: Namespace::default(),
                    name: "pay".to_owned(),
                },
            ),
        ])));

        // Prose link resolves to system:default/pay; the diagram's $link
        // resolves to domain:default/billing. Walker::finish must union both.
        let cached_svg = r#"<svg id="real"><a href="/domains/billing/api">API</a></svg>"#;
        let processor = DiagramProcessor::new("http://127.0.0.1:1")
            .with_cache(Box::new(AlwaysHit(cached_svg.as_bytes().to_vec())))
            .with_sections(Arc::clone(&sections));

        let md = "[pay](/systems/pay)\n\n```mermaid\nA-->B\n```\n";
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_sections(sections)
            .render(md, Pipeline::new().with_processor(processor));

        assert_eq!(
            result.section_refs,
            BTreeSet::from([
                "domain:default/billing".to_owned(),
                "system:default/pay".to_owned(),
            ])
        );
    }

    #[test]
    fn png_diagrams_collect_no_section_refs() {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
        use rw_sections::{Namespace, Section, Sections};
        use std::collections::HashMap;

        struct AlwaysHit(Vec<u8>);
        impl CacheBucket for AlwaysHit {
            fn get(&self, _key: &str, _etag: &str) -> Option<Vec<u8>> {
                Some(self.0.clone())
            }
            fn set(&self, _key: &str, _etag: &str, _value: &[u8]) {}
        }

        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));

        // PNG diagrams render to an <img>, never an SVG with <a> links, so the
        // PNG path never calls annotate_svg_links — no refs are collected even
        // with a section registered.
        let processor = DiagramProcessor::new("http://127.0.0.1:1")
            .with_cache(Box::new(AlwaysHit(b"data:image/png;base64,ABC".to_vec())))
            .with_sections(sections);

        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "```mermaid {format=png}\nA-->B\n```\n",
            Pipeline::new().with_processor(processor),
        );

        assert!(result.section_refs.is_empty());
    }

    #[test]
    fn id_emitted_on_success_svg_figure() {
        let html = render_diagrams_cached(
            "```mermaid {#architecture}\nA-->B\n```\n",
            "<svg id=\"real\"></svg>",
        );
        // The id lands on a real (non-error) success figure, not the error path.
        assert!(!html.contains("diagram-error"), "{html}");
        assert!(
            html.contains(
                r#"<figure class="diagram" data-diagram-id="architecture"><rw-diagram><svg id="real"></svg></rw-diagram></figure>"#
            ),
            "{html}"
        );
    }

    #[test]
    fn id_emitted_on_success_png_figure() {
        let html = render_diagrams_cached(
            "```mermaid {#pic format=png}\nA-->B\n```\n",
            "data:image/png;base64,ABC",
        );
        assert!(!html.contains("diagram-error"), "{html}");
        // PNG figures hold an <img> with no ids to collide, so they are not
        // wrapped in <rw-diagram> — only the inline SVG path is.
        assert!(!html.contains("rw-diagram"), "{html}");
        assert!(
            html.contains(
                r#"<figure class="diagram" data-diagram-id="pic"><img src="data:image/png;base64,ABC" alt="diagram"></figure>"#
            ),
            "{html}"
        );
    }

    #[test]
    fn empty_brace_id_falls_back_to_auto_id() {
        // `{#}` yields no explicit id, so the auto fallback applies.
        let html = render_diagrams("```mermaid {#}\nA-->B\n```\n");
        assert!(html.contains(r#"data-diagram-id="diagram-0""#), "{html}");
    }

    #[test]
    fn explicit_id_colliding_with_auto_id_warns() {
        // The second diagram explicitly claims the first diagram's auto id.
        let warnings = render_diagrams_warnings(
            "```mermaid\nA-->B\n```\n\n```mermaid {#diagram-0}\nC-->D\n```\n",
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("duplicate id 'diagram-0'")),
            "{warnings:?}"
        );
    }

    #[test]
    fn diagram_id_attr_formats_and_escapes() {
        assert_eq!(
            diagram_id_attr(Some("architecture")),
            r#" data-diagram-id="architecture""#
        );
        assert_eq!(
            diagram_id_attr(Some("a\"b&c")),
            r#" data-diagram-id="a&quot;b&amp;c""#
        );
        assert_eq!(diagram_id_attr(None), "");
    }

    /// The populated-`id_attr` case is pinned end-to-end by
    /// `id_emitted_on_success_svg_figure`; this covers the empty case, which is
    /// reachable (a fence with no `{#id}`) and asserted nowhere else.
    #[test]
    fn svg_figure_without_id_attr_still_wraps() {
        let html = DiagramProcessor::svg_figure("", "<svg/>");
        assert_eq!(
            html,
            "<figure class=\"diagram\"><rw-diagram><svg/></rw-diagram></figure>"
        );
    }

    #[test]
    fn has_transient_error_false_when_no_diagrams() {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        let processor = DiagramProcessor::new("http://kroki.invalid");
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "# Title\n\nJust prose, no diagrams.\n",
            Pipeline::new().with_processor(processor),
        );

        assert!(!result.has_transient_error);
    }

    #[test]
    fn has_transient_error_true_on_unreachable_kroki() {
        use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

        // Unreachable Kroki → the diagram render fails with a transient
        // HttpRequest error, which must surface on the RenderResult (true) so
        // the page renderer declines to cache the error figure. Connecting to a
        // closed loopback port fails fast in both sandboxed and unsandboxed runs.
        let processor = DiagramProcessor::new("http://127.0.0.1:1");
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "# Title\n\n```plantuml\n@startuml\nA -> B\n@enduml\n```\n",
            Pipeline::new().with_processor(processor),
        );

        assert!(result.has_transient_error);
        assert!(result.html.contains("diagram-error"));
        // Error figures hold a <pre>, so they are not wrapped in <rw-diagram> —
        // only the inline SVG path is.
        assert!(!result.html.contains("rw-diagram"));
    }

    #[test]
    fn add_errors_reports_transient_only_for_transient_kinds() {
        use crate::kroki::DiagramErrorKind;

        // A deterministic error (Kroki 400 on malformed source) is recorded as a
        // figure but is NOT transient, so the page still caches.
        let mut det = Figures::new();
        let det_transient = det.add_errors(vec![DiagramError {
            index: 0,
            kind: DiagramErrorKind::HttpResponse {
                status: 400,
                body: String::new(),
            },
        }]);
        assert!(!det_transient);

        // A transient error (Kroki 503) is recorded AND flagged transient.
        let mut tr = Figures::new();
        let tr_transient = tr.add_errors(vec![DiagramError {
            index: 0,
            kind: DiagramErrorKind::HttpResponse {
                status: 503,
                body: String::new(),
            },
        }]);
        assert!(tr_transient);

        // No errors → not transient.
        let mut empty = Figures::new();
        assert!(!empty.add_errors(vec![]));
    }

    #[test]
    fn test_process_plantuml() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();
        let source = "@startuml\nA -> B\n@enduml";

        let result = processor.process("plantuml", &attrs, source, 0);

        assert_eq!(result, ProcessResult::Deferred);
        assert_eq!(processor.extracted().len(), 1);
        assert_eq!(processor.extracted()[0].language, "plantuml");
        assert_eq!(processor.extracted()[0].source, source);
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_mermaid() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();

        let result = processor.process("mermaid", &attrs, "graph TD\n  A --> B", 0);

        assert_eq!(result, ProcessResult::Deferred);
        assert_eq!(processor.extracted()[0].language, "mermaid");
    }

    #[test]
    fn test_process_kroki_prefix() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();

        let result = processor.process("kroki-mermaid", &attrs, "graph TD", 0);

        assert_eq!(result, ProcessResult::Deferred);
        assert_eq!(processor.extracted()[0].language, "kroki-mermaid");
    }

    #[test]
    fn test_process_non_diagram() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);

        assert_eq!(result, ProcessResult::PassThrough);
        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_process_with_format_png() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = FenceAttrs::default();
        attrs.map.insert("format".to_owned(), "png".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs().get("format"),
            Some(&"png".to_owned())
        );
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_with_invalid_format() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = FenceAttrs::default();
        attrs.map.insert("format".to_owned(), "jpeg".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        // Should default to svg
        assert_eq!(
            processor.extracted()[0].attrs().get("format"),
            Some(&"svg".to_owned())
        );
        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown format value 'jpeg'"));
    }

    #[test]
    fn test_process_with_unknown_attribute() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = FenceAttrs::default();
        attrs.map.insert("size".to_owned(), "large".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_process_multiple_diagrams() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();

        processor.process("plantuml", &attrs, "source1", 0);
        processor.process("mermaid", &attrs, "source2", 1);

        assert_eq!(processor.extracted().len(), 2);
        assert_eq!(processor.extracted()[0].index, 0);
        assert_eq!(processor.extracted()[1].index, 1);
    }

    #[test]
    fn test_to_extracted_diagram() {
        let block = ExtractedCodeBlock::new(
            0,
            "plantuml".to_owned(),
            "@startuml\nA -> B\n@enduml".to_owned(),
            None,
            HashMap::from([("format".to_owned(), "png".to_owned())]),
        );

        let diagram = to_extracted_diagram(&block).unwrap();

        assert_eq!(diagram.index, 0);
        assert_eq!(diagram.language, DiagramLanguage::PlantUml);
        assert_eq!(diagram.format, DiagramFormat::Png);
        assert!(diagram.source.contains("A -> B"));
    }

    #[test]
    fn test_to_extracted_diagram_non_diagram() {
        let block = ExtractedCodeBlock::new(
            0,
            "rust".to_owned(),
            "fn main() {}".to_owned(),
            None,
            HashMap::new(),
        );

        assert!(to_extracted_diagram(&block).is_none());
    }

    #[test]
    fn test_to_extracted_diagrams() {
        let blocks = vec![
            ExtractedCodeBlock::new(
                0,
                "plantuml".to_owned(),
                "source1".to_owned(),
                None,
                HashMap::new(),
            ),
            ExtractedCodeBlock::new(
                1,
                "rust".to_owned(), // Not a diagram
                "source2".to_owned(),
                None,
                HashMap::new(),
            ),
            ExtractedCodeBlock::new(
                2,
                "mermaid".to_owned(),
                "source3".to_owned(),
                None,
                HashMap::new(),
            ),
        ];

        let diagrams = to_extracted_diagrams(&blocks);

        assert_eq!(diagrams.len(), 2);
        assert_eq!(diagrams[0].index, 0);
        assert_eq!(diagrams[0].language, DiagramLanguage::PlantUml);
        assert_eq!(diagrams[1].index, 2);
        assert_eq!(diagrams[1].language, DiagramLanguage::Mermaid);
    }

    #[test]
    fn test_stores_endpoint_in_attrs() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = FenceAttrs::default();

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs().get("endpoint"),
            Some(&"plantuml".to_owned())
        );
    }

    #[test]
    fn test_all_diagram_languages() {
        let languages = [
            "plantuml",
            "c4plantuml",
            "mermaid",
            "graphviz",
            "dot",
            "ditaa",
            "blockdiag",
            "seqdiag",
            "actdiag",
            "nwdiag",
            "packetdiag",
            "rackdiag",
            "erd",
            "nomnoml",
            "svgbob",
            "vega",
            "vegalite",
            "wavedrom",
        ];

        for lang in languages {
            let mut processor = DiagramProcessor::new("https://kroki.io");
            let attrs = FenceAttrs::default();

            let result = processor.process(lang, &attrs, "source", 0);

            assert_eq!(
                result,
                ProcessResult::Deferred,
                "Expected Deferred for language: {lang}"
            );
        }
    }

    // `into_fills` just moves `Figures::map` into `Fills` keyed by code-block
    // index; the *renderer* (not this processor) splices each fill at its
    // pre-reserved offset. So the tests below assert on `Fills::get` instead of
    // HTML substrings; end-to-end placement through a real render (proving a
    // fill lands at its reserved offset, not appended or scanned) is covered by
    // `deferred_code_block_fills_at_its_offset` in `code_block.rs` and by
    // `figures_land_in_source_order` below.

    #[test]
    fn into_fills_ignores_add_order() {
        // Figures are collected in whatever order diagrams finish rendering
        // (cache hits first, then Kroki responses as they arrive) — the key,
        // not insertion order, must determine which fill lands where.
        let mut figures = Figures::new();
        figures.add(2, "C".to_owned());
        figures.add(0, "A".to_owned());
        figures.add(1, "B".to_owned());

        let mut fills = Fills::new();
        figures.into_fills(&mut fills);

        assert_eq!(fills.get(0), Some("A"));
        assert_eq!(fills.get(1), Some("B"));
        assert_eq!(fills.get(2), Some("C"));
    }

    #[test]
    fn into_fills_leaves_a_gap_unfilled() {
        // A gap in a partially-populated `Figures` stays a gap: `into_fills`
        // must never invent an entry for an index it was not given.
        let mut figures = Figures::new();
        figures.add(0, "A".to_owned());
        // No figure added for index 1.

        let mut fills = Fills::new();
        figures.into_fills(&mut fills);

        assert_eq!(fills.get(0), Some("A"));
        assert_eq!(fills.get(1), None);
    }

    #[test]
    fn into_fills_of_empty_figures_sets_nothing() {
        let mut fills = Fills::new();
        Figures::new().into_fills(&mut fills);

        assert_eq!(fills.get(0), None);
    }

    #[test]
    fn figures_land_in_source_order() {
        // End-to-end through the real renderer: two diagrams (unreachable Kroki,
        // so each resolves to a distinguishable error figure) must appear
        // between the same prose that surrounds them in the source, regardless
        // of the order `fills` happens to collect them in.
        let html = render_diagrams(
            "before\n\n```mermaid {#first}\nA-->B\n```\n\nmiddle\n\n```mermaid {#second}\nC-->D\n```\n\nafter\n",
        );

        let before = html
            .find("before")
            .unwrap_or_else(|| panic!("before missing: {html}"));
        let first = html
            .find(r#"data-diagram-id="first""#)
            .unwrap_or_else(|| panic!("first diagram missing: {html}"));
        let middle = html
            .find("middle")
            .unwrap_or_else(|| panic!("middle missing: {html}"));
        let second = html
            .find(r#"data-diagram-id="second""#)
            .unwrap_or_else(|| panic!("second diagram missing: {html}"));
        let after = html
            .find("after")
            .unwrap_or_else(|| panic!("after missing: {html}"));

        assert!(
            before < first && first < middle && middle < second && second < after,
            "figures out of source order: {html}"
        );
    }

    #[test]
    fn test_bundle_non_plantuml_returns_none() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        assert!(processor.bundle("mermaid", "graph TD\nA --> B").is_none());
        assert!(processor.bundle("rust", "fn main() {}").is_none());
        assert!(processor.bundle("graphviz", "digraph {}").is_none());
    }

    #[test]
    fn test_bundle_plantuml_without_includes_returns_none() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = processor.bundle("plantuml", source);
        assert!(result.is_none());
    }

    #[test]
    fn test_bundle_c4plantuml_without_includes_returns_none() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let source = "@startuml\nPerson(user, \"User\")\n@enduml";
        let result = processor.bundle("c4plantuml", source);
        assert!(result.is_none());
    }

    #[test]
    fn test_bundle_resolves_filesystem_include() {
        let temp_dir = std::env::temp_dir();
        let include_file = temp_dir.join("bundle_test.iuml");
        std::fs::write(&include_file, "Bob -> Charlie").unwrap();

        let mut processor =
            DiagramProcessor::new("https://kroki.io").include_dirs(std::slice::from_ref(&temp_dir));
        let source = "@startuml\n!include bundle_test.iuml\n@enduml";
        let result = processor.bundle("plantuml", source).unwrap();

        std::fs::remove_file(&include_file).unwrap();

        assert!(result.contains("Bob -> Charlie"));
        assert!(!result.contains("!include"));
    }

    #[test]
    fn test_timeout_builder() {
        // Test that timeout builder method works and can be chained
        let processor = DiagramProcessor::new("https://kroki.io")
            .timeout(Duration::from_mins(1))
            .dpi(96);

        // Verify the processor was created successfully and can process diagrams
        assert!(processor.extracted().is_empty());
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_timeout_builder_short_timeout() {
        // Test with a very short timeout
        let processor =
            DiagramProcessor::new("https://kroki.io").timeout(Duration::from_millis(100));

        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_bundle_preserves_meta_includes() {
        use std::sync::Arc;

        use crate::meta_includes::{EntityInfo, MetaIncludeSource};

        struct TestMetaSource;
        impl MetaIncludeSource for TestMetaSource {
            fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo> {
                if entity_type == "system" && name == "payment_gateway" {
                    Some(EntityInfo {
                        title: "Payment Gateway".to_owned(),
                        description: Some("Processes payments".to_owned()),
                        url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
                    })
                } else {
                    None
                }
            }
        }

        let mut processor = DiagramProcessor::new("https://kroki.io")
            .with_meta_include_source(Arc::new(TestMetaSource));
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let result = processor.bundle("plantuml", source);
        // Meta includes should NOT be resolved at bundle time
        assert!(
            result.is_none(),
            "bundle() should not resolve meta includes"
        );
    }
}
