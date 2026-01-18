//! Diagram extraction as an iterator adapter over pulldown-cmark events.
//!
//! Supports multiple diagram languages via Kroki: `PlantUML`, Mermaid, `GraphViz`, etc.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

/// Supported diagram languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramLanguage {
    PlantUml,
    C4PlantUml,
    Mermaid,
    GraphViz,
    Ditaa,
    BlockDiag,
    SeqDiag,
    ActDiag,
    NwDiag,
    PacketDiag,
    RackDiag,
    Erd,
    Nomnoml,
    Svgbob,
    Vega,
    VegaLite,
    WaveDrom,
}

impl DiagramLanguage {
    /// Parse language from code fence info string.
    ///
    /// Supports both direct language names (`mermaid`) and `kroki-` prefixed names
    /// (`kroki-mermaid`) for compatibility with `MkDocs` Kroki plugin.
    ///
    /// Returns None if the language is not a supported diagram type.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        // Support both "mermaid" and "kroki-mermaid" formats
        let lang = s.strip_prefix("kroki-").unwrap_or(s);

        match lang {
            "plantuml" => Some(Self::PlantUml),
            "c4plantuml" => Some(Self::C4PlantUml),
            "mermaid" => Some(Self::Mermaid),
            "graphviz" | "dot" => Some(Self::GraphViz),
            "ditaa" => Some(Self::Ditaa),
            "blockdiag" => Some(Self::BlockDiag),
            "seqdiag" => Some(Self::SeqDiag),
            "actdiag" => Some(Self::ActDiag),
            "nwdiag" => Some(Self::NwDiag),
            "packetdiag" => Some(Self::PacketDiag),
            "rackdiag" => Some(Self::RackDiag),
            "erd" => Some(Self::Erd),
            "nomnoml" => Some(Self::Nomnoml),
            "svgbob" => Some(Self::Svgbob),
            "vega" => Some(Self::Vega),
            "vegalite" => Some(Self::VegaLite),
            "wavedrom" => Some(Self::WaveDrom),
            _ => None,
        }
    }

    /// Kroki endpoint name for this diagram type.
    #[must_use]
    pub fn kroki_endpoint(&self) -> &'static str {
        match self {
            Self::PlantUml => "plantuml",
            Self::C4PlantUml => "c4plantuml",
            Self::Mermaid => "mermaid",
            Self::GraphViz => "graphviz",
            Self::Ditaa => "ditaa",
            Self::BlockDiag => "blockdiag",
            Self::SeqDiag => "seqdiag",
            Self::ActDiag => "actdiag",
            Self::NwDiag => "nwdiag",
            Self::PacketDiag => "packetdiag",
            Self::RackDiag => "rackdiag",
            Self::Erd => "erd",
            Self::Nomnoml => "nomnoml",
            Self::Svgbob => "svgbob",
            Self::Vega => "vega",
            Self::VegaLite => "vegalite",
            Self::WaveDrom => "wavedrom",
        }
    }

    /// Whether this diagram type requires PlantUML-specific preprocessing.
    ///
    /// `PlantUML` and `C4PlantUML` need `!include` resolution and config injection.
    #[must_use]
    pub fn needs_plantuml_preprocessing(&self) -> bool {
        matches!(self, Self::PlantUml | Self::C4PlantUml)
    }
}

/// Output format for rendered diagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramFormat {
    /// Inline SVG (default, supports links and interactivity).
    #[default]
    Svg,
    /// Inline PNG as base64 data URI (smaller for complex diagrams, no interactivity).
    Png,
}

impl DiagramFormat {
    /// Parse format from attribute value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "svg" => Some(Self::Svg),
            "png" => Some(Self::Png),
            _ => None,
        }
    }
}

/// Information about an extracted diagram.
#[derive(Debug, Clone)]
pub struct ExtractedDiagram {
    /// Original source code from markdown.
    pub source: String,
    /// Zero-based index of this diagram.
    pub index: usize,
    /// Diagram language (plantuml, mermaid, etc.).
    pub language: DiagramLanguage,
    /// Output format (svg, png, img).
    pub format: DiagramFormat,
}

/// Result of parsing a code fence info string.
struct ParsedInfoString {
    language: DiagramLanguage,
    format: DiagramFormat,
    warnings: Vec<String>,
}

