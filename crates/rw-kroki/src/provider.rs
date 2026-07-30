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

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use rw_cache::{Cache, CacheBucket, CacheBucketExt, NullCache};
use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, ResolveContext, Resolved,
    Size,
};
use rw_plantuml::prepare_diagram_source;
use ureq::Agent;

use crate::cache::DiagramKey;
use crate::consts::{DEFAULT_TIMEOUT, PNG_DATA_URI_PREFIX};
use crate::html_embed::{scale_svg_dimensions, strip_google_fonts_import};
use crate::kroki::{
    DiagramError as KrokiError, DiagramRequest as KrokiJob, create_agent, png_data_uri_dimensions,
    render_all_png_data_uri_partial, render_all_svg_partial,
};
use crate::language::{DiagramFormat, DiagramLanguage};
use crate::scale::to_display_px;

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
    /// Kroki server URL renders are posted to.
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
            kroki_url: kroki_url.into(),
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

    /// Give up on a Kroki request after `timeout`.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.agent = create_agent(timeout);
        self
    }

    /// Render the SVG batch, storing and reporting each result at the position
    /// its request held.
    fn render_svg(&self, jobs: &[KrokiJob], misses: &mut [Option<Miss>], results: &mut [Slot]) {
        if jobs.is_empty() {
            return;
        }

        let outcome = render_all_svg_partial(jobs, &self.kroki_url, &self.agent);
        for rendered in outcome.rendered {
            let Some(miss) = take_miss(misses, rendered.index) else {
                continue;
            };
            // Content manipulation, not markup: the external font request is
            // dropped and the oversampled dimensions are divided back down, so
            // what a consumer embeds is already display-sized.
            let stripped = strip_google_fonts_import(rendered.svg.trim());
            let (scaled, _size) = scale_svg_dimensions(&stripped, rendered.language.render_dpi());
            self.cache.set_string(&miss.hash, "", &scaled);

            fill(
                results,
                rendered.index,
                Ok(Resolved {
                    asset: Asset::Inline(DiagramContent::Svg(scaled)),
                    // An SVG carries its own width and height, so there is
                    // nothing left for a consumer to size it by.
                    size: None,
                    digest: miss.hash,
                    warnings: miss.warnings,
                }),
            );
        }
        record_errors(outcome.errors, results);
    }

    /// Render the PNG batch, storing and reporting each result at the position
    /// its request held.
    fn render_png(&self, jobs: &[KrokiJob], misses: &mut [Option<Miss>], results: &mut [Slot]) {
        if jobs.is_empty() {
            return;
        }

        let outcome = render_all_png_data_uri_partial(jobs, &self.kroki_url, &self.agent);
        for rendered in outcome.rendered {
            let Some(miss) = take_miss(misses, rendered.index) else {
                continue;
            };
            self.cache.set_string(&miss.hash, "", &rendered.data_uri);
            fill(
                results,
                rendered.index,
                png_resolution(&rendered.data_uri, rendered.language, miss),
            );
        }
        record_errors(outcome.errors, results);
    }
}

impl DiagramProvider for KrokiProvider {
    fn handles(&self, language: &str) -> bool {
        DiagramLanguage::parse(language).is_some()
    }

