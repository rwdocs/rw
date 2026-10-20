//! Kroki as a diagram provider.
//!
//! [`KrokiProvider`] turns diagram source into rendered content: SVG text or
//! PNG bytes, a display size, the digest that content is cached under, and any
//! warnings the render produced. It emits no markup and writes no output
//! artifact — it populates its own render cache, but never the file a
//! consumer's markup references — so the same resolution can be inlined into a
//! page or uploaded as an attachment.
//!
//! # Example
//!
//! ```
//! use rw_diagrams::DiagramProvider;
//! use rw_kroki::KrokiProvider;
//!
//! let provider = KrokiProvider::new("https://kroki.io");
//! assert!(provider.handles("kroki-mermaid"));
//! assert!(!provider.handles("rust"));
//! ```

use std::path::PathBuf;
use std::time::Duration;

use rw_cache::{Cache, CacheBucket, CacheBucketExt, NullCache};
use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, ResolveContext, Resolved,
    Size,
};
use rw_error::error_chain;
use rw_plantuml::prepare_diagram_source;
use ureq::Agent;

use crate::cache::diagram_digest;
use crate::consts::STANDARD_DPI;
use crate::kroki::{DiagramErrorKind, KrokiJob, create_agent, get_png_dimensions, render_all};
use crate::language::{DiagramFormat, DiagramLanguage};
use crate::svg::{scale_svg_dimensions, strip_google_fonts_import};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Renders diagrams through a Kroki server.
///
/// Handles every fence language `DiagramLanguage` knows — `plantuml`,
/// `mermaid`, `graphviz` and the rest of that list, each also accepted with a
/// `kroki-` prefix. Kroki itself serves more than that list does, so a fence
/// this provider declines may still be one Kroki could have rendered. A
/// `PlantUML`-family fence has its `!include` directives resolved and its
/// render configuration injected first; every other language is sent as the
/// author wrote it.
///
/// Renders go through the configured cache, keyed by a hash of what determines
/// the bytes: the prepared source, the Kroki endpoint, the output format and
/// the render DPI. That hash is also the [`digest`](Resolved::digest) each
/// resolution carries.
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
///
/// use rw_cache::{Cache, NullCache};
/// use rw_kroki::KrokiProvider;
///
/// let provider = KrokiProvider::new("https://kroki.io")
///     .include_dirs(&[PathBuf::from("docs")])
///     .with_cache(NullCache.bucket("diagrams"));
/// ```
pub struct KrokiProvider {
    /// Kroki server URL renders are posted to, without a trailing `/`.
    kroki_url: String,
    /// Directories searched for `PlantUML` `!include` files.
    include_dirs: Vec<PathBuf>,
    /// Where renders are stored and looked up.
    cache: Box<dyn CacheBucket>,
    /// HTTP agent, reused across renders for connection pooling.
    agent: Agent,
}

impl KrokiProvider {
    /// A provider that renders through the Kroki server at `kroki_url`.
    ///
    /// Starts with no include directories, no caching, and the default
    /// request timeout.
    #[must_use]
    pub fn new(kroki_url: impl Into<String>) -> Self {
        Self {
            kroki_url: kroki_url.into().trim_end_matches('/').to_owned(),
            include_dirs: Vec::new(),
            cache: NullCache.bucket("diagrams"),
            agent: create_agent(DEFAULT_TIMEOUT),
        }
    }

    /// Search `dirs`, in order, for `PlantUML` `!include` files.
    #[must_use]
    pub fn include_dirs(mut self, dirs: &[PathBuf]) -> Self {
        self.include_dirs = dirs.to_vec();
        self
    }

    /// Store renders in `cache` and look them up there before rendering.
    #[must_use]
    pub fn with_cache(mut self, cache: Box<dyn CacheBucket>) -> Self {
        self.cache = cache;
        self
    }

    /// Store a fresh render and turn it into a resolution.
    fn finish(
        &self,
        language: DiagramLanguage,
        miss: Miss,
        rendered: Result<DiagramContent, DiagramErrorKind>,
    ) -> Result<Resolved, DiagramError> {
        let content = match rendered {
            Ok(DiagramContent::Svg(svg)) => {
                // Content manipulation, not markup: the external font request
                // is dropped and the oversampled dimensions are divided back
                // down, so what a consumer embeds is already display-sized.
                let stripped = strip_google_fonts_import(svg.trim());
                let scaled = scale_svg_dimensions(&stripped, language.render_dpi());
                DiagramContent::Svg(scaled)
            }
            Ok(png @ DiagramContent::Png(_)) => png,
            Err(kind) => {
                return Err(DiagramError {
                    message: error_chain(&kind),
                    transient: kind.is_transient(),
                });
            }
        };
        self.store(&miss.hash, &content);
        Ok(resolution(content, language, miss))
    }

    /// The render cached under `digest`, if any.
    fn load(&self, digest: &str, format: DiagramFormat) -> Option<DiagramContent> {
        match format {
            DiagramFormat::Svg => self.cache.get_string(digest, "").map(DiagramContent::Svg),
            DiagramFormat::Png => self
                .cache
                .get(&png_cache_key(digest), "")
                .map(DiagramContent::Png),
        }
    }

    fn store(&self, digest: &str, content: &DiagramContent) {
        match content {
            DiagramContent::Svg(svg) => self.cache.set_string(digest, "", svg),
            DiagramContent::Png(bytes) => self.cache.set(&png_cache_key(digest), "", bytes),
        }
    }
}

