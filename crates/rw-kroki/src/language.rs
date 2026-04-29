//! Diagram types for supported diagram languages.
//!
//! Supports multiple diagram languages via Kroki: `PlantUML`, Mermaid, `GraphViz`, etc.

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
    pub fn kroki_endpoint(self) -> &'static str {
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
    pub fn needs_plantuml_preprocessing(self) -> bool {
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

    /// Return format as string representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }
}

/// Information about an extracted diagram.
#[derive(Debug)]
pub struct ExtractedDiagram {
    /// Original source code from markdown.
    pub source: String,
    /// Zero-based index of this diagram.
    pub index: usize,
    /// Diagram language (plantuml, mermaid, etc.).
    pub language: DiagramLanguage,
    /// Output format (svg, png).
    pub format: DiagramFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_diagram_format_as_str() {
        assert_eq!(DiagramFormat::Svg.as_str(), "svg");
        assert_eq!(DiagramFormat::Png.as_str(), "png");
    }

    #[test]
    fn test_extracted_diagram_debug() {
        let diagram = ExtractedDiagram {
            source: "test".to_owned(),
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
}