/// Parse code fence info string into language and attributes.
///
/// Format: `language [key=value ...]`
///
/// Example: `plantuml format=png` → `(PlantUml, Png)`
fn parse_info_string(info: &str) -> Option<ParsedInfoString> {
    let mut parts = info.split_whitespace();
    let language = DiagramLanguage::parse(parts.next()?)?;

    let mut format = DiagramFormat::default();
    let mut warnings = Vec::new();

    for part in parts {
        if let Some((key, value)) = part.split_once('=') {
            if key == "format" {
                if let Some(f) = DiagramFormat::parse(value) {
                    format = f;
                } else {
                    warnings.push(format!(
                        "unknown format value '{value}', using default 'svg' (valid: svg, png)"
                    ));
                }
            } else {
                warnings.push(format!("unknown attribute '{key}' ignored (valid: format)"));
            }
        } else {
            warnings.push(format!(
                "malformed attribute '{part}' ignored (expected key=value)"
            ));
        }
    }

    Some(ParsedInfoString {
        language,
        format,
        warnings,
    })
}

/// Iterator adapter that extracts diagrams from a pulldown-cmark event stream.
///
/// This filter:
/// - Intercepts code blocks with supported diagram languages
/// - Collects their source code into `ExtractedDiagram` structs
/// - Emits `{{DIAGRAM_N}}` placeholder as `Event::Html`
/// - Passes through all other events unchanged
/// - Collects warnings for unknown attributes or format values
pub struct DiagramFilter<'a, I: Iterator<Item = Event<'a>>> {
    iter: I,
    diagrams: Vec<ExtractedDiagram>,
    warnings: Vec<String>,
    state: FilterState,
}

#[derive(Debug, Default)]
enum FilterState {
    #[default]
    Normal,
    InDiagram {
        source: String,
        language: DiagramLanguage,
        format: DiagramFormat,
    },
}

