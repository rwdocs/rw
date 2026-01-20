//! Code block processor for diagram languages.
//!
//! This module provides [`DiagramProcessor`], which implements the
//! [`CodeBlockProcessor`] trait for extracting diagram code blocks during rendering.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use docstage_renderer::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};

use crate::cache::{DiagramCache, NullCache, compute_diagram_hash};
use crate::consts::DEFAULT_DPI;
use crate::html_embed::{scale_svg_dimensions, strip_google_fonts_import};
use crate::kroki::{
    DiagramRequest, render_all, render_all_png_data_uri_partial, render_all_svg_partial,
};
use crate::language::{DiagramFormat, DiagramLanguage, ExtractedDiagram};
use crate::output::{DiagramOutput, RenderedDiagramInfo};
use crate::plantuml::{PrepareResult, load_config_file, prepare_diagram_source};

/// Code block processor for diagram languages.
///
/// Extracts diagram code blocks (PlantUML, Mermaid, GraphViz, etc.) and replaces
/// them with placeholders during rendering. Placeholders are replaced with
/// rendered diagrams during `post_process()`.
///
/// # Configuration
///
/// Configure the processor using builder methods:
/// - [`kroki_url`](Self::kroki_url): Set the Kroki server URL (required for rendering)
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
/// let processor = DiagramProcessor::new()
///     .kroki_url("https://kroki.io")
///     .dpi(192);
///
/// let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_processor(processor);
///
/// // render() auto-calls post_process() on all processors
/// let result = renderer.render(parser);
/// ```
pub struct DiagramProcessor {
    extracted: Vec<ExtractedCodeBlock>,
    warnings: Vec<String>,
    /// Kroki server URL for rendering diagrams.
    kroki_url: Option<String>,
    /// Directories to search for PlantUML `!include` files.
    include_dirs: Vec<PathBuf>,
    /// PlantUML config content (loaded from config file).
    config_content: Option<String>,
    /// DPI for diagram rendering (None = default 192).
    dpi: Option<u32>,
    /// Cache for diagram rendering (defaults to NullCache).
    cache: Arc<dyn DiagramCache>,
    /// Output mode for diagram rendering.
    output: DiagramOutput,
}