    fn resolve(
        &self,
        requests: &[DiagramRequest],
        ctx: &ResolveContext<'_>,
    ) -> Vec<Result<Resolved, DiagramError>> {
        let mut results: Vec<Slot> = (0..requests.len()).map(|_| None).collect();
        let mut misses: Vec<Option<Miss>> = (0..requests.len()).map(|_| None).collect();
        let mut svg_jobs = Vec::new();
        let mut png_jobs = Vec::new();

        for (position, request) in requests.iter().enumerate() {
            let Some(language) = DiagramLanguage::parse(&request.language) else {
                fill(
                    &mut results,
                    position,
                    Err(DiagramError {
                        message: format!(
                            "kroki renders no language named '{}'; routing sent this fence to the wrong provider",
                            request.language
                        ),
                        transient: false,
                    }),
                );
                continue;
            };

            let mut warnings = Vec::new();
            let format = read_format(&request.attrs, &mut warnings);

            // Asked of the parsed language, not the fence string: the two
            // agree for every spelling that parses today, and asking the
            // language keeps them agreeing if an alias is ever added.
            let source = if language.needs_plantuml_preprocessing() {
                let prepared = prepare_diagram_source(
                    &request.source,
                    &self.include_dirs,
                    language.render_dpi(),
                    ctx.model,
                );
                warnings.extend(prepared.warnings);
                prepared.source
            } else {
                request.source.clone()
            };

            let hash = DiagramKey::for_render(&source, language, format).compute_hash();
            let miss = Miss { hash, warnings };

            if let Some(content) = self.cache.get_string(&miss.hash, "") {
                fill(
                    &mut results,
                    position,
                    cached_resolution(format, &content, language, miss),
                );
                continue;
            }

            let job = KrokiJob::new(position, source, language);
            match format {
                DiagramFormat::Svg => svg_jobs.push(job),
                DiagramFormat::Png => png_jobs.push(job),
            }
            misses[position] = Some(miss);
        }

        // Rendering is batched per format, so results arrive grouped rather
        // than in request order; each is written back to the position its
        // request held, which is what the batch below is indexed by.
        self.render_svg(&svg_jobs, &mut misses, &mut results);
        self.render_png(&png_jobs, &mut misses, &mut results);

        results
            .into_iter()
            .map(|result| {
                // Every job comes back either rendered or failed, so this is
                // not a case the render paths above leave behind. It is here
                // because the trait's one-result-per-request promise is worth
                // holding by construction rather than by inspection.
                result.unwrap_or_else(|| {
                    Err(DiagramError {
                        message: "kroki returned neither a render nor an error for this diagram"
                            .to_owned(),
                        transient: true,
                    })
                })
            })
            .collect()
    }
}

/// One request's answer, once it has one.
type Slot = Option<Result<Resolved, DiagramError>>;

/// What a request that has to be rendered still needs afterwards.
struct Miss {
    /// Cache key for this render, and the digest its resolution carries.
    hash: String,
    /// Warnings raised while reading attributes and preparing the source.
    warnings: Vec<String>,
}

/// Record `result` as the answer to the request at `position`.
///
/// Positions are handed out by the loop that built the batch, so neither an
/// out-of-range one nor a second answer for the same request is reachable;
/// both are asserted rather than trusted, since either would quietly break the
/// one-result-per-request promise the caller matches on.
fn fill(results: &mut [Slot], position: usize, result: Result<Resolved, DiagramError>) {
    debug_assert!(
        position < results.len(),
        "no request at position {position} of {}",
        results.len(),
    );
    if let Some(slot) = results.get_mut(position) {
        debug_assert!(
            slot.is_none(),
            "the request at position {position} was answered twice",
        );
        *slot = Some(result);
    }
}

/// Reclaim the pending state of the request at `position`.
fn take_miss(misses: &mut [Option<Miss>], position: usize) -> Option<Miss> {
    misses.get_mut(position)?.take()
}