impl<'a, I: Iterator<Item = Event<'a>>> DiagramFilter<'a, I> {
    /// Create a new diagram filter wrapping the given event iterator.
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            diagrams: Vec::new(),
            warnings: Vec::new(),
            state: FilterState::Normal,
        }
    }

    /// Get a reference to the diagrams extracted so far.
    #[must_use]
    pub fn diagrams(&self) -> &[ExtractedDiagram] {
        &self.diagrams
    }

    /// Consume the filter and return the collected diagrams.
    #[must_use]
    pub fn into_diagrams(self) -> Vec<ExtractedDiagram> {
        self.diagrams
    }

    /// Get a reference to the warnings collected so far.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Consume the filter and return both diagrams and warnings.
    #[must_use]
    pub fn into_parts(self) -> (Vec<ExtractedDiagram>, Vec<String>) {
        (self.diagrams, self.warnings)
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for DiagramFilter<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let event = self.iter.next()?;

            match (&mut self.state, event) {
                // Start of a diagram code block
                (
                    FilterState::Normal,
                    Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))),
                ) => {
                    if let Some(parsed) = parse_info_string(&info) {
                        // Collect any warnings from parsing with diagram context
                        let diagram_index = self.diagrams.len();
                        for w in parsed.warnings {
                            self.warnings.push(format!("diagram {diagram_index}: {w}"));
                        }
                        self.state = FilterState::InDiagram {
                            source: String::new(),
                            language: parsed.language,
                            format: parsed.format,
                        };
                        // Don't emit the Start event, continue to collect content
                    } else {
                        // Not a diagram, pass through
                        return Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))));
                    }
                }

                // Text inside diagram block - collect it
                (FilterState::InDiagram { source, .. }, Event::Text(text)) => {
                    source.push_str(&text);
                }

                // End of diagram block - emit placeholder
                (FilterState::InDiagram { .. }, Event::End(TagEnd::CodeBlock)) => {
                    // Take the state and reset to Normal
                    let old_state = std::mem::take(&mut self.state);
                    let FilterState::InDiagram {
                        source,
                        language,
                        format,
                    } = old_state
                    else {
                        // Should not happen - state machine invariant violated
                        // Continue without emitting anything rather than panicking
                        continue;
                    };

                    let index = self.diagrams.len();
                    self.diagrams.push(ExtractedDiagram {
                        source,
                        index,
                        language,
                        format,
                    });

                    // Emit placeholder as Html event (passes through unchanged)
                    let placeholder = format!("{{{{DIAGRAM_{index}}}}}");
                    return Some(Event::Html(CowStr::Boxed(placeholder.into_boxed_str())));
                }

                // Any other event while in diagram block (shouldn't happen normally)
                (FilterState::InDiagram { source, .. }, other) => {
                    // Handle unexpected events - just collect text representation
                    if let Event::SoftBreak | Event::HardBreak = other {
                        source.push('\n');
                    }
                }

                // Normal event - pass through
                (FilterState::Normal, event) => {
                    return Some(event);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    #[test]
    fn test_extracts_plantuml_diagram() {
        let markdown = "# Title\n\n```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n\nText";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].index, 0);
        assert_eq!(diagrams[0].language, DiagramLanguage::PlantUml);
        assert_eq!(diagrams[0].format, DiagramFormat::Svg);
        assert!(diagrams[0].source.contains("Alice -> Bob"));

        let has_placeholder = events
            .iter()
            .any(|e| matches!(e, Event::Html(s) if s.contains("{{DIAGRAM_0}}")));
        assert!(has_placeholder);
    }

    #[test]
    fn test_extracts_mermaid_diagram() {
        let markdown = "```mermaid\ngraph TD\n  A --> B\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].language, DiagramLanguage::Mermaid);
        assert!(diagrams[0].source.contains("graph TD"));
    }

    #[test]
    fn test_extracts_graphviz_diagram() {
        let markdown = "```graphviz\ndigraph G { A -> B }\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].language, DiagramLanguage::GraphViz);
    }

    #[test]
    fn test_extracts_dot_alias() {
        let markdown = "```dot\ndigraph G { A -> B }\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].language, DiagramLanguage::GraphViz);
    }

    #[test]
    fn test_parses_format_attribute() {
        let markdown = "```plantuml format=png\n@startuml\nA -> B\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].format, DiagramFormat::Png);
    }

    #[test]
    fn test_extracts_multiple_diagrams() {
        let markdown = r"
```plantuml
@startuml
A -> B
@enduml
```

Some text

```mermaid
graph TD
  C --> D
```
";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 2);
        assert_eq!(diagrams[0].language, DiagramLanguage::PlantUml);
        assert_eq!(diagrams[1].language, DiagramLanguage::Mermaid);

        let placeholder_count = events
            .iter()
            .filter(|e| matches!(e, Event::Html(s) if s.contains("DIAGRAM_")))
            .count();
        assert_eq!(placeholder_count, 2);
    }

    #[test]
    fn test_passes_through_other_code_blocks() {
        let markdown = "```rust\nfn main() {}\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert!(diagrams.is_empty());

        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Start(Tag::CodeBlock(_))))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Text(s) if s.contains("fn main")))
        );
    }

    #[test]
    fn test_no_diagrams() {
        let markdown = "# Just text\n\nNo diagrams here.";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert!(diagrams.is_empty());
    }

    #[test]
    fn test_kroki_endpoints() {
        assert_eq!(DiagramLanguage::PlantUml.kroki_endpoint(), "plantuml");
        assert_eq!(DiagramLanguage::Mermaid.kroki_endpoint(), "mermaid");
        assert_eq!(DiagramLanguage::GraphViz.kroki_endpoint(), "graphviz");
        assert_eq!(DiagramLanguage::C4PlantUml.kroki_endpoint(), "c4plantuml");
    }

    #[test]
    fn test_plantuml_preprocessing_flag() {
        assert!(DiagramLanguage::PlantUml.needs_plantuml_preprocessing());
        assert!(DiagramLanguage::C4PlantUml.needs_plantuml_preprocessing());
        assert!(!DiagramLanguage::Mermaid.needs_plantuml_preprocessing());
        assert!(!DiagramLanguage::GraphViz.needs_plantuml_preprocessing());
    }

    #[test]
    fn test_parse_info_string() {
        let result = parse_info_string("plantuml").unwrap();
        assert_eq!(result.language, DiagramLanguage::PlantUml);
        assert_eq!(result.format, DiagramFormat::Svg);
        assert!(result.warnings.is_empty());

        let result = parse_info_string("plantuml format=png").unwrap();
        assert_eq!(result.language, DiagramLanguage::PlantUml);
        assert_eq!(result.format, DiagramFormat::Png);
        assert!(result.warnings.is_empty());

        assert!(parse_info_string("rust").is_none());
        assert!(parse_info_string("").is_none());
    }

    #[test]
    fn test_parse_info_string_unknown_format() {
        let result = parse_info_string("plantuml format=jpeg").unwrap();
        assert_eq!(result.language, DiagramLanguage::PlantUml);
        assert_eq!(result.format, DiagramFormat::Svg); // fallback
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("unknown format value 'jpeg'"));
    }

    #[test]
    fn test_parse_info_string_unknown_attribute() {
        let result = parse_info_string("plantuml size=large").unwrap();
        assert_eq!(result.language, DiagramLanguage::PlantUml);
        assert_eq!(result.format, DiagramFormat::Svg);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_parse_info_string_malformed_attribute() {
        let result = parse_info_string("plantuml nope").unwrap();
        assert_eq!(result.language, DiagramLanguage::PlantUml);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("malformed attribute 'nope'"));
    }

    #[test]
    fn test_filter_collects_warnings() {
        let markdown = "```plantuml formt=png\n@startuml\nA -> B\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let (diagrams, warnings) = filter.into_parts();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("diagram 0"));
        assert!(warnings[0].contains("unknown attribute 'formt'"));
    }

    #[test]
    fn test_diagrams_ref_during_iteration() {
        let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        // Diagrams empty before iteration
        assert!(filter.diagrams().is_empty());

        let _events: Vec<_> = filter.by_ref().collect();

        // Diagrams available via reference after iteration
        assert_eq!(filter.diagrams().len(), 1);
        assert!(filter.diagrams()[0].source.contains("A -> B"));
    }

    #[test]
    fn test_warnings_ref_during_iteration() {
        let markdown = "```plantuml bad=attr\n@startuml\nA -> B\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        // Warnings empty before iteration
        assert!(filter.warnings().is_empty());

        let _events: Vec<_> = filter.by_ref().collect();

        // Warnings available via reference after iteration
        assert_eq!(filter.warnings().len(), 1);
        assert!(filter.warnings()[0].contains("unknown attribute"));
    }

    #[test]
    fn test_all_diagram_languages() {
        // Test all supported languages, both direct and kroki- prefixed forms
        let languages = [
            ("plantuml", DiagramLanguage::PlantUml),
            ("c4plantuml", DiagramLanguage::C4PlantUml),
            ("mermaid", DiagramLanguage::Mermaid),
            ("graphviz", DiagramLanguage::GraphViz),
            ("dot", DiagramLanguage::GraphViz), // alias, no kroki- form
            ("ditaa", DiagramLanguage::Ditaa),
            ("blockdiag", DiagramLanguage::BlockDiag),
            ("seqdiag", DiagramLanguage::SeqDiag),
            ("actdiag", DiagramLanguage::ActDiag),
            ("nwdiag", DiagramLanguage::NwDiag),
            ("packetdiag", DiagramLanguage::PacketDiag),
            ("rackdiag", DiagramLanguage::RackDiag),
            ("erd", DiagramLanguage::Erd),
            ("nomnoml", DiagramLanguage::Nomnoml),
            ("svgbob", DiagramLanguage::Svgbob),
            ("vega", DiagramLanguage::Vega),
            ("vegalite", DiagramLanguage::VegaLite),
            ("wavedrom", DiagramLanguage::WaveDrom),
        ];

        for (name, expected) in languages {
            // Test direct form
            let parsed = DiagramLanguage::parse(name);
            assert_eq!(parsed, Some(expected), "Failed to parse: {name}");

            // Test kroki- prefixed form (MkDocs Kroki plugin format)
            let kroki_name = format!("kroki-{name}");
            let kroki_parsed = DiagramLanguage::parse(&kroki_name);
            assert_eq!(
                kroki_parsed,
                Some(expected),
                "Failed to parse: {kroki_name}"
            );
        }
    }

    #[test]
    fn test_kroki_prefix_extraction() {
        let markdown = "```kroki-mermaid\ngraph TD\n  A --> B\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].language, DiagramLanguage::Mermaid);
        assert!(diagrams[0].source.contains("graph TD"));
    }

    #[test]
    fn test_kroki_prefix_unknown_language() {
        // kroki-unknown should not be recognized
        assert!(DiagramLanguage::parse("kroki-unknown").is_none());
        assert!(DiagramLanguage::parse("kroki-").is_none());
    }

    #[test]
    fn test_all_kroki_endpoints() {
        // Verify all languages have correct endpoint
        let endpoints = [
            (DiagramLanguage::PlantUml, "plantuml"),
            (DiagramLanguage::C4PlantUml, "c4plantuml"),
            (DiagramLanguage::Mermaid, "mermaid"),
            (DiagramLanguage::GraphViz, "graphviz"),
            (DiagramLanguage::Ditaa, "ditaa"),
            (DiagramLanguage::BlockDiag, "blockdiag"),
            (DiagramLanguage::SeqDiag, "seqdiag"),
            (DiagramLanguage::ActDiag, "actdiag"),
            (DiagramLanguage::NwDiag, "nwdiag"),
            (DiagramLanguage::PacketDiag, "packetdiag"),
            (DiagramLanguage::RackDiag, "rackdiag"),
            (DiagramLanguage::Erd, "erd"),
            (DiagramLanguage::Nomnoml, "nomnoml"),
            (DiagramLanguage::Svgbob, "svgbob"),
            (DiagramLanguage::Vega, "vega"),
            (DiagramLanguage::VegaLite, "vegalite"),
            (DiagramLanguage::WaveDrom, "wavedrom"),
        ];

        for (lang, expected) in endpoints {
            assert_eq!(
                lang.kroki_endpoint(),
                expected,
                "Wrong endpoint for {lang:?}"
            );
        }
    }

    #[test]
    fn test_diagram_format_default() {
        let format = DiagramFormat::default();
        assert_eq!(format, DiagramFormat::Svg);
    }

    #[test]
    fn test_diagram_format_parse() {
        assert_eq!(DiagramFormat::parse("svg"), Some(DiagramFormat::Svg));
        assert_eq!(DiagramFormat::parse("png"), Some(DiagramFormat::Png));
        assert_eq!(DiagramFormat::parse("img"), None);
        assert_eq!(DiagramFormat::parse("jpeg"), None);
        assert_eq!(DiagramFormat::parse(""), None);
    }

    #[test]
    fn test_extracted_diagram_clone() {
        let diagram = ExtractedDiagram {
            source: "test".to_string(),
            index: 0,
            language: DiagramLanguage::PlantUml,
            format: DiagramFormat::Svg,
        };
        let cloned = diagram.clone();
        assert_eq!(cloned.source, "test");
        assert_eq!(cloned.index, 0);
        assert_eq!(cloned.language, DiagramLanguage::PlantUml);
        assert_eq!(cloned.format, DiagramFormat::Svg);
    }

    #[test]
    fn test_extracted_diagram_debug() {
        let diagram = ExtractedDiagram {
            source: "test".to_string(),
            index: 0,
            language: DiagramLanguage::Mermaid,
            format: DiagramFormat::Png,
        };
        let debug_str = format!("{diagram:?}");
        assert!(debug_str.contains("ExtractedDiagram"));
        assert!(debug_str.contains("Mermaid"));
        assert!(debug_str.contains("Png"));
    }

    #[test]
    fn test_diagram_language_clone_copy() {
        let lang = DiagramLanguage::PlantUml;
        let copied = lang;
        assert_eq!(lang, DiagramLanguage::PlantUml);
        assert_eq!(copied, DiagramLanguage::PlantUml);
    }

    #[test]
    fn test_diagram_format_clone_copy() {
        let fmt = DiagramFormat::Png;
        let copied = fmt;
        assert_eq!(fmt, DiagramFormat::Png);
        assert_eq!(copied, DiagramFormat::Png);
    }

    #[test]
    fn test_empty_diagram_block() {
        let markdown = "```plantuml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert!(diagrams[0].source.is_empty());

        let has_placeholder = events
            .iter()
            .any(|e| matches!(e, Event::Html(s) if s.contains("{{DIAGRAM_0}}")));
        assert!(has_placeholder);
    }

    #[test]
    fn test_diagram_preserves_whitespace() {
        let markdown = "```plantuml\n  @startuml\n    A -> B\n  @enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert!(diagrams[0].source.contains("  @startuml"));
        assert!(diagrams[0].source.contains("    A -> B"));
    }

    #[test]
    fn test_multiple_warnings_in_single_diagram() {
        let markdown = "```plantuml format=bad size=huge\n@startuml\nA\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let warnings = filter.warnings();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("format value 'bad'"));
        assert!(warnings[1].contains("unknown attribute 'size'"));
    }

    #[test]
    fn test_c4plantuml_extraction() {
        let markdown = "```c4plantuml\n@startuml\nSystem(sys, \"System\")\n@enduml\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].language, DiagramLanguage::C4PlantUml);
        assert!(diagrams[0].language.needs_plantuml_preprocessing());
    }
}