impl Default for DiagramProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramProcessor {
    /// Create a new diagram processor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            extracted: Vec::new(),
            warnings: Vec::new(),
            kroki_url: None,
            include_dirs: Vec::new(),
            config_content: None,
            dpi: None,
            cache: Arc::new(NullCache),
            output: DiagramOutput::default(),
        }
    }

    /// Set the Kroki server URL for rendering diagrams.
    ///
    /// This is required for [`post_process`](Self::post_process) to render diagrams.
    /// If not set, placeholders will not be replaced.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new()
    ///     .kroki_url("https://kroki.io");
    /// ```
    #[must_use]
    pub fn kroki_url(mut self, url: impl Into<String>) -> Self {
        self.kroki_url = Some(url.into());
        self
    }

    /// Set directories to search for PlantUML `!include` files.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new()
    ///     .include_dirs(&[PathBuf::from("docs"), PathBuf::from("includes")]);
    /// ```
    #[must_use]
    pub fn include_dirs(mut self, dirs: &[PathBuf]) -> Self {
        self.include_dirs = dirs.to_vec();
        self
    }

    /// Load PlantUML config from a file.
    ///
    /// The config file is searched in the include directories.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new()
    ///     .include_dirs(&[PathBuf::from(".")])
    ///     .config_file(Some("config.iuml"));
    /// ```
    #[must_use]
    pub fn config_file(mut self, config_file: Option<&str>) -> Self {
        self.config_content = config_file.and_then(|cf| load_config_file(&self.include_dirs, cf));
        self
    }

    /// Set PlantUML config content directly.
    ///
    /// Use this when the config content is already loaded (e.g., from a previous call).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new()
    ///     .config_content(Some("skinparam backgroundColor white"));
    /// ```
    #[must_use]
    pub fn config_content(mut self, content: Option<&str>) -> Self {
        self.config_content = content.map(ToString::to_string);
        self
    }

    /// Set DPI for diagram rendering.
    ///
    /// Default is 192 (2x for retina displays). Set to 96 for standard resolution.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processor = DiagramProcessor::new()
    ///     .dpi(96); // Standard resolution
    /// ```
    #[must_use]
    pub fn dpi(mut self, dpi: u32) -> Self {
        self.dpi = Some(dpi);
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
    /// let processor = DiagramProcessor::new()
    ///     .kroki_url("https://kroki.io")
    ///     .with_cache(cache);
    /// ```
    #[must_use]
    pub fn with_cache(mut self, cache: Arc<dyn DiagramCache>) -> Self {
        self.cache = cache;
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
    /// let processor = DiagramProcessor::new()
    ///     .kroki_url("https://kroki.io")
    ///     .output(DiagramOutput::Files {
    ///         output_dir: "public/diagrams".into(),
    ///         tag_generator: Arc::new(ImgTagGenerator::new("/diagrams/")),
    ///     });
    /// ```
    #[must_use]
    pub fn output(mut self, output: DiagramOutput) -> Self {
        self.output = output;
        self
    }

    /// Prepare diagram source for rendering.
    ///
    /// For PlantUML diagrams, this resolves `!include` directives and injects config.
    /// For other diagram types, returns the source as-is.
    fn prepare_source(&self, diagram: &ExtractedDiagram) -> PrepareResult {
        if diagram.language.needs_plantuml_preprocessing() {
            prepare_diagram_source(
                &diagram.source,
                &self.include_dirs,
                self.config_content.as_deref(),
                self.dpi,
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
        let Some(kroki_url) = self.kroki_url.clone() else {
            return;
        };

        let diagrams = to_extracted_diagrams(&self.extracted);
        if diagrams.is_empty() {
            return;
        }

        match self.output.clone() {
            DiagramOutput::Inline => {
                let cache = self.cache.clone();
                self.post_process_inline(html, &diagrams, &kroki_url, &cache);
            }
            DiagramOutput::Files {
                output_dir,
                tag_generator,
            } => {
                self.post_process_files(html, &diagrams, &kroki_url, &output_dir, &tag_generator);
            }
        }
    }

    fn extracted(&self) -> Vec<ExtractedCodeBlock> {
        self.extracted.clone()
    }

    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

/// Inline diagram rendering implementation.
impl DiagramProcessor {
    /// Post-process with inline output mode.
    ///
    /// Checks cache first, renders only cache misses via Kroki.
    fn post_process_inline(
        &mut self,
        html: &mut String,
        diagrams: &[ExtractedDiagram],
        kroki_url: &str,
        cache: &Arc<dyn DiagramCache>,
    ) {
        // Prepare all diagrams and compute hashes
        let prepared: Vec<_> = diagrams
            .iter()
            .map(|diagram| {
                let prepare_result = self.prepare_source(diagram);
                self.warnings.extend(prepare_result.warnings);

                let endpoint = diagram.language.kroki_endpoint();
                let format_str = diagram.format.as_str();
                let hash =
                    compute_diagram_hash(&prepare_result.source, endpoint, format_str, self.dpi);

                (diagram, prepare_result.source, hash)
            })
            .collect();

        // Separate cache hits from misses
        let mut svg_to_render: Vec<(usize, DiagramRequest, String)> = Vec::new();
        let mut png_to_render: Vec<(usize, DiagramRequest, String)> = Vec::new();

        for (diagram, source, hash) in &prepared {
            let format_str = diagram.format.as_str();

            if let Some(cached_content) = cache.get(hash, format_str) {
                // Cache hit: replace placeholder directly
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
                replace_placeholder(html, diagram.index, &figure);
            } else {
                // Cache miss: add to render queue
                let request = DiagramRequest::new(diagram.index, source.clone(), diagram.language);

                match diagram.format {
                    DiagramFormat::Svg => {
                        svg_to_render.push((diagram.index, request, hash.clone()))
                    }
                    DiagramFormat::Png => {
                        png_to_render.push((diagram.index, request, hash.clone()))
                    }
                }
            }
        }

        // Render cache misses
        self.render_and_cache_svg(html, &svg_to_render, kroki_url, cache);
        self.render_and_cache_png(html, &png_to_render, kroki_url, cache);
    }

    /// Render SVG diagrams, cache results, and replace placeholders.
    fn render_and_cache_svg(
        &mut self,
        html: &mut String,
        to_render: &[(usize, DiagramRequest, String)],
        kroki_url: &str,
        cache: &Arc<dyn DiagramCache>,
    ) {
        if to_render.is_empty() {
            return;
        }

        let (requests, hash_map) = extract_requests_and_hashes(to_render);

        match render_all_svg_partial(&requests, kroki_url, 4) {
            Ok(result) => {
                for r in result.rendered {
                    let clean_svg = strip_google_fonts_import(r.svg.trim());
                    let scaled_svg = scale_svg_dimensions(&clean_svg, self.dpi);

                    if let Some(hash) = hash_map.get(&r.index) {
                        cache.set(hash, "svg", &scaled_svg);
                    }

                    let figure = format!(r#"<figure class="diagram">{scaled_svg}</figure>"#);
                    replace_placeholder(html, r.index, &figure);
                }
                handle_render_errors(html, result.errors);
            }
            Err(e) => replace_all_with_error(html, to_render, &e.to_string()),
        }
    }

    /// Render PNG diagrams, cache results, and replace placeholders.
    fn render_and_cache_png(
        &self,
        html: &mut String,
        to_render: &[(usize, DiagramRequest, String)],
        kroki_url: &str,
        cache: &Arc<dyn DiagramCache>,
    ) {
        if to_render.is_empty() {
            return;
        }

        let (requests, hash_map) = extract_requests_and_hashes(to_render);

        match render_all_png_data_uri_partial(&requests, kroki_url, 4) {
            Ok(result) => {
                for r in result.rendered {
                    if let Some(hash) = hash_map.get(&r.index) {
                        cache.set(hash, "png", &r.data_uri);
                    }

                    let figure = format!(
                        r#"<figure class="diagram"><img src="{}" alt="diagram"></figure>"#,
                        r.data_uri
                    );
                    replace_placeholder(html, r.index, &figure);
                }
                handle_render_errors(html, result.errors);
            }
            Err(e) => replace_all_with_error(html, to_render, &e.to_string()),
        }
    }

    /// Post-process with file-based output mode.
    ///
    /// Renders diagrams to PNG files and replaces placeholders with custom tags.
    fn post_process_files(
        &mut self,
        html: &mut String,
        diagrams: &[ExtractedDiagram],
        kroki_url: &str,
        output_dir: &std::path::Path,
        tag_generator: &Arc<dyn crate::output::DiagramTagGenerator>,
    ) {
        // Prepare all diagrams
        let diagram_requests: Vec<_> = diagrams
            .iter()
            .map(|d| {
                let prepare_result = self.prepare_source(d);
                self.warnings.extend(prepare_result.warnings);
                DiagramRequest::new(d.index, prepare_result.source, d.language)
            })
            .collect();

        let server_url = kroki_url.trim_end_matches('/');
        let dpi = self.dpi.unwrap_or(DEFAULT_DPI);

        match render_all(&diagram_requests, server_url, output_dir, 4) {
            Ok(rendered_diagrams) => {
                for r in rendered_diagrams {
                    let info = RenderedDiagramInfo {
                        filename: r.filename,
                        width: r.width,
                        height: r.height,
                    };
                    let tag = tag_generator.generate_tag(&info, dpi);
                    replace_placeholder(html, r.index, &tag);
                }
            }
            Err(e) => {
                // Replace all placeholders with error
                for d in diagrams {
                    replace_with_error(html, d.index, &e.to_string());
                }
            }
        }
    }
}

/// Extract requests and build index-to-hash mapping from render queue.
fn extract_requests_and_hashes(
    to_render: &[(usize, DiagramRequest, String)],
) -> (Vec<DiagramRequest>, HashMap<usize, &str>) {
    let requests = to_render.iter().map(|(_, r, _)| r.clone()).collect();
    let hash_map = to_render
        .iter()
        .map(|(idx, _, hash)| (*idx, hash.as_str()))
        .collect();
    (requests, hash_map)
}

/// Replace a diagram placeholder with content.
fn replace_placeholder(html: &mut String, index: usize, content: &str) {
    let placeholder = format!("{{{{DIAGRAM_{index}}}}}");
    *html = html.replace(&placeholder, content);
}

/// Replace a diagram placeholder with an error message.
fn replace_with_error(html: &mut String, index: usize, error_msg: &str) {
    use docstage_renderer::escape_html;

    let error_figure = format!(
        r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: {}</pre></figure>"#,
        escape_html(error_msg)
    );
    replace_placeholder(html, index, &error_figure);
}

/// Handle render errors by replacing placeholders with error messages.
fn handle_render_errors(html: &mut String, errors: Vec<crate::kroki::DiagramError>) {
    for e in errors {
        replace_with_error(html, e.index, &e.to_string());
    }
}

/// Replace all placeholders with the same error message.
fn replace_all_with_error(
    html: &mut String,
    to_render: &[(usize, DiagramRequest, String)],
    error_msg: &str,
) {
    for (idx, _, _) in to_render {
        replace_with_error(html, *idx, error_msg);
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
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
        let attrs = HashMap::new();

        let result = processor.process("rust", &attrs, "fn main() {}", 0);

        assert_eq!(result, ProcessResult::PassThrough);
        assert!(processor.extracted().is_empty());
    }

    #[test]
    fn test_process_with_format_png() {
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
        let mut attrs = HashMap::new();
        attrs.insert("size".to_string(), "large".to_string());

        processor.process("plantuml", &attrs, "source", 0);

        assert_eq!(processor.warnings().len(), 1);
        assert!(processor.warnings()[0].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_process_multiple_diagrams() {
        let mut processor = DiagramProcessor::new();
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
        let mut processor = DiagramProcessor::new();
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
            let mut processor = DiagramProcessor::new();
            let attrs = HashMap::new();

            let result = processor.process(lang, &attrs, "source", 0);

            assert!(
                matches!(result, ProcessResult::Placeholder(_)),
                "Expected Placeholder for language: {}",
                lang
            );
        }
    }
}