/// The cache key a PNG render is stored under.
///
/// Not the bare digest: an S3 cache is never wiped and may still hold base64
/// data URIs there, which must not be read back as image bytes. Nor a different
/// digest: that names Confluence attachments.
fn png_cache_key(digest: &str) -> String {
    format!("png/{digest}")
}

impl DiagramProvider for KrokiProvider {
    fn handles(&self, language: &str) -> bool {
        language.parse::<DiagramLanguage>().is_ok()
    }

    fn resolve(
        &self,
        requests: &[&DiagramRequest],
        ctx: &ResolveContext<'_>,
    ) -> Vec<Result<Resolved, DiagramError>> {
        let mut steps = Vec::with_capacity(requests.len());
        let mut jobs = Vec::new();

        for request in requests {
            let Ok(language) = request.language.parse::<DiagramLanguage>() else {
                steps.push(Step::Answered(Err(DiagramError {
                    message: format!(
                        "kroki renders no language named '{}'; routing sent this fence to the wrong provider",
                        request.language
                    ),
                    transient: false,
                })));
                continue;
            };

            let mut warnings = Vec::new();
            let format = read_format(&request.attrs, &mut warnings);

            // Asked of the parsed language, not the fence string: the two
            // agree for every spelling that parses today, and asking the
            // language keeps them agreeing if an alias is ever added.
            let source = if language.needs_plantuml_preprocessing() {
                let prepared =
                    prepare_diagram_source(&request.source, &self.include_dirs, ctx.model);
                warnings.extend(prepared.warnings);
                if let Some(record) = ctx.record_read {
                    prepared.reads.into_iter().for_each(record);
                }
                prepared.source
            } else {
                request.source.clone()
            };

            let hash = diagram_digest(&source, language, format);
            let miss = Miss { hash, warnings };

            if let Some(content) = self.load(&miss.hash, format) {
                steps.push(Step::Answered(Ok(resolution(content, language, miss))));
                continue;
            }

            jobs.push(KrokiJob {
                source,
                language,
                format,
            });
            steps.push(Step::Render(miss));
        }

        let mut rendered = jobs
            .iter()
            .zip(render_all(&jobs, &self.kroki_url, &self.agent));
        steps
            .into_iter()
            // `render_all` answers every job, so this never stops early; if it
            // did, `Providers` reports each request left without a result.
            .map_while(|step| match step {
                Step::Answered(result) => Some(result),
                Step::Render(miss) => rendered
                    .next()
                    .map(|(job, result)| self.finish(job.language, miss, result)),
            })
            .collect()
    }
}

/// Where a request stands once the cache has been consulted.
enum Step {
    Answered(Result<Resolved, DiagramError>),
    /// Waiting on the next job's render.
    Render(Miss),
}

/// What a request that has to be rendered still needs afterwards.
struct Miss {
    /// The digest this render's resolution carries; its cache key derives from it.
    hash: String,
    /// Warnings raised while reading attributes and preparing the source.
    warnings: Vec<String>,
}

/// The output format the fence asked for, warning about anything unrecognised.
///
/// An unknown value falls back to the default rather than failing the fence, so
/// a typo costs a warning and a diagram in the wrong format, not a broken page.
///
/// The loop keeps assigning rather than stopping at the first `format`, which is
/// [`DiagramRequest::attrs`]' last-occurrence-wins rule: a caller appends a
/// `format` to override the author's while leaving theirs in the list to be
/// diagnosed, and both are warned about here in the order they were written.
///
/// [`DiagramRequest::attrs`]: rw_diagrams::DiagramRequest::attrs
fn read_format(attrs: &[(String, String)], warnings: &mut Vec<String>) -> DiagramFormat {
    let mut format = DiagramFormat::default();
    for (key, value) in attrs {
        if key == "format" {
            format = value.parse().unwrap_or_else(|()| {
                warnings.push(format!(
                    "unknown format value '{value}', using default 'svg' (valid: svg, png)"
                ));
                DiagramFormat::default()
            });
        } else {
            warnings.push(format!("unknown attribute '{key}' ignored (valid: format)"));
        }
    }
    format
}

/// Turn content — a fresh render or a cache hit — into a resolution.
///
/// A stored SVG is already post-processed, so both sources agree on what the
/// content means.
fn resolution(content: DiagramContent, language: DiagramLanguage, miss: Miss) -> Resolved {
    let size = match &content {
        // An SVG carries its own width and height, so there is nothing left
        // for a consumer to size it by.
        DiagramContent::Svg(_) => None,
        // Absent for a PNG whose header will not parse, which is the only
        // case there is no size to report.
        DiagramContent::Png(bytes) => get_png_dimensions(bytes).map(|(width, height)| {
            let dpi = language.render_dpi();
            Size {
                width: to_display_px(width, dpi),
                height: to_display_px(height, dpi),
            }
        }),
    };
    Resolved {
        asset: Asset::Inline(content),
        size,
        digest: miss.hash,
        warnings: miss.warnings,
    }
}

