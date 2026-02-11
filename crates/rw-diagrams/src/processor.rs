//! Code block processor for diagram languages.
//!
//! This module provides [`DiagramProcessor`], which implements the
//! [`CodeBlockProcessor`] trait for extracting diagram code blocks during rendering.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rw_renderer::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
use ureq::Agent;

use crate::cache::DiagramKey;
use crate::consts::{DEFAULT_DPI, DEFAULT_TIMEOUT};
use crate::html_embed::{scale_svg_dimensions, strip_google_fonts_import};
use crate::kroki::{
    DiagramRequest, create_agent, render_all, render_all_png_data_uri_partial,
    render_all_svg_partial,
};
use crate::language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};
use crate::output::{DiagramOutput, DiagramTagGenerator, RenderedDiagramInfo};
use crate::plantuml::{PrepareResult, prepare_diagram_source};
use rw_cache::{Cache, CacheBucket, CacheBucketExt};

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
}

/// Code block processor for diagram languages.
///
/// Extracts diagram code blocks (`PlantUML`, Mermaid, `GraphViz`, etc.) and replaces
/// them with placeholders during rendering. Placeholders are replaced with
/// rendered diagrams during `post_process()`.
///
/// # Configuration
///
/// Create the processor with a required Kroki URL, then configure using builder methods:
/// - [`include_dirs`](Self::include_dirs): Set directories for `PlantUML` `!include` resolution
/// - [`dpi`](Self::dpi): Set DPI for diagram rendering (default: 192)
///
/// # Example
///
/// ```ignore
/// use pulldown_cmark::Parser;
/// use rw_diagrams::DiagramProcessor;
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
/// let parser = Parser::new(markdown);
///
/// let processor = DiagramProcessor::new("https://kroki.io")
///     .dpi(192);
///
/// let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_processor(processor);
///
/// // render() auto-calls post_process() on all processors
/// let result = renderer.render(parser);
/// ```
pub struct DiagramProcessor {
    /// Configuration (immutable after setup).
    config: ProcessorConfig,
    /// Extracted code blocks (accumulated during processing).
    extracted: Vec<ExtractedCodeBlock>,
    /// Warnings (accumulated during processing).
    warnings: Vec<String>,
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
    /// ```ignore
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
            },
            extracted: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Set directories to search for `PlantUML` `!include` files.
    ///
    /// # Example
    ///
    /// ```ignore
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
    /// ```ignore
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
    /// ```ignore
    /// use std::time::Duration;
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
    /// When a cache is provided, [`post_process`](Self::post_process) will:
    /// 1. Compute a content hash for each diagram
    /// 2. Check the cache for a hit before rendering via Kroki
    /// 3. Store newly rendered diagrams in the cache
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rw_cache::{Cache, NullCache};
    /// use rw_diagrams::DiagramProcessor;
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
    /// ```ignore
    /// use std::sync::Arc;
    /// use rw_diagrams::{DiagramProcessor, DiagramOutput, ImgTagGenerator};
    ///
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .output(DiagramOutput::Files {
    ///         output_dir: "public/diagrams".into(),
    ///         tag_generator: Arc::new(ImgTagGenerator::new("/diagrams/")),
    ///     });
    /// ```
    #[must_use]
    pub fn output(mut self, output: DiagramOutput) -> Self {
        self.config.output = output;
        self
    }

    /// Prepare diagram source for rendering.
    ///
    /// For `PlantUML` diagrams, this resolves `!include` directives and injects config.
    /// For other diagram types, returns the source as-is.
    fn prepare_source(config: &ProcessorConfig, diagram: &ExtractedDiagram) -> PrepareResult {
        if diagram.language.needs_plantuml_preprocessing() {
            prepare_diagram_source(&diagram.source, &config.include_dirs, config.dpi)
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
        attrs: &HashMap<String, String>,
        source: &str,
        index: usize,
    ) -> ProcessResult {
        let Some(diagram_language) = DiagramLanguage::parse(language) else {
            return ProcessResult::PassThrough;
        };

        // Parse format attribute with validation
        let format = attrs.get("format").map_or(DiagramFormat::default(), |value| {
            DiagramFormat::parse(value).unwrap_or_else(|| {
                self.warnings.push(format!(
                    "diagram {index}: unknown format value '{value}', using default 'svg' (valid: svg, png)"
                ));
                DiagramFormat::default()
            })
        });

        // Warn about unknown attributes
        for key in attrs.keys().filter(|k| *k != "format") {
            self.warnings.push(format!(
                "diagram {index}: unknown attribute '{key}' ignored (valid: format)"
            ));
        }

        // Store the format in attrs for later use
        let mut stored_attrs = attrs.clone();
        stored_attrs.insert("format".to_owned(), format.as_str().to_owned());
        stored_attrs.insert(
            "endpoint".to_owned(),
            diagram_language.kroki_endpoint().to_owned(),
        );

        // Extract the code block
        self.extracted.push(ExtractedCodeBlock {
            index,
            language: language.to_owned(),
            source: source.to_owned(),
            attrs: stored_attrs,
        });

        ProcessResult::Placeholder(format!("{{{{DIAGRAM_{index}}}}}"))
    }

    fn post_process(&mut self, html: &mut String) {
        let diagrams = to_extracted_diagrams(&self.extracted);
        if diagrams.is_empty() {
            return;
        }

        match &self.config.output {
            DiagramOutput::Inline => {
                Self::post_process_inline(&self.config, &mut self.warnings, html, &diagrams);
            }
            DiagramOutput::Files {
                output_dir,
                tag_generator,
            } => {
                Self::post_process_files(
                    &self.config,
                    &mut self.warnings,
                    html,
                    &diagrams,
                    output_dir,
                    tag_generator,
                );
            }
        }
    }

    fn extracted(&self) -> &[ExtractedCodeBlock] {
        &self.extracted
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
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
    /// Post-process with inline output mode.
    ///
    /// Checks cache first, renders only cache misses via Kroki.
    fn post_process_inline(
        config: &ProcessorConfig,
        warnings: &mut Vec<String>,
        html: &mut String,
        diagrams: &[ExtractedDiagram],
    ) {
        // Collect all replacements for single-pass application
        let mut replacements = Replacements::with_capacity(diagrams.len());

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
                // Cache hit: add replacement directly
                let figure = match diagram.format {
                    DiagramFormat::Svg => {
                        format!(r#"<figure class="diagram">{cached_content}</figure>"#)
                    }
                    DiagramFormat::Png => {
                        format!(
                            r#"<figure class="diagram"><img src="{cached_content}" alt="diagram"></figure>"#
                        )
                    }
                };
                replacements.add(diagram.index, figure);
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

        // Render cache misses and collect replacements
        Self::render_and_cache_svg(config, &mut replacements, svg_to_render);
        Self::render_and_cache_png(config, &mut replacements, png_to_render);

        // Apply all replacements in a single pass
        replacements.apply(html);
    }

    /// Render SVG diagrams, cache results, and collect replacements.
    fn render_and_cache_svg(
        config: &ProcessorConfig,
        replacements: &mut Replacements,
        to_render: Vec<(DiagramRequest, CacheInfo)>,
    ) {
        if to_render.is_empty() {
            return;
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

            let figure = format!(r#"<figure class="diagram">{scaled_svg}</figure>"#);
            replacements.add(r.index, figure);
        }
        for e in result.errors {
            replacements.add_error(e.index, &e.to_string());
        }
    }

    /// Render PNG diagrams, cache results, and collect replacements.
    fn render_and_cache_png(
        config: &ProcessorConfig,
        replacements: &mut Replacements,
        to_render: Vec<(DiagramRequest, CacheInfo)>,
    ) {
        if to_render.is_empty() {
            return;
        }

        let (requests, cache_map) = extract_requests_and_cache_info(to_render);

        let result = render_all_png_data_uri_partial(&requests, &config.kroki_url, &config.agent);
        for r in result.rendered {
            if let Some(info) = cache_map.get(&r.index) {
                let hash = info.key(config.dpi).compute_hash();
                config.cache.set_string(&hash, "", &r.data_uri);
            }

            let figure = format!(
                r#"<figure class="diagram"><img src="{}" alt="diagram"></figure>"#,
                r.data_uri
            );
            replacements.add(r.index, figure);
        }
        for e in result.errors {
            replacements.add_error(e.index, &e.to_string());
        }
    }

    /// Post-process with file-based output mode.
    ///
    /// Renders diagrams to PNG files and replaces placeholders with custom tags.
    fn post_process_files(
        config: &ProcessorConfig,
        warnings: &mut Vec<String>,
        html: &mut String,
        diagrams: &[ExtractedDiagram],
        output_dir: &std::path::Path,
        tag_generator: &Arc<dyn DiagramTagGenerator>,
    ) {
        // Collect all replacements for single-pass application
        let mut replacements = Replacements::with_capacity(diagrams.len());

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
            let info = RenderedDiagramInfo {
                filename: r.filename,
                width: r.width,
                height: r.height,
            };
            let tag = tag_generator.generate_tag(&info, config.dpi);
            replacements.add(r.index, tag);
        }
        for e in result.errors {
            replacements.add_error(e.index, &e.to_string());
        }

        // Apply all replacements in a single pass
        replacements.apply(html);
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

/// Collects diagram replacements for single-pass application.
///
/// Instead of calling `html.replace()` for each diagram (O(N × `string_length`)),
/// this collects all replacements and applies them in a single pass.
struct Replacements {
    map: HashMap<usize, String>,
}

impl Replacements {
    #[cfg(test)]
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
        }
    }

    /// Add a replacement for a diagram placeholder.
    fn add(&mut self, index: usize, content: String) {
        self.map.insert(index, content);
    }

    /// Add an error message for a diagram placeholder.
    fn add_error(&mut self, index: usize, error_msg: &str) {
        use rw_renderer::escape_html;

        let error_figure = format!(
            r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: {}</pre></figure>"#,
            escape_html(error_msg)
        );
        self.add(index, error_figure);
    }

    /// Apply all replacements in a single pass.
    ///
    /// This scans the HTML once, replacing all `{{DIAGRAM_N}}` placeholders
    /// with their corresponding content from the map.
    fn apply(self, html: &mut String) {
        if self.map.is_empty() {
            return;
        }

        let mut result = String::with_capacity(html.len());
        let mut remaining = html.as_str();

        while let Some(start) = remaining.find("{{DIAGRAM_") {
            // Add everything before the placeholder
            result.push_str(&remaining[..start]);

            // Find the end of the placeholder
            let after_prefix = &remaining[start + 10..]; // Skip "{{DIAGRAM_"
            if let Some(end_pos) = after_prefix.find("}}") {
                // Parse the index
                let index_str = &after_prefix[..end_pos];
                if let Ok(index) = index_str.parse::<usize>() {
                    if let Some(replacement) = self.map.get(&index) {
                        result.push_str(replacement);
                    } else {
                        // No replacement found, keep original placeholder
                        result.push_str(&remaining[start..start + 10 + end_pos + 2]);
                    }
                } else {
                    // Invalid index, keep original placeholder
                    result.push_str(&remaining[start..start + 10 + end_pos + 2]);
                }
                remaining = &after_prefix[end_pos + 2..];
            } else {
                // No closing }}, keep rest as-is
                result.push_str(&remaining[start..]);
                remaining = "";
            }
        }

        // Add any remaining content
        result.push_str(remaining);

        *html = result;
    }
}

/// Convert an [`ExtractedCodeBlock`] to an [`ExtractedDiagram`].
///
/// Returns `None` if the code block is not a diagram type.
///
/// # Example
///
/// ```ignore
/// use rw_diagrams::{DiagramProcessor, to_extracted_diagram};
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// // After rendering with DiagramProcessor...
/// let blocks = renderer.extracted_code_blocks();
/// for block in &blocks {
///     if let Some(diagram) = to_extracted_diagram(block) {
///         println!("Diagram: {:?}", diagram.language);
///     }
/// }
/// ```
#[must_use]
pub fn to_extracted_diagram(block: &ExtractedCodeBlock) -> Option<ExtractedDiagram> {
    let language = DiagramLanguage::parse(&block.language)?;
    let format = block
        .attrs
        .get("format")
        .and_then(|s| DiagramFormat::parse(s))
        .unwrap_or_default();

    Some(ExtractedDiagram {
        source: block.source.clone(),
        index: block.index,
        language,
        format,
    })
}

/// Convert multiple [`ExtractedCodeBlock`]s to [`ExtractedDiagram`]s.
///
/// Filters out non-diagram blocks automatically.
///
/// # Example
///
/// ```ignore
/// use rw_diagrams::{DiagramProcessor, to_extracted_diagrams};
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// // After rendering with DiagramProcessor...
/// let blocks = renderer.extracted_code_blocks();
/// let diagrams = to_extracted_diagrams(&blocks);
/// ```
#[must_use]
pub fn to_extracted_diagrams(blocks: &[ExtractedCodeBlock]) -> Vec<ExtractedDiagram> {
    blocks.iter().filter_map(to_extracted_diagram).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_plantuml() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = HashMap::new();
        let source = "@startuml\nA -> B\n@enduml";

        let result = processor.process("plantuml", &attrs, source, 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_owned())
        );
        assert_eq!(processor.extracted().len(), 1);
        assert_eq!(processor.extracted()[0].language, "plantuml");
        assert_eq!(processor.extracted()[0].source, source);
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_mermaid() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = HashMap::new();

        let result = processor.process("mermaid", &attrs, "graph TD\n  A --> B", 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_owned())
        );
        assert_eq!(processor.extracted()[0].language, "mermaid");
    }

    #[test]
    fn test_process_kroki_prefix() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = HashMap::new();

        let result = processor.process("kroki-mermaid", &attrs, "graph TD", 0);

        assert_eq!(
            result,
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_owned())
        );
        assert_eq!(processor.extracted()[0].language, "kroki-mermaid");
    }

    #[test]
    fn test_process_non_diagram() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = HashMap::new();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);

        assert_eq!(result, ProcessResult::PassThrough);
        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_process_with_format_png() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = HashMap::new();
        attrs.insert("format".to_owned(), "png".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"png".to_owned())
        );
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_with_invalid_format() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = HashMap::new();
        attrs.insert("format".to_owned(), "jpeg".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        // Should default to svg
        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"svg".to_owned())
        );
        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown format value 'jpeg'"));
    }

    #[test]
    fn test_process_with_unknown_attribute() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = HashMap::new();
        attrs.insert("size".to_owned(), "large".to_owned());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_process_multiple_diagrams() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let attrs = HashMap::new();

        processor.process("plantuml", &attrs, "source1", 0);
        processor.process("mermaid", &attrs, "source2", 1);

        assert_eq!(processor.extracted().len(), 2);
        assert_eq!(processor.extracted()[0].index, 0);
        assert_eq!(processor.extracted()[1].index, 1);
    }

    #[test]
    fn test_to_extracted_diagram() {
        let block = ExtractedCodeBlock {
            index: 0,
            language: "plantuml".to_owned(),
            source: "@startuml\nA -> B\n@enduml".to_owned(),
            attrs: HashMap::from([("format".to_owned(), "png".to_owned())]),
        };

        let diagram = to_extracted_diagram(&block).unwrap();

        assert_eq!(diagram.index, 0);
        assert_eq!(diagram.language, DiagramLanguage::PlantUml);
        assert_eq!(diagram.format, DiagramFormat::Png);
        assert!(diagram.source.contains("A -> B"));
    }

    #[test]
    fn test_to_extracted_diagram_non_diagram() {
        let block = ExtractedCodeBlock {
            index: 0,
            language: "rust".to_owned(),
            source: "fn main() {}".to_owned(),
            attrs: HashMap::new(),
        };

        assert!(to_extracted_diagram(&block).is_none());
    }

    #[test]
    fn test_to_extracted_diagrams() {
        let blocks = vec![
            ExtractedCodeBlock {
                index: 0,
                language: "plantuml".to_owned(),
                source: "source1".to_owned(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 1,
                language: "rust".to_owned(), // Not a diagram
                source: "source2".to_owned(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 2,
                language: "mermaid".to_owned(),
                source: "source3".to_owned(),
                attrs: HashMap::new(),
            },
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
        let attrs = HashMap::new();

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs.get("endpoint"),
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
            let attrs = HashMap::new();

            let result = processor.process(lang, &attrs, "source", 0);

            assert!(
                matches!(result, ProcessResult::Placeholder(_)),
                "Expected Placeholder for language: {lang}"
            );
        }
    }

    #[test]
    fn test_replacements_single() {
        let mut html = String::from("<p>Before</p>{{DIAGRAM_0}}<p>After</p>");
        let mut replacements = Replacements::new();
        replacements.add(0, "<svg>diagram</svg>".to_owned());

        replacements.apply(&mut html);

        assert_eq!(html, "<p>Before</p><svg>diagram</svg><p>After</p>");
    }

    #[test]
    fn test_replacements_multiple() {
        let mut html =
            String::from("{{DIAGRAM_0}}<p>middle</p>{{DIAGRAM_1}}<p>end</p>{{DIAGRAM_2}}");
        let mut replacements = Replacements::new();
        replacements.add(0, "<svg>first</svg>".to_owned());
        replacements.add(1, "<svg>second</svg>".to_owned());
        replacements.add(2, "<svg>third</svg>".to_owned());

        replacements.apply(&mut html);

        assert_eq!(
            html,
            "<svg>first</svg><p>middle</p><svg>second</svg><p>end</p><svg>third</svg>"
        );
    }

    #[test]
    fn test_replacements_out_of_order() {
        let mut html = String::from("{{DIAGRAM_2}}{{DIAGRAM_0}}{{DIAGRAM_1}}");
        let mut replacements = Replacements::new();
        replacements.add(0, "A".to_owned());
        replacements.add(1, "B".to_owned());
        replacements.add(2, "C".to_owned());

        replacements.apply(&mut html);

        assert_eq!(html, "CAB");
    }

    #[test]
    fn test_replacements_missing_keeps_placeholder() {
        let mut html = String::from("{{DIAGRAM_0}}{{DIAGRAM_1}}");
        let mut replacements = Replacements::new();
        replacements.add(0, "A".to_owned());
        // No replacement for index 1

        replacements.apply(&mut html);

        assert_eq!(html, "A{{DIAGRAM_1}}");
    }

    #[test]
    fn test_replacements_empty_no_change() {
        let mut html = String::from("<p>No placeholders</p>");
        let replacements = Replacements::new();

        replacements.apply(&mut html);

        assert_eq!(html, "<p>No placeholders</p>");
    }

    #[test]
    fn test_replacements_large_index() {
        let mut html = String::from("{{DIAGRAM_12345}}");
        let mut replacements = Replacements::new();
        replacements.add(12345, "content".to_owned());

        replacements.apply(&mut html);

        assert_eq!(html, "content");
    }

    #[test]
    fn test_timeout_builder() {
        // Test that timeout builder method works and can be chained
        let processor = DiagramProcessor::new("https://kroki.io")
            .timeout(Duration::from_secs(60))
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
}