/// Report each render failure against the request it belongs to.
fn record_errors(errors: Vec<KrokiError>, results: &mut [Slot]) {
    for error in errors {
        let transient = error.kind.is_transient();
        fill(
            results,
            error.index,
            Err(DiagramError {
                // The kind alone. The error's own `Display` prefixes the index
                // it carries — which is this request's position in the batch
                // `resolve` was handed, the same number `fill` writes back to,
                // and a number that means nothing to whoever reads the message.
                message: error.kind.to_string(),
                transient,
            }),
        );
    }
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
            format = DiagramFormat::parse(value).unwrap_or_else(|| {
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

/// Turn a cache entry back into a resolution.
///
/// The stored SVG is already post-processed, and the stored PNG is the same
/// data URI a fresh render produces, so both paths agree on what a hit means.
fn cached_resolution(
    format: DiagramFormat,
    content: &str,
    language: DiagramLanguage,
    miss: Miss,
) -> Result<Resolved, DiagramError> {
    match format {
        DiagramFormat::Svg => Ok(Resolved {
            asset: Asset::Inline(DiagramContent::Svg(content.to_owned())),
            size: None,
            digest: miss.hash,
            warnings: miss.warnings,
        }),
        DiagramFormat::Png => png_resolution(content, language, miss),
    }
}

/// Decode a PNG data URI into the bytes and display size a consumer needs.
///
/// PNG travels as a base64 data URI — the form the string-keyed cache holds —
/// so a fresh render and a cache hit decode the same way.
fn png_resolution(
    data_uri: &str,
    language: DiagramLanguage,
    miss: Miss,
) -> Result<Resolved, DiagramError> {
    let bytes = data_uri
        .strip_prefix(PNG_DATA_URI_PREFIX)
        .and_then(|base64| BASE64_STANDARD.decode(base64).ok());
    let Some(bytes) = bytes else {
        return Err(DiagramError {
            message: "the PNG for this diagram is not a base64 image data URI".to_owned(),
            transient: false,
        });
    };

    let dpi = language.render_dpi();
    // Absent for a PNG whose header will not parse, which is the only case
    // there is no size to report.
    let size = png_data_uri_dimensions(data_uri).map(|(width, height)| Size {
        width: to_display_px(width, dpi),
        height: to_display_px(height, dpi),
    });

    Ok(Resolved {
        asset: Asset::Inline(DiagramContent::Png(bytes)),
        size,
        digest: miss.hash,
        warnings: miss.warnings,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use parking_lot::Mutex;
    use rw_cache::{CacheBucket, CacheBucketExt};
    use rw_diagrams::{
        Asset, DiagramContent, DiagramProvider, DiagramRequest, Entity, RequestKey, ResolveContext,
        Resolved, SiteModel, Size,
    };

    use super::{DEFAULT_TIMEOUT, KrokiProvider, PNG_DATA_URI_PREFIX};
    use crate::cache::DiagramKey;
    use crate::language::{DiagramFormat, DiagramLanguage};
    use crate::test_support::{KrokiStub, Reply, png_header, prepared_plantuml};

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
            (etag.is_empty() || entry.etag == etag).then(|| entry.value.clone())
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

    /// The 400x200 PNG plus `marker`, as the base64 data URI the cache holds.
    fn png_data_uri(marker: &[u8]) -> String {
        let mut bytes = png_header();
        bytes.extend_from_slice(marker);
        format!("{PNG_DATA_URI_PREFIX}{}", BASE64_STANDARD.encode(&bytes))
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
        DiagramKey::for_render(source, language, format).compute_hash()
    }

    // ── handles() ─────────────────────────────────────────────────────────

    #[test]
    fn handles_every_language_diagramlanguage_parses() {
        let provider = KrokiProvider::new(DEAD_KROKI);
        for language in [
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
        ] {
            assert!(provider.handles(language), "should handle '{language}'");
            let prefixed = format!("kroki-{language}");
            assert!(provider.handles(&prefixed), "should handle '{prefixed}'");
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
            &[with_attr(request(0, "mermaid", source), "format", "jpeg")],
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
                request(0, "mermaid", source),
                with_attr(request(1, "mermaid", source), "width", "100"),
            ],
            &ResolveContext::default(),
        );

        let plain = results[0].as_ref().expect("resolved");
        let annotated = results[1].as_ref().expect("resolved");
        assert_eq!(
            annotated.warnings,
            ["unknown attribute 'width' ignored (valid: format)"],
        );
        assert!(plain.warnings.is_empty());
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
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Png),
            "",
            &png_data_uri(b"png-entry"),
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[with_attr(request(0, "mermaid", source), "format", "png")],
            &ResolveContext::default(),
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert!(
            png_of(resolved).ends_with(b"png-entry"),
            "the PNG entry must be the one read, not the SVG one",
        );
        assert!(resolved.warnings.is_empty());
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
                &prepared_plantuml(source, &[], None),
                DiagramFormat::Svg,
            ),
            "",
            "<svg>prepared</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[request(0, "plantuml", source)],
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
        let results =
            provider.resolve(&[request(0, "mermaid", source)], &ResolveContext::default());

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
                &prepared_plantuml(source, &[], None),
                DiagramFormat::Svg,
            ),
            "",
            "<svg>cached</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[request(0, "plantuml", source)],
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

    // ── the site model ────────────────────────────────────────────────────

    #[test]
    fn a_meta_include_resolves_from_the_context_model() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let expanded = prepared_plantuml(source, &[], Some(&OneSystem));
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
            &[request(0, "plantuml", source)],
            &ResolveContext {
                model: Some(&OneSystem),
                page: None,
            },
        );

        let resolved = results[0].as_ref().expect("resolved");
        assert_eq!(svg_of(resolved), "<svg>expanded</svg>");
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn without_a_model_a_meta_include_is_left_alone() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let unexpanded = prepared_plantuml(source, &[], None);
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
            &[request(0, "plantuml", source)],
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
                &prepared_plantuml(source, &[], Some(&OneSystem)),
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
                &prepared_plantuml(source, &[], None),
                DiagramFormat::Svg,
            ),
            "",
            "<svg>from the cache</svg>",
        );

        // Nothing listens on the configured port, so content coming back at
        // all is the proof that the cache answered.
        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[request(0, "plantuml", source)],
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
                request(0, "plantuml", source),
                with_attr(request(1, "plantuml", source), "format", "png"),
            ],
            &ResolveContext::default(),
        );

        let svg = svg_of(results[0].as_ref().expect("rendered"));
        let prepared = prepared_plantuml(source, &[], None);
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
        // Both formats are stored, and the PNG stays in the data-URI form the
        // string-keyed cache holds rather than the bytes the caller was given.
        let stored_png = cache
            .get_string(
                &hash_of(DiagramLanguage::PlantUml, &prepared, DiagramFormat::Png),
                "",
            )
            .expect("the rendered PNG is cached too");
        assert!(
            stored_png.starts_with(PNG_DATA_URI_PREFIX),
            "the PNG entry must stay a data URI: {stored_png}",
        );
        assert_eq!(stub.paths(), ["/plantuml/svg", "/plantuml/png"]);
    }

    #[test]
    fn a_freshly_rendered_svg_is_display_sized_and_font_free() {
        let stub = KrokiStub::start();
        let provider = KrokiProvider::new(&stub.url);
        let results = provider.resolve(
            &[request(0, "plantuml", "@startuml\nA -> B\n@enduml")],
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
    fn the_cache_key_separates_svg_from_png() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Svg),
            "",
            "<svg>the svg entry</svg>",
        );
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Png),
            "",
            &png_data_uri(b"the png entry"),
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[
                request(0, "mermaid", source),
                with_attr(request(1, "mermaid", source), "format", "png"),
            ],
            &ResolveContext::default(),
        );

        let svg = results[0].as_ref().expect("resolved");
        let png = results[1].as_ref().expect("resolved");
        assert_eq!(svg_of(svg), "<svg>the svg entry</svg>");
        assert!(png_of(png).ends_with(b"the png entry"));
        assert_ne!(
            svg.digest, png.digest,
            "one source rendered two ways is two cache entries and two attachments",
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
                request(0, "mermaid", "graph one"),
                request(1, "mermaid", "graph two"),
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

        let language = DiagramLanguage::parse(cell.language).expect("a known language");
        let rendered_source = if language.needs_plantuml_preprocessing() {
            prepared_plantuml(source, &[], None)
        } else {
            source.to_owned()
        };
        let hash = hash_of(language, &rendered_source, cell.format);

        let cache = MemoryCache::default();
        let provider = if cell.fresh {
            KrokiProvider::new(&stub.url).with_cache(Box::new(cache))
        } else {
            cache.set_string(
                &hash,
                "",
                &match cell.format {
                    DiagramFormat::Svg => r#"<svg width="200" height="100"></svg>"#.to_owned(),
                    DiagramFormat::Png => png_data_uri(b""),
                },
            );
            KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache))
        };

        let mut request = request(0, cell.language, source);
        if cell.format == DiagramFormat::Png {
            request = with_attr(request, "format", "png");
        }
        let results = provider.resolve(&[request], &ResolveContext::default());

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

        let provider =
            KrokiProvider::new(format!("http://{address}")).timeout(Duration::from_millis(50));
        let started = Instant::now();
        let results = provider.resolve(
            &[request(0, "mermaid", "graph TD")],
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
                &[request(0, "mermaid", "graph TD")],
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
            &[with_attr(
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
            &[request(0, "mermaid", "graph TD")],
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

    #[test]
    fn a_png_entry_that_is_not_a_data_uri_fails_its_diagram() {
        let source = "graph TD";
        let cache = MemoryCache::default();
        // A cache entry can be anything on disk: truncated, written by an
        // older format, or corrupted. It must not be handed to a consumer as
        // if it were image bytes.
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Png),
            "",
            "<svg>not a png at all</svg>",
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[with_attr(request(0, "mermaid", source), "format", "png")],
            &ResolveContext::default(),
        );

        let error = results[0].as_ref().expect_err("the entry is not a PNG");
        assert!(
            error.message.contains("data URI"),
            "the message should say what was wrong with it: {}",
            error.message,
        );
        assert!(
            !error.transient,
            "the same entry decodes the same way every time",
        );
    }

    #[test]
    fn a_png_whose_header_will_not_parse_reports_no_size() {
        let source = "graph TD";
        // Decodes cleanly, but the bytes are not a PNG: there is no header to
        // read dimensions from, and no size to report — the one case a PNG
        // resolution comes back without one.
        let payload = b"not a png, but long enough to decode";
        let cache = MemoryCache::default();
        cache.set_string(
            &hash_of(DiagramLanguage::Mermaid, source, DiagramFormat::Png),
            "",
            &format!("{PNG_DATA_URI_PREFIX}{}", BASE64_STANDARD.encode(payload)),
        );

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[with_attr(request(0, "mermaid", source), "format", "png")],
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
        let expanded = prepared_plantuml(source, &include_dirs, None);
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
            &[request(0, "plantuml", source)],
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
            &prepared_plantuml(source, &[], None),
            DiagramFormat::Svg,
        );
        let cache = MemoryCache::default();
        cache.set_string(&hash, "", "<svg>cached</svg>");

        let provider = KrokiProvider::new(DEAD_KROKI).with_cache(Box::new(cache));
        let results = provider.resolve(
            &[request(0, "plantuml", source)],
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
                with_attr(request(0, "plantuml", source), "width", "100"),
                with_attr(
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
        let provider = KrokiProvider::new(DEAD_KROKI)
            .timeout(Duration::from_millis(50))
            .with_cache(Box::new(cache));
        let results = provider.resolve(
            &[
                request(0, "mermaid", cached),
                request(1, "rust", "fn main() {}"),
                request(2, "mermaid", "graph LR"),
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
        // This text reaches an operator, so it has to say what went wrong —
        // and it must not open with the batch position the kroki-side error
        // carries, which is a number no reader can place.
        assert!(
            unreachable.message.contains("HTTP request failed"),
            "the message should describe the failure: {}",
            unreachable.message,
        );
        assert!(
            !unreachable.message.starts_with("diagram "),
            "attribution is the caller's, not a batch index: {}",
            unreachable.message,
        );
    }

    #[test]
    fn a_batch_returns_one_result_per_request_in_order() {
        let stub = KrokiStub::start();
        let provider = KrokiProvider::new(&stub.url);

        // Interleaved formats: rendering is batched per format, so results
        // come back grouped and have to be put back where they came from.
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

        let results = provider.resolve(&requests, &ResolveContext::default());

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
}
