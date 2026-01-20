//! Code block processor for diagram languages.
//!
//! This module provides [`DiagramProcessor`], which implements the
//! [`CodeBlockProcessor`] trait for extracting diagram code blocks during rendering.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use docstage_renderer::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};

use crate::cache::{DiagramCache, DiagramKey, NullCache};
use crate::consts::DEFAULT_DPI;
use crate::html_embed::{scale_svg_dimensions, strip_google_fonts_import};
use crate::kroki::{
    DiagramRequest, render_all, render_all_png_data_uri_partial, render_all_svg_partial,
};
use crate::language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};
use crate::output::{DiagramOutput, DiagramTagGenerator, RenderedDiagramInfo};
use crate::plantuml::{PrepareResult, load_config_file, prepare_diagram_source};

/// Configuration for diagram processing (immutable after setup).
///
/// Separated from mutable state to allow borrowing config while mutating state,
/// avoiding unnecessary clones in Rust's ownership model.
struct ProcessorConfig {
    /// Kroki server URL for rendering diagrams (required).
    kroki_url: String,
    /// Directories to search for PlantUML `!include` files.
    include_dirs: Vec<PathBuf>,
    /// PlantUML config content (loaded from config file).
    config_content: Option<String>,
    /// DPI for diagram rendering (default: 192).
    dpi: u32,
    /// Cache for diagram rendering (defaults to NullCache).
    cache: Arc<dyn DiagramCache>,
    /// Output mode for diagram rendering.
    output: DiagramOutput,
}