/// Scale a whole-pixel dimension rendered at `dpi` down to its display size.
///
/// Rounds down, but never to zero: a dimension that survives rendering is worth
/// at least one pixel, and `width="0"` would hide the diagram entirely.
fn to_display_px(value: u32, dpi: u32) -> u32 {
    if value == 0 {
        return 0;
    }
    let scaled = u64::from(value) * u64::from(STANDARD_DPI) / u64::from(dpi);
    u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use parking_lot::Mutex;
    use rw_cache::{CacheBucket, CacheBucketExt};
    use rw_diagrams::{
        Asset, DiagramContent, DiagramProvider, DiagramRequest, Entity, FileStamp, RequestKey,
        ResolveContext, Resolved, SiteModel, Size,
    };

    use super::{
        DEFAULT_TIMEOUT, KrokiProvider, STANDARD_DPI, create_agent, png_cache_key,
        prepare_diagram_source, to_display_px,
    };
    use crate::cache::diagram_digest;
    use crate::language::{DiagramFormat, DiagramLanguage};
    use crate::test_support::{KrokiStub, PNG_HEADER, Reply};

    /// Nothing listens here, so a test pointed at it fails loudly instead of
    /// quietly succeeding against whatever Kroki the developer has running.
    const DEAD_KROKI: &str = "http://127.0.0.1:1";

    // ── A cache a test can look inside ────────────────────────────────────

    /// An in-memory [`CacheBucket`], for inspectability: cloning shares the
    /// storage, so a test reads back exactly what the provider stored, with no
    /// directory or version file in between. `NullCache` cannot exercise a hit
    /// at all, since it always misses.
    #[derive(Clone, Default)]
    struct MemoryCache {
        entries: Arc<Mutex<HashMap<String, Entry>>>,
    }

    /// One stored render, with the etag it was written under.
    struct Entry {
        etag: String,
        value: Vec<u8>,
    }

    impl CacheBucket for MemoryCache {
        fn get(&self, key: &str, etag: &str) -> Option<Vec<u8>> {
            let entries = self.entries.lock();
            let entry = entries.get(key)?;
            (entry.etag == etag).then(|| entry.value.clone())
        }

        fn set(&self, key: &str, etag: &str, value: &[u8]) {
            self.entries.lock().insert(
                key.to_owned(),
                Entry {
                    etag: etag.to_owned(),
                    value: value.to_vec(),
                },
            );
        }
    }

    // ── Fixtures ──────────────────────────────────────────────────────────

    /// The 400x200 PNG plus `marker`.
    fn png_bytes(marker: &[u8]) -> Vec<u8> {
        [PNG_HEADER, marker].concat()
    }

    struct OneSystem;

    impl SiteModel for OneSystem {
        fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
            (kind == "system" && name == "payment-gateway").then(|| Entity {
                title: "Payment Gateway".to_owned(),
                description: None,
                url_path: None,
            })
        }
    }

    fn request(key: u32, language: &str, source: &str) -> DiagramRequest {
        DiagramRequest {
            key: RequestKey::from(key),
            language: language.to_owned(),
            source: source.to_owned(),
            attrs: Vec::new(),
            id: None,
        }
    }

    fn with_attr(mut request: DiagramRequest, key: &str, value: &str) -> DiagramRequest {
        request.attrs.push((key.to_owned(), value.to_owned()));
        request
    }

    fn svg_of(resolved: &Resolved) -> &str {
        match &resolved.asset {
            Asset::Inline(DiagramContent::Svg(svg)) => svg,
            other => panic!("expected inline SVG, got {other:?}"),
        }
    }

    fn png_of(resolved: &Resolved) -> &[u8] {
        match &resolved.asset {
            Asset::Inline(DiagramContent::Png(bytes)) => bytes,
            other => panic!("expected inline PNG, got {other:?}"),
        }
    }

    /// The cache key and digest one render is expected to carry.
    ///
    /// Derived through the same call the provider makes, deliberately: what the
    /// tests below discriminate is *routing* — which entry a request reads,
    /// which digest it comes back with, whether two requests land on the same
    /// key — and mirroring the formula keeps them doing that when it changes.
    ///
    /// It follows that none of them pins the hash's *value*. Two hardcoded
    /// anchors do that, and they are the only two: `cache.rs`'s
    /// `png_attachment_hash_is_stable_for_a_known_source`, and the literal
    /// `diagram_<hash>.png` filenames in `rw-confluence`'s
    /// `render_names_attachments_from_the_prepared_source_digest`. Delete both
    /// and the formula becomes free to change silently, renaming every
    /// published Confluence attachment.
    fn hash_of(language: DiagramLanguage, source: &str, format: DiagramFormat) -> String {
        diagram_digest(source, language, format)
    }

    fn with_timeout(provider: KrokiProvider, timeout: Duration) -> KrokiProvider {
        KrokiProvider {
            agent: create_agent(timeout),
            ..provider
        }
    }

    // ── handles() ─────────────────────────────────────────────────────────

    /// Every spelling is pinned in `language.rs`; this only checks that
    /// `handles` asks the same parser.
    #[test]
    fn handles_the_languages_diagramlanguage_parses() {
        let provider = KrokiProvider::new(DEAD_KROKI);
        for language in ["plantuml", "kroki-mermaid", "dot", "structurizr"] {
            assert!(provider.handles(language), "should handle '{language}'");
        }
    }

    #[test]
    fn does_not_handle_prose_fences() {
        let provider = KrokiProvider::new(DEAD_KROKI);
        // A provider that claimed everything would take `rust` away from
        // syntax highlighting and render it as a diagram error.
        for language in ["rust", "text", "", "puml", "kroki-unknown"] {
            assert!(
                !provider.handles(language),
                "should not handle '{language}'"
            );
        }
    }

    // ── the format attribute ──────────────────────────────────────────────

    #[test]
    fn an_unknown_format_value_warns_and_falls_back_to_svg() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&with_attr(request(0, "mermaid", source), "format", "jpeg")],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("the fence still renders");
        // Reading the SVG entry is the evidence of the fallback: a provider
        // that kept 'jpeg' would have keyed on it and missed the cache.
        assert_eq!(svg_of(resolved), "<svg>cached</svg>");
        assert_eq!(
            resolved.warnings,
            ["unknown format value 'jpeg', using default 'svg' (valid: svg, png)"],
        );
    }

    #[test]
    fn an_unknown_attribute_warns_and_is_otherwise_ignored() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[
                &request(0, "mermaid", source),
                &with_attr(request(1, "mermaid", source), "width", "100"),
            ],
            &ResolveContext::default(),
        );

        let plain = results[0].as_ref().expect("resolved");
        let annotated = results[1].as_ref().expect("resolved");
        assert_eq!(
            annotated.warnings,
            ["unknown attribute 'width' ignored (valid: format)"],
        );
        assert_eq!(plain.warnings, [] as [String; 0]);
        // "Otherwise ignored": everything but the warning is what the same
        // fence without the attribute produced.
        assert_eq!(annotated.asset, plain.asset);
        assert_eq!(annotated.digest, plain.digest);
    }

    #[test]
    fn format_png_is_honoured() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
            "",
            "<svg>cached</svg>",
        );
        cache.set(
            &png_cache_key(&hash_of(
                DiagramLanguage::Mermaid,
                source,
                DiagramFormat::Png,
            )),
            "",
            &png_bytes(b"png-entry"),
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&with_attr(request(0, "mermaid", source), "format", "png")],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert!(
            png_of(resolved).ends_with(b"png-entry"),
            "the PNG entry must be the one read, not the SVG one",
        );
        assert_eq!(resolved.warnings, [] as [String; 0]);
    }

    // ── preprocessing ─────────────────────────────────────────────────────

    #[test]
    fn plantuml_source_is_preprocessed_before_hashing() {
        let source = "@startuml\nA -> B\n@enduml";
        let cache = MemoryCache::default();
        // Both spellings are in the cache, so the test discriminates between
        // them rather than just observing a hit.
        cache.set_string(
            &hash_of(DiagramLanguage::PlantUml, source, DiagramFormat::Svg),
            "",
            "<svg>raw</svg>",
        );
        cache.set_string(
            &hash_of(
                DiagramLanguage::PlantUml,
                &prepare_diagram_source(source, &[], None).source,
                DiagramFormat::Svg,
            ),
            "",
            "<svg>prepared</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(
            svg_of(resolved),
            "<svg>prepared</svg>",
            "the key must be over the prepared source, not the fence body",
        );
    }

    #[test]
    fn a_non_plantuml_language_is_not_preprocessed() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "mermaid", source)],
            &ResolveContext::default(),
        );

        // Injecting `skinparam` into Mermaid would change the key, miss the
        // cache, and send an unrenderable diagram to Kroki.
        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(svg_of(resolved), "<svg>cached</svg>");
        assert_eq!(
            resolved.digest,
            hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
        );
    }

    #[test]
    fn an_unresolved_include_warns_but_still_renders() {
        let source = "@startuml\n!include missing/thing.iuml\n@enduml";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(
                DiagramLanguage::PlantUml,
                &prepare_diagram_source(source, &[], None).source,
                DiagramFormat::Svg,
            ),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0]
            .as_ref()
            .expect("a missing include must not fail the diagram");
        assert_eq!(svg_of(resolved), "<svg>cached</svg>");
        assert!(
            resolved
                .warnings
                .iter()
                .any(|w| w.contains("missing/thing.iuml")),
            "the include should be named: {:?}",
            resolved.warnings,
        );
    }

    /// Includes are reported however the diagram resolves: a page caching its
    /// markup depends on them whether or not Kroki answered.
    #[test]
    fn include_reads_are_reported_on_a_hit_and_a_failed_render_alike() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cached.iuml"), "A -> B").unwrap();
        std::fs::write(dir.path().join("failing.iuml"), "C -> D").unwrap();
        let dirs = [dir.path().to_path_buf()];
        let cached = "@startuml\n!include cached.iuml\n@enduml";
        let failing = "@startuml\n!include failing.iuml\n@enduml";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(
                DiagramLanguage::PlantUml,
                &prepare_diagram_source(cached, &dirs, None).source,
                DiagramFormat::Svg,
            ),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI)
            .include_dirs(&dirs)
            .with_cache(Box::new(cache));
        let reads = Mutex::new(Vec::new());
        let results = provider.resolve(
            &[
                &request(0, "plantuml", cached),
                &request(1, "plantuml", failing),
            ],
            &ResolveContext {
                model: None,
                record_read: Some(&|stamp| reads.lock().push(stamp)),
            },
        );

        assert!(results[0].is_ok() && results[1].is_err(), "{results:?}");
        assert_eq!(
            reads.into_inner(),
            [
                FileStamp::of(dir.path().join("cached.iuml")),
                FileStamp::of(dir.path().join("failing.iuml")),
            ],
        );
    }

    // ── the site model ────────────────────────────────────────────────────

    #[test]
    fn a_meta_include_resolves_from_the_context_model() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let expanded = prepare_diagram_source(source, &[], Some(&OneSystem)).source;
        assert!(
            expanded.contains("System(sys_payment_gateway"),
            "fixture check: the model should expand the include",
        );

        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::PlantUml, &expanded, DiagramFormat::Svg),
            "",
            "<svg>expanded</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext {
                model: Some(&OneSystem),
                record_read: None,
            },
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(svg_of(resolved), "<svg>expanded</svg>");
        assert_eq!(resolved.warnings, [] as [String; 0]);
    }

    #[test]
    fn without_a_model_a_meta_include_is_left_alone() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let unexpanded = prepare_diagram_source(source, &[], None).source;
        assert!(
            unexpanded.contains("!include systems/sys_payment_gateway.iuml"),
            "fixture check: with no model the include stays put",
        );

        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::PlantUml, &unexpanded, DiagramFormat::Svg),
            "",
            "<svg>unexpanded</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(svg_of(resolved), "<svg>unexpanded</svg>");
        assert!(
            resolved.warnings.is_empty(),
            "an include awaiting a model is not a problem to report: {:?}",
            resolved.warnings,
        );
        assert_ne!(
            resolved.digest,
            hash_of(
                DiagramLanguage::PlantUml,
                &prepare_diagram_source(source, &[], Some(&OneSystem)).source,
                DiagramFormat::Svg,
            ),
            "a model that expands the include must change the digest",
        );
    }

    // ── the cache ─────────────────────────────────────────────────────────

    #[test]
    fn a_cache_hit_returns_content_without_touching_the_network() {
        let source = "@startuml\nA -> B\n@enduml";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(
                DiagramLanguage::PlantUml,
                &prepare_diagram_source(source, &[], None).source,
                DiagramFormat::Svg,
            ),
            "",
            "<svg>from the cache</svg>",
        );

        // Nothing listens on the configured port, so content coming back at
        // all is the proof that the cache answered.
        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("the cache answers");
        assert_eq!(svg_of(resolved), "<svg>from the cache</svg>");
    }

    #[test]
    fn a_cache_miss_stores_what_it_rendered() {
        let stub = KrokiStub::start();
        let cache = MemoryCache::default();
        let source = "@startuml\nA -> B\n@enduml";

        let provider = KrokiProvider::new(&stub.url).with_cache(Box::new(cache.clone()));
        let results = provider.resolve(
            &[
                &request(0, "plantuml", source),
                &with_attr(request(1, "plantuml", source), "format", "png"),
            ],
            &ResolveContext::default(),
        );

        let svg = svg_of(results[0].as_ref().expect("rendered"));
        let prepared = prepare_diagram_source(source, &[], None).source;
        assert_eq!(
            cache
                .get_string(
                    &hash_of(DiagramLanguage::PlantUml, &prepared, DiagramFormat::Svg),
                    ""
                )
                .as_deref(),
            Some(svg),
            "the cache must hold exactly the content the resolution returned",
        );
        let png = png_of(results[1].as_ref().expect("rendered"));
        assert_eq!(
            cache
                .get(
                    &png_cache_key(&hash_of(
                        DiagramLanguage::PlantUml,
                        &prepared,
                        DiagramFormat::Png
                    )),
                    ""
                )
                .as_deref(),
            Some(png),
            "the cache must hold exactly the PNG bytes the resolution returned",
        );
        // Rendered in parallel, so they arrive in either order.
        let mut paths = stub.paths();
        paths.sort();
        assert_eq!(paths, ["/plantuml/png", "/plantuml/svg"]);
    }

    #[test]
    fn a_freshly_rendered_svg_is_display_sized_and_font_free() {
        let stub = KrokiStub::start();
        let provider = KrokiProvider::new(&stub.url);
        let results = provider.resolve(
            &[&request(0, "plantuml", "@startuml\nA -> B\n@enduml")],
            &ResolveContext::default(),
        );

        let svg = svg_of(results[0].as_ref().expect("rendered"));
        assert!(
            svg.contains(r#"width="200""#),
            "the 400px render at 192 DPI must be halved for display: {svg}",
        );
        assert!(
            !svg.contains("fonts.googleapis.com"),
            "the Google Fonts import must be stripped, or the page fetches it: {svg}",
        );
    }

    #[test]
    fn the_cache_key_separates_two_sources() {
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, "graph one", DiagramFormat::Svg),
            "",
            "<svg>one</svg>",
        );
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, "graph two", DiagramFormat::Svg),
            "",
            "<svg>two</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[
                &request(0, "mermaid", "graph one"),
                &request(1, "mermaid", "graph two"),
            ],
            &ResolveContext::default(),
        );

        let one = results[0].as_ref().expect("resolved");
        let two = results[1].as_ref().expect("resolved");
        assert_eq!(svg_of(one), "<svg>one</svg>");
        assert_eq!(svg_of(two), "<svg>two</svg>");
        assert_ne!(one.digest, two.digest);
    }

    // ── results ───────────────────────────────────────────────────────────

    /// One combination of the result-shape matrix below.
    struct Cell {
        /// Fence language, which decides preprocessing and render DPI.
        language: &'static str,
        format: DiagramFormat,
        /// Whether the content arrives from a fresh render rather than a hit.
        fresh: bool,
        /// The display size the resolution must carry.
        size: Option<Size>,
    }

    /// Both content sources must agree on the shape of what they resolve to.
    ///
    /// Rather than a test per side per property, this walks
    /// `[hit, render] x [svg, png] x [plantuml, mermaid]` and pins the shape of
    /// `Resolved` — asset variant, display size, digest — for all eight.
    ///
    /// Content differs between the two sources by design (a cache entry is
    /// stored already post-processed), so only shape is asserted here; what the
    /// bytes are is each behaviour's own test.
    #[test]
    fn a_resolution_has_the_same_shape_from_the_cache_as_from_a_render() {
        // 400x200 is what both the stub and the cached fixtures carry, so the
        // expected size isolates the DPI correction: halved for the oversampled
        // PlantUML family, untouched for everything else.
        let plantuml_png = Some(Size {
            width: 200,
            height: 100,
        });
        let mermaid_png = Some(Size {
            width: 400,
            height: 200,
        });
        let cells = [
            Cell {
                language: "plantuml",
                format: DiagramFormat::Svg,
                fresh: false,
                size: None,
            },
            Cell {
                language: "plantuml",
                format: DiagramFormat::Svg,
                fresh: true,
                size: None,
            },
            Cell {
                language: "plantuml",
                format: DiagramFormat::Png,
                fresh: false,
                size: plantuml_png,
            },
            Cell {
                language: "plantuml",
                format: DiagramFormat::Png,
                fresh: true,
                size: plantuml_png,
            },
            Cell {
                language: "mermaid",
                format: DiagramFormat::Svg,
                fresh: false,
                size: None,
            },
            Cell {
                language: "mermaid",
                format: DiagramFormat::Svg,
                fresh: true,
                size: None,
            },
            Cell {
                language: "mermaid",
                format: DiagramFormat::Png,
                fresh: false,
                size: mermaid_png,
            },
            Cell {
                language: "mermaid",
                format: DiagramFormat::Png,
                fresh: true,
                size: mermaid_png,
            },
        ];

        let stub = KrokiStub::start();
        for cell in cells {
            assert_shape(&cell, &stub);
        }
    }

    /// Resolve one matrix cell and check the shape of what came back, naming
    /// the combination in every failure.
    fn assert_shape(cell: &Cell, stub: &KrokiStub) {
        let source = "A -> B";
        let origin = if cell.fresh { "a render" } else { "the cache" };
        let which = format!("{} {:?} from {origin}", cell.language, cell.format);

        let language: DiagramLanguage = cell.language.parse().expect("a known language");
        let rendered_source = if language.needs_plantuml_preprocessing() {
            prepare_diagram_source(source, &[], None).source
        } else {
            source.to_owned()
        };
        let hash = hash_of(language, &rendered_source, cell.format);

        let cache = MemoryCache::default();
        let provider = if cell.fresh {
            KrokiProvider::new(&stub.url).with_cache(Box::new(cache))
        } else {
            match cell.format {
                DiagramFormat::Svg => {
                    cache.set_string(&hash, "", r#"<svg width="200" height="100"></svg>"#);
                }
                DiagramFormat::Png => cache.set(&png_cache_key(&hash), "", &png_bytes(b"")),
            }
            KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache))
        };

        let mut request = request(0, cell.language, source);
        if cell.format == DiagramFormat::Png {
            request = with_attr(request, "format", "png");
        }
        let results = provider.resolve(&[&request], &ResolveContext::default());

        assert_eq!(results.len(), 1, "{which}");
        let resolved = results[0]
            .as_ref()
            .unwrap_or_else(|error| panic!("{which} should resolve, got {error:?}"));

        match (cell.format, &resolved.asset) {
            (DiagramFormat::Svg, Asset::Inline(DiagramContent::Svg(_)))
            | (DiagramFormat::Png, Asset::Inline(DiagramContent::Png(_))) => {}
            (_, other) => panic!("{which} resolved to the wrong asset: {other:?}"),
        }
        if cell.format == DiagramFormat::Svg {
            // Keeps the `size: None` assertion below from going vacuous: it
            // means "the dimensions are on the tag, so there is nothing to
            // report", which says nothing about an SVG that has none.
            let svg = svg_of(resolved);
            assert!(
                svg.contains(r#"width=""#) && svg.contains(r#"height=""#),
                "{which} resolved to an SVG carrying no dimensions: {svg}",
            );
        }
        assert_eq!(resolved.size, cell.size, "{which} reported the wrong size");
        assert_eq!(resolved.digest, hash, "{which} carries the wrong digest");
    }

    #[test]
    fn a_render_gives_up_after_the_configured_timeout() {
        use std::net::TcpListener;
        use std::time::Instant;

        // Never accepted, so the kernel completes the handshake from the
        // backlog and the request then waits for a response that never comes.
        // The only thing that can end it is the timeout.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a silent server");
        let address = listener.local_addr().expect("the silent server's address");

        let provider = with_timeout(
            KrokiProvider::new(format!("http://{address}")),
            Duration::from_millis(50),
        );
        let started = Instant::now();
        let results = provider.resolve(
            &[&request(0, "mermaid", "graph TD")],
            &ResolveContext::default(),
        );
        let elapsed = started.elapsed();

        let error = results[0]
            .as_ref()
            .expect_err("a server that never answers");
        assert!(
            error.transient,
            "a timeout is worth retrying: {}",
            error.message,
        );
        // Two orders of magnitude below the default, so this fails on a
        // provider that ignored the configured timeout without depending on
        // how promptly a loaded machine gets round to the 50ms one.
        assert!(
            elapsed < Duration::from_secs(5),
            "the configured timeout should have ended this, not the {DEFAULT_TIMEOUT:?} default; took {elapsed:?}",
        );
    }

    /// The verdict a caller caches on. `rw-site` re-renders a page whose
    /// diagrams failed transiently and caches one whose diagrams failed for
    /// good, so calling a 400 retryable makes every request re-render the same
    /// broken diagram forever, and calling a 503 deterministic freezes an
    /// outage into the cache.
    ///
    /// Both directions, through a real response: nothing but the status the
    /// server sent decides this.
    #[test]
    fn a_kroki_error_status_decides_whether_the_failure_is_retryable() {
        for (status, transient) in [(400, false), (503, true)] {
            let stub = KrokiStub::answering(Reply::Fixed {
                status,
                body: b"kroki said no",
            });
            let provider = KrokiProvider::new(&stub.url);
            let results = provider.resolve(
                &[&request(0, "mermaid", "graph TD")],
                &ResolveContext::default(),
            );

            let error = results[0]
                .as_ref()
                .expect_err("an error status is not a render");
            assert_eq!(
                error.transient, transient,
                "HTTP {status} was classified wrong: {}",
                error.message,
            );
            // The status and Kroki's own explanation both reach an operator.
            assert!(
                error.message.contains(&format!("HTTP {status}"))
                    && error.message.contains("kroki said no"),
                "HTTP {status} lost its detail: {}",
                error.message,
            );
        }
    }

    /// A 200 whose body is not a PNG. Deterministic, because the same response
    /// decodes the same way every time — and re-rendering it on every request
    /// would hammer Kroki for a diagram it cannot produce.
    #[test]
    fn a_png_response_that_is_not_a_png_fails_deterministically() {
        let stub = KrokiStub::answering(Reply::Fixed {
            status: 200,
            body: b"<html>a proxy error page</html>",
        });
        let provider = KrokiProvider::new(&stub.url);
        let results = provider.resolve(
            &[&with_attr(
                request(0, "mermaid", "graph TD"),
                "format",
                "png",
            )],
            &ResolveContext::default(),
        );

        let error = results[0].as_ref().expect_err("the body is not a PNG");
        assert!(
            error.message.contains("invalid PNG"),
            "the message should say what was wrong with it: {}",
            error.message,
        );
        assert!(!error.transient, "the same response decodes the same way");
    }

    /// The SVG counterpart: a 200 whose bytes are not text at all.
    #[test]
    fn an_svg_response_that_is_not_utf8_fails_deterministically() {
        let stub = KrokiStub::answering(Reply::Fixed {
            status: 200,
            body: &[0xFF, 0xFE, 0x00],
        });
        let provider = KrokiProvider::new(&stub.url);
        let results = provider.resolve(
            &[&request(0, "mermaid", "graph TD")],
            &ResolveContext::default(),
        );

        let error = results[0].as_ref().expect_err("the body is not UTF-8");
        assert!(
            error.message.contains("invalid UTF-8"),
            "the message should say what was wrong with it: {}",
            error.message,
        );
        assert!(!error.transient, "the same response decodes the same way");
    }

    /// Older releases cached a PNG as a base64 data URI under the bare digest,
    /// and an S3 cache is never wiped on upgrade. Read as raw bytes, that text
    /// would reach a page or a Confluence attachment as a broken image.
    #[test]
    fn a_png_entry_cached_as_a_data_uri_is_never_read() {
        let stub = KrokiStub::start();
        let source = "graph TD";
        let hash = hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Png);
        let cache = MemoryCache::default();
        cache.set_string(&hash, "", "data:image/png;base64,iVBORw0KGgo=");

        let provider = KrokiProvider::new(&stub.url).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&with_attr(request(0, "mermaid", source), "format", "png")],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("rendered afresh");
        assert_eq!(png_of(resolved), png_bytes(source.as_bytes()));
        assert_eq!(stub.paths(), ["/mermaid/png"]);
        assert_eq!(
            resolved.digest, hash,
            "the digest names Confluence attachments, so it must not move",
        );
    }

    #[test]
    fn a_png_whose_header_will_not_parse_reports_no_size() {
        let source = "graph TD";
        // There is no header to read dimensions from, and no size to report —
        // the one case a PNG resolution comes back without one.
        let payload = b"not a png, but long enough to have a header";
        let cache = MemoryCache::default();
        cache.set(
            &png_cache_key(&hash_of(
                DiagramLanguage::Mermaid,
                source,
                DiagramFormat::Png,
            )),
            "",
            payload,
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&with_attr(request(0, "mermaid", source), "format", "png")],
            &ResolveContext::default(),
        );

        let resolved = results[0]
            .as_ref()
            .expect("undecodable dimensions are not a failed render");
        assert_eq!(resolved.size, None);
        assert_eq!(
            png_of(resolved),
            payload,
            "the bytes must still reach the caller",
        );
    }

    #[test]
    fn an_include_resolves_from_the_configured_directories() {
        let dir = tempfile::tempdir().expect("a temporary include directory");
        std::fs::write(dir.path().join("parts.iuml"), "Alice -> Bob\n")
            .expect("write the include file");

        let source = "@startuml\n!include parts.iuml\n@enduml";
        let include_dirs = [dir.path().to_path_buf()];
        let expanded = prepare_diagram_source(source, &include_dirs, None).source;
        assert!(
            expanded.contains("Alice -> Bob"),
            "fixture check: the include should be expanded into the source",
        );

        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::PlantUml, &expanded, DiagramFormat::Svg),
            "",
            "<svg>included</svg>",
        );

        // Keyed on the expanded source: a provider that ignored its include
        // directories would key on the unexpanded one and miss.
        let provider = KrokiProvider::new(DEAD_KROKI)
            .include_dirs(&include_dirs)
            .with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(svg_of(resolved), "<svg>included</svg>");
        assert!(
            resolved.warnings.is_empty(),
            "an include that resolved is not worth a warning: {:?}",
            resolved.warnings,
        );
    }

    /// Confluence names an attachment from the first 12 characters of a
    /// resolution's digest, so the digest has to be a filename-safe string of
    /// stable length — whatever it is a hash of.
    ///
    /// Shape only. That the digest is the key the cache was read under is the
    /// routing every `hash_of` test above asserts, and asserting it again here
    /// against a locally computed `hash_of` would say nothing further.
    #[test]
    fn a_digest_is_a_fixed_length_lowercase_hex_string() {
        let source = "@startuml\nA -> B\n@enduml";
        let hash = hash_of(
            DiagramLanguage::PlantUml,
            &prepare_diagram_source(source, &[], None).source,
            DiagramFormat::Svg,
        );
        let cache = MemoryCache::default();
        cache.set_string(&hash, "", "<svg>cached</svg>");

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[&request(0, "plantuml", source)],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(resolved.digest.len(), 64);
        assert!(
            resolved
                .digest
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "the digest must be lowercase hex: {}",
            resolved.digest,
        );
    }

    #[test]
    fn a_freshly_rendered_diagram_keeps_its_warnings() {
        // Every other warning assertion here runs through a cache hit. If
        // warnings were dropped on the render path instead, they would vanish
        // on a page's first render and appear only once it was cached — so an
        // operator on a cold cache would be told nothing at all.
        let stub = KrokiStub::start();
        let provider = KrokiProvider::new(&stub.url);
        let source = "@startuml\n!include missing/thing.iuml\n@enduml";
        let results = provider.resolve(
            &[
                &with_attr(request(0, "plantuml", source), "width", "100"),
                &with_attr(
                    with_attr(request(1, "plantuml", source), "format", "png"),
                    "width",
                    "100",
                ),
            ],
            &ResolveContext::default(),
        );

        for (position, resolved) in results.iter().enumerate() {
            let resolved = resolved.as_ref().expect("rendered");
            assert!(
                resolved
                    .warnings
                    .iter()
                    .any(|w| w.contains("unknown attribute 'width'")),
                "result {position} lost its attribute warning: {:?}",
                resolved.warnings,
            );
            assert!(
                resolved
                    .warnings
                    .iter()
                    .any(|w| w.contains("missing/thing.iuml")),
                "result {position} lost its include warning: {:?}",
                resolved.warnings,
            );
        }
    }

    #[test]
    fn one_bad_request_does_not_fail_its_batch() {
        let cached = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, cached, DiagramFormat::Svg),
            "",
            "<svg>cached</svg>",
        );

        // Loopback refuses at once, but an environment that drops the packet
        // instead would otherwise leave this waiting out the 30s default.
        let provider = with_timeout(KrokiProvider::new(DEAD_KROKI), Duration::from_millis(50))
            .with_cache(Box::new(cache));
        let results = provider.resolve(
            &[
                &request(0, "mermaid", cached),
                &request(1, "rust", "fn main() {}"),
                &request(2, "mermaid", "graph LR"),
            ],
            &ResolveContext::default(),
        );

        assert_eq!(results.len(), 3, "one result per request, always");
        assert_eq!(
            svg_of(results[0].as_ref().expect("resolved")),
            "<svg>cached</svg>"
        );

        let unhandled = results[1].as_ref().expect_err("kroki renders no rust");
        assert!(
            unhandled.message.contains("rust"),
            "the message should name the language: {}",
            unhandled.message,
        );
        assert!(!unhandled.transient, "retrying cannot make rust a diagram");

        let unreachable = results[2]
            .as_ref()
            .expect_err("nothing listens on the configured port");
        assert!(
            unreachable.transient,
            "a connection failure must stay retryable, or the caller caches it forever",
        );
        assert!(
            unreachable.message.starts_with("HTTP request failed: "),
            "the message should describe the failure and its cause: {}",
            unreachable.message,
        );
    }

    #[test]
    fn a_batch_returns_one_result_per_request_in_order() {
        let stub = KrokiStub::start();
        let provider = KrokiProvider::new(&stub.url);

        // Interleaved formats, so a provider that grouped renders by format
        // would hand them back out of order.
        let sources = ["graph one", "graph two", "graph three", "graph four"];
        let requests: Vec<DiagramRequest> = sources
            .iter()
            .enumerate()
            .map(|(position, source)| {
                let request = request(
                    u32::try_from(position).expect("a small index"),
                    "mermaid",
                    source,
                );
                if position % 2 == 0 {
                    request
                } else {
                    with_attr(request, "format", "png")
                }
            })
            .collect();

        let results = provider.resolve(
            &requests.iter().collect::<Vec<_>>(),
            &ResolveContext::default(),
        );

        assert_eq!(results.len(), sources.len());
        for (position, source) in sources.iter().enumerate() {
            let resolved = results[position].as_ref().expect("rendered");
            let format = if position % 2 == 0 {
                DiagramFormat::Svg
            } else {
                DiagramFormat::Png
            };
            assert_eq!(
                resolved.digest,
                hash_of(DiagramLanguage::Mermaid, source, format),
                "result {position} answers the wrong request",
            );
            if position % 2 == 0 {
                assert!(
                    svg_of(resolved).contains(source),
                    "result {position} holds another diagram's SVG",
                );
            } else {
                assert!(
                    png_of(resolved).ends_with(source.as_bytes()),
                    "result {position} holds another diagram's PNG",
                );
            }
        }
    }

    #[test]
    fn to_display_px_scales_down_from_the_render_dpi() {
        assert_eq!(to_display_px(400, 192), 200);
        assert_eq!(to_display_px(300, STANDARD_DPI), 300);
    }

    /// `value * STANDARD_DPI` overflows u32 above ~44.7M.
    #[test]
    fn to_display_px_does_not_overflow_on_wide_input() {
        assert_eq!(to_display_px(u32::MAX, STANDARD_DPI), u32::MAX);
        assert_eq!(to_display_px(u32::MAX, 192), u32::MAX / 2);
    }

    /// Rounding a 1px dimension down to 0 would collapse the element.
    #[test]
    fn to_display_px_keeps_at_least_one_pixel() {
        assert_eq!(to_display_px(1, 192), 1);
        assert_eq!(to_display_px(0, 192), 0, "nothing to show stays nothing");
    }
}
