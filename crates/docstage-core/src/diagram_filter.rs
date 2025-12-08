//! Diagram extraction as an iterator adapter over pulldown-cmark events.
//!
//! Supports multiple diagram languages via Kroki: PlantUML, Mermaid, GraphViz, etc.

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
    /// Returns None if the language is not a supported diagram type.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
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
    /// PlantUML and C4PlantUML need `!include` resolution and config injection.
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
    /// External SVG via `<img>` tag (cacheable separately, but no links).
    Img,
}

impl DiagramFormat {
    /// Parse format from attribute value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "svg" => Some(Self::Svg),
            "png" => Some(Self::Png),
            "img" => Some(Self::Img),
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

/// Parse code fence info string into language and attributes.
///
/// Format: `language [key=value ...]`
///
/// Example: `plantuml format=png` → `(PlantUml, Png)`
fn parse_info_string(info: &str) -> Option<(DiagramLanguage, DiagramFormat)> {
    let mut parts = info.split_whitespace();
    let language = DiagramLanguage::parse(parts.next()?)?;

    let mut format = DiagramFormat::default();
    for part in parts {
        if let Some((key, value)) = part.split_once('=')
            && key == "format"
            && let Some(f) = DiagramFormat::parse(value)
        {
            format = f;
        }
    }

    Some((language, format))
}

/// Iterator adapter that extracts diagrams from a pulldown-cmark event stream.
///
/// This filter:
/// - Intercepts code blocks with supported diagram languages
/// - Collects their source code into `ExtractedDiagram` structs
/// - Emits `{{DIAGRAM_N}}` placeholder as `Event::Html`
/// - Passes through all other events unchanged
pub struct DiagramFilter<'a, I: Iterator<Item = Event<'a>>> {
    iter: I,
    diagrams: Vec<ExtractedDiagram>,
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
                    if let Some((language, format)) = parse_info_string(&info) {
                        self.state = FilterState::InDiagram {
                            source: String::new(),
                            language,
                            format,
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
                    if let FilterState::InDiagram {
                        source,
                        language,
                        format,
                    } = old_state
                    {
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
                    unreachable!()
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
    fn test_parses_img_format() {
        let markdown = "```mermaid format=img\ngraph TD\n  A --> B\n```";
        let parser = Parser::new(markdown);
        let mut filter = DiagramFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].format, DiagramFormat::Img);
    }

    #[test]
    fn test_extracts_multiple_diagrams() {
        let markdown = r#"
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
"#;
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
        assert_eq!(
            parse_info_string("plantuml"),
            Some((DiagramLanguage::PlantUml, DiagramFormat::Svg))
        );
        assert_eq!(
            parse_info_string("plantuml format=png"),
            Some((DiagramLanguage::PlantUml, DiagramFormat::Png))
        );
        assert_eq!(
            parse_info_string("mermaid format=img"),
            Some((DiagramLanguage::Mermaid, DiagramFormat::Img))
        );
        assert_eq!(parse_info_string("rust"), None);
        assert_eq!(parse_info_string(""), None);
    }
}