/// Code block processor for diagram languages.
///
/// Extracts diagram code blocks (PlantUML, Mermaid, GraphViz, etc.) and replaces
/// them with placeholders during rendering. Placeholders are replaced with
/// rendered diagrams during `post_process()`.
///
/// # Configuration
///
/// Create the processor with a required Kroki URL, then configure using builder methods:
/// - [`include_dirs`](Self::include_dirs): Set directories for PlantUML `!include` resolution
/// - [`config_file`](Self::config_file): Load PlantUML config from a file
/// - [`dpi`](Self::dpi): Set DPI for diagram rendering (default: 192)
///
/// # Example
///
/// ```ignore
/// use pulldown_cmark::Parser;
/// use docstage_diagrams::DiagramProcessor;
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
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
                config_content: None,
                dpi: DEFAULT_DPI,
                cache: Arc::new(NullCache),
                output: DiagramOutput::default(),
            },
            extracted: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Set directories to search for PlantUML `!include` files.
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

    /// Load PlantUML config from a file.
    ///
    /// The config file is searched in the include directories.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .include_dirs(&[PathBuf::from(".")])
    ///     .config_file(Some("config.iuml"));
    /// ```
    #[must_use]
    pub fn config_file(mut self, config_file: Option<&str>) -> Self {
        self.config.config_content =
            config_file.and_then(|cf| load_config_file(&self.config.include_dirs, cf));
        self
    }

    /// Set PlantUML config content directly.
    ///
    /// Use this when the config content is already loaded (e.g., from a previous call).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .config_content(Some("skinparam backgroundColor white"));
    /// ```
    #[must_use]
    pub fn config_content(mut self, content: Option<&str>) -> Self {
        self.config.config_content = content.map(ToString::to_string);
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
    /// use std::sync::Arc;
    /// use docstage_diagrams::{DiagramProcessor, FileCache};
    ///
    /// let cache = Arc::new(FileCache::new(".cache/diagrams".into()));
    /// let processor = DiagramProcessor::new("https://kroki.io")
    ///     .with_cache(cache);
    /// ```
    #[must_use]
    pub fn with_cache(mut self, cache: Arc<dyn DiagramCache>) -> Self {
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
    /// use docstage_diagrams::{DiagramProcessor, DiagramOutput, ImgTagGenerator};
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
    /// For PlantUML diagrams, this resolves `!include` directives and injects config.
    /// For other diagram types, returns the source as-is.
    fn prepare_source(config: &ProcessorConfig, diagram: &ExtractedDiagram) -> PrepareResult {
        if diagram.language.needs_plantuml_preprocessing() {
            prepare_diagram_source(
                &diagram.source,
                &config.include_dirs,
                config.config_content.as_deref(),
                config.dpi,
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
        stored_attrs.insert("format".to_string(), format.as_str().to_string());
        stored_attrs.insert(
            "endpoint".to_string(),
            diagram_language.kroki_endpoint().to_string(),
        );

        // Extract the code block
        self.extracted.push(ExtractedCodeBlock {
            index,
            language: language.to_string(),
            source: source.to_string(),
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
        let mut replacements = Replacements::new();

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

        for (diagram, source) in &prepared {
            let cache_info = CacheInfo {
                index: diagram.index,
                source: source.clone(),
                endpoint: diagram.language.kroki_endpoint(),
                format: diagram.format.as_str(),
            };

            if let Some(cached_content) = config.cache.get(cache_info.key(config.dpi)) {
                // Cache hit: add replacement
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
                // Cache miss: add to render queue
                let request = DiagramRequest::new(diagram.index, source.clone(), diagram.language);

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

        match render_all_svg_partial(&requests, &config.kroki_url, 4) {
            Ok(result) => {
                for r in result.rendered {
                    let clean_svg = strip_google_fonts_import(r.svg.trim());
                    let scaled_svg = scale_svg_dimensions(&clean_svg, config.dpi);

                    if let Some(info) = cache_map.get(&r.index) {
                        config.cache.set(info.key(config.dpi), &scaled_svg);
                    }

                    let figure = format!(r#"<figure class="diagram">{scaled_svg}</figure>"#);
                    replacements.add(r.index, figure);
                }
                for e in result.errors {
                    replacements.add_error(e.index, &e.to_string());
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                for &idx in cache_map.keys() {
                    replacements.add_error(idx, &error_msg);
                }
            }
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

        match render_all_png_data_uri_partial(&requests, &config.kroki_url, 4) {
            Ok(result) => {
                for r in result.rendered {
                    if let Some(info) = cache_map.get(&r.index) {
                        config.cache.set(info.key(config.dpi), &r.data_uri);
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
            Err(e) => {
                let error_msg = e.to_string();
                for &idx in cache_map.keys() {
                    replacements.add_error(idx, &error_msg);
                }
            }
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
        let mut replacements = Replacements::new();

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

        match render_all(&diagram_requests, server_url, output_dir, 4, config.dpi) {
            Ok(rendered_diagrams) => {
                for r in rendered_diagrams {
                    let info = RenderedDiagramInfo {
                        filename: r.filename,
                        width: r.width,
                        height: r.height,
                    };
                    let tag = tag_generator.generate_tag(&info, config.dpi);
                    replacements.add(r.index, tag);
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                for d in diagrams {
                    replacements.add_error(d.index, &error_msg);
                }
            }
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
/// Instead of calling `html.replace()` for each diagram (O(N × string_length)),
/// this collects all replacements and applies them in a single pass.
struct Replacements {
    map: HashMap<usize, String>,
}

impl Replacements {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Add a replacement for a diagram placeholder.
    fn add(&mut self, index: usize, content: String) {
        self.map.insert(index, content);
    }

    /// Add an error message for a diagram placeholder.
    fn add_error(&mut self, index: usize, error_msg: &str) {
        use docstage_renderer::escape_html;

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
/// use docstage_diagrams::{DiagramProcessor, to_extracted_diagram};
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
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
/// use docstage_diagrams::{DiagramProcessor, to_extracted_diagrams};
/// use docstage_renderer::{MarkdownRenderer, HtmlBackend};
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
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
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
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
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
            ProcessResult::Placeholder("{{DIAGRAM_0}}".to_string())
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
        attrs.insert("format".to_string(), "png".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"png".to_string())
        );
        assert!(processor.warnings().is_empty());
    }

    #[test]
    fn test_process_with_invalid_format() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = HashMap::new();
        attrs.insert("format".to_string(), "jpeg".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        // Should default to svg
        assert_eq!(
            processor.extracted()[0].attrs.get("format"),
            Some(&"svg".to_string())
        );
        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown format value 'jpeg'"));
    }

    #[test]
    fn test_process_with_unknown_attribute() {
        let mut processor = DiagramProcessor::new("https://kroki.io");
        let mut attrs = HashMap::new();
        attrs.insert("size".to_string(), "large".to_string());

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
            language: "plantuml".to_string(),
            source: "@startuml\nA -> B\n@enduml".to_string(),
            attrs: HashMap::from([("format".to_string(), "png".to_string())]),
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
            language: "rust".to_string(),
            source: "fn main() {}".to_string(),
            attrs: HashMap::new(),
        };

        assert!(to_extracted_diagram(&block).is_none());
    }

    #[test]
    fn test_to_extracted_diagrams() {
        let blocks = vec![
            ExtractedCodeBlock {
                index: 0,
                language: "plantuml".to_string(),
                source: "source1".to_string(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 1,
                language: "rust".to_string(), // Not a diagram
                source: "source2".to_string(),
                attrs: HashMap::new(),
            },
            ExtractedCodeBlock {
                index: 2,
                language: "mermaid".to_string(),
                source: "source3".to_string(),
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
            Some(&"plantuml".to_string())
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
                "Expected Placeholder for language: {}",
                lang
            );
        }
    }

    #[test]
    fn test_replacements_single() {
        let mut html = String::from("<p>Before</p>{{DIAGRAM_0}}<p>After</p>");
        let mut replacements = Replacements::new();
        replacements.add(0, "<svg>diagram</svg>".to_string());

        replacements.apply(&mut html);

        assert_eq!(html, "<p>Before</p><svg>diagram</svg><p>After</p>");
    }

    #[test]
    fn test_replacements_multiple() {
        let mut html =
            String::from("{{DIAGRAM_0}}<p>middle</p>{{DIAGRAM_1}}<p>end</p>{{DIAGRAM_2}}");
        let mut replacements = Replacements::new();
        replacements.add(0, "<svg>first</svg>".to_string());
        replacements.add(1, "<svg>second</svg>".to_string());
        replacements.add(2, "<svg>third</svg>".to_string());

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
        replacements.add(0, "A".to_string());
        replacements.add(1, "B".to_string());
        replacements.add(2, "C".to_string());

        replacements.apply(&mut html);

        assert_eq!(html, "CAB");
    }

    #[test]
    fn test_replacements_missing_keeps_placeholder() {
        let mut html = String::from("{{DIAGRAM_0}}{{DIAGRAM_1}}");
        let mut replacements = Replacements::new();
        replacements.add(0, "A".to_string());
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
        replacements.add(12345, "content".to_string());

        replacements.apply(&mut html);

        assert_eq!(html, "content");
    }
}
