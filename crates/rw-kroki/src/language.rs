//! Diagram types for supported diagram languages.
//!
//! Supports multiple diagram languages via Kroki: `PlantUML`, Mermaid, `GraphViz`, etc.

use std::fmt;
use std::str::FromStr;

use crate::consts::STANDARD_DPI;
use rw_diagrams::canonical_language;
use rw_plantuml::{PLANTUML_DPI, is_plantuml_fence};

/// Supported diagram languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagramLanguage {
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
    Structurizr,
}

impl DiagramLanguage {
    const ALL: [Self; 18] = [
        Self::PlantUml,
        Self::C4PlantUml,
        Self::Mermaid,
        Self::GraphViz,
        Self::Ditaa,
        Self::BlockDiag,
        Self::SeqDiag,
        Self::ActDiag,
        Self::NwDiag,
        Self::PacketDiag,
        Self::RackDiag,
        Self::Erd,
        Self::Nomnoml,
        Self::Svgbob,
        Self::Vega,
        Self::VegaLite,
        Self::WaveDrom,
        Self::Structurizr,
    ];

    /// Kroki endpoint name for this diagram type, which is also the fence
    /// language that selects it.
    pub(crate) fn kroki_endpoint(self) -> &'static str {
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
            Self::Structurizr => "structurizr",
        }
    }

    /// Whether this diagram type requires `PlantUML`-specific preprocessing:
    /// `!include` resolution and config injection.
    ///
    /// Delegates to `rw_plantuml::is_plantuml_fence` so the fence-string
    /// predicate and this one cannot define the family differently. That holds
    /// only while [`Self::kroki_endpoint`] returns a spelling that predicate
    /// accepts — `test_every_language_name` pins the spellings.
    pub(crate) fn needs_plantuml_preprocessing(self) -> bool {
        is_plantuml_fence(self.kroki_endpoint())
    }

    /// Whether Kroki renders this language as PNG; it renders all of them as
    /// SVG.
    ///
    /// Mirrors each Kroki service's `SUPPORTED_FORMATS`. Kroki answers a PNG
    /// request for the rest with a 400.
    pub(crate) fn renders_png(self) -> bool {
        !matches!(self, Self::Nomnoml | Self::Svgbob | Self::WaveDrom)
    }

    /// The DPI this language's output has to be scaled back down from.
    ///
    /// Only the languages that get a `skinparam dpi` injected into their source
    /// are rendered oversized, so only those need correcting. Everything else
    /// renders at a size Kroki alone decides, and reporting [`STANDARD_DPI`] for
    /// them makes the scaling a no-op.
    pub(crate) fn render_dpi(self) -> u32 {
        if self.needs_plantuml_preprocessing() {
            PLANTUML_DPI
        } else {
            STANDARD_DPI
        }
    }
}

/// Parses a code fence language, with or without the `kroki-` prefix the
/// `MkDocs` Kroki plugin uses.
impl FromStr for DiagramLanguage {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match canonical_language(s) {
            "dot" => Ok(Self::GraphViz),
            lang => Self::ALL
                .into_iter()
                .find(|language| language.kroki_endpoint() == lang)
                .ok_or(()),
        }
    }
}

/// Output format for rendered diagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DiagramFormat {
    /// SVG text (default, supports links and interactivity).
    #[default]
    Svg,
    /// PNG bytes (smaller for complex diagrams, no interactivity).
    Png,
}

impl FromStr for DiagramFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "svg" => Ok(Self::Svg),
            "png" => Ok(Self::Png),
            _ => Err(()),
        }
    }
}

/// The spelling Kroki's URL path and the diagram digest use.
impl fmt::Display for DiagramFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Svg => "svg",
            Self::Png => "png",
        })
    }
}

#[cfg(test)]
mod tests {
    use rw_diagrams::diagram_language;

    use super::*;

    #[test]
    fn test_plantuml_preprocessing_flag() {
        assert!(DiagramLanguage::PlantUml.needs_plantuml_preprocessing());
        assert!(DiagramLanguage::C4PlantUml.needs_plantuml_preprocessing());
        assert!(!DiagramLanguage::Mermaid.needs_plantuml_preprocessing());
        assert!(!DiagramLanguage::GraphViz.needs_plantuml_preprocessing());
    }

    /// The endpoint spellings are part of the diagram digest, so they are
    /// spelled out here rather than derived from `kroki_endpoint`.
    #[test]
    fn test_every_language_name() {
        let languages = [
            ("plantuml", DiagramLanguage::PlantUml),
            ("c4plantuml", DiagramLanguage::C4PlantUml),
            ("mermaid", DiagramLanguage::Mermaid),
            ("graphviz", DiagramLanguage::GraphViz),
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
            ("structurizr", DiagramLanguage::Structurizr),
        ];
        assert_eq!(languages.len(), DiagramLanguage::ALL.len());

        for (name, language) in languages {
            assert_eq!(language.kroki_endpoint(), name);
            assert_eq!(name.parse(), Ok(language), "{name}");
            assert_eq!(
                format!("kroki-{name}").parse(),
                Ok(language),
                "kroki-{name}"
            );
        }
        assert_eq!("dot".parse(), Ok(DiagramLanguage::GraphViz));
        assert_eq!("kroki-dot".parse(), Ok(DiagramLanguage::GraphViz));
    }

    #[test]
    fn every_diagram_file_language_is_a_kroki_language() {
        for path in [
            "a.puml", "a.mmd", "a.gv", "a.ditaa", "a.bob", "a.noml", "a.er", "a.vg", "a.vl",
        ] {
            let language = diagram_language(path).unwrap();
            assert!(language.parse::<DiagramLanguage>().is_ok(), "{path}");
        }
    }

    #[test]
    fn only_nomnoml_svgbob_and_wavedrom_lack_png() {
        let svg_only = [
            DiagramLanguage::Nomnoml,
            DiagramLanguage::Svgbob,
            DiagramLanguage::WaveDrom,
        ];
        for language in DiagramLanguage::ALL {
            assert_eq!(
                language.renders_png(),
                !svg_only.contains(&language),
                "{language:?}",
            );
        }
    }

    #[test]
    fn test_kroki_prefix_unknown_language() {
        assert_eq!("kroki-unknown".parse::<DiagramLanguage>(), Err(()));
        assert_eq!("kroki-".parse::<DiagramLanguage>(), Err(()));
    }

    #[test]
    fn test_diagram_format_round_trips() {
        for format in [DiagramFormat::Svg, DiagramFormat::Png] {
            assert_eq!(format.to_string().parse(), Ok(format));
        }
        assert_eq!(DiagramFormat::Svg.to_string(), "svg");
        assert_eq!(DiagramFormat::Png.to_string(), "png");
        for unknown in ["img", "jpeg", ""] {
            assert_eq!(unknown.parse::<DiagramFormat>(), Err(()), "{unknown}");
        }
    }
}
