//! Diagram cache key computation.
//!
//! Provides [`DiagramKey`] for computing content-based hashes used as cache keys.

use sha2::{Digest, Sha256};

use crate::language::{DiagramFormat, DiagramLanguage};

/// Diagram parameters for cache key computation.
///
/// Contains all parameters that affect the rendered diagram output.
/// Used to compute a content-based hash for caching.
#[derive(Debug)]
pub(crate) struct DiagramKey<'a> {
    /// Diagram source code (after preprocessing).
    pub source: &'a str,
    /// Kroki endpoint (e.g., "plantuml", "mermaid").
    pub endpoint: &'a str,
    /// Output format ("svg" or "png").
    pub format: &'a str,
    /// DPI used for rendering.
    pub dpi: u32,
}

impl<'a> DiagramKey<'a> {
    /// The cache and attachment key for one render.
    ///
    /// `source` is the prepared source — after `PlantUML` include resolution
    /// and config injection — because that is what determines the bytes.
    ///
    /// The DPI in the key is [`DiagramLanguage::render_dpi`], the DPI the
    /// output is actually rendered at. Do NOT key on a user-configurable DPI
    /// instead: a `PlantUML` source already carries its injected `skinparam
    /// dpi`, and for every other language DPI changes nothing about the bytes
    /// — a raw setting in the key renames byte-identical Confluence
    /// attachments on every change.
    ///
    /// Prefer this over a [`DiagramKey`] literal, so endpoint and DPI cannot be
    /// chosen out of step with the language, and `format` narrows to the two
    /// spellings Kroki is ever asked for.
    pub(crate) fn for_render(
        source: &'a str,
        language: DiagramLanguage,
        format: DiagramFormat,
    ) -> Self {
        Self {
            source,
            endpoint: language.kroki_endpoint(),
            dpi: language.render_dpi(),
            format: format.as_str(),
        }
    }
}

impl DiagramKey<'_> {
    /// Compute a content hash for this diagram key.
    ///
    /// The hash is computed from the combination of endpoint, format, DPI, and source.
    /// This ensures that changes to any of these parameters result in a cache miss.
    ///
    /// # Hash Format
    ///
    /// SHA-256 of `"{endpoint}:{format}:{dpi}:{source}"`.
    #[must_use]
    pub fn compute_hash(&self) -> String {
        let content = format!(
            "{}:{}:{}:{}",
            self.endpoint, self.format, self.dpi, self.source
        );
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::DEFAULT_DPI;

    fn make_key<'a>(source: &'a str, endpoint: &'a str, format: &'a str) -> DiagramKey<'a> {
        DiagramKey {
            source,
            endpoint,
            format,
            dpi: DEFAULT_DPI,
        }
    }

    #[test]
    fn test_diagram_key_hash() {
        let key1 = make_key("@startuml\nA -> B\n@enduml", "plantuml", "svg");
        let key2 = make_key("@startuml\nA -> B\n@enduml", "plantuml", "svg");
        let key3 = make_key("@startuml\nC -> D\n@enduml", "plantuml", "svg");

        // Same inputs produce same hash
        assert_eq!(key1.compute_hash(), key2.compute_hash());
        // Different source produces different hash
        assert_ne!(key1.compute_hash(), key3.compute_hash());
        // Hash is 64 hex characters (256 bits)
        assert_eq!(key1.compute_hash().len(), 64);
    }

    #[test]
    fn test_diagram_key_hash_dpi_matters() {
        let key_192 = DiagramKey {
            source: "source",
            endpoint: "plantuml",
            format: "svg",
            dpi: 192,
        };
        let key_96 = DiagramKey { dpi: 96, ..key_192 };

        assert_ne!(key_192.compute_hash(), key_96.compute_hash());
    }

    #[test]
    fn test_diagram_key_hash_format_matters() {
        let key_svg = make_key("source", "plantuml", "svg");
        let key_png = DiagramKey {
            format: "png",
            ..key_svg
        };

        assert_ne!(key_svg.compute_hash(), key_png.compute_hash());
    }

    #[test]
    fn test_diagram_key_hash_format() {
        // Verify hash format: hex-encoded SHA-256 (64 characters)
        // Hash algorithm: sha256("{endpoint}:{format}:{dpi}:{source}")
        // This matches Python's implementation for cache compatibility
        let key = make_key("test source", "plantuml", "svg");
        let hash = key.compute_hash();

        assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );
    }

    /// Confluence attachments are named `diagram_<hash12>.png` from this key, so
    /// the hash is part of rw's published output: changing it renames every
    /// attachment on the next publish, and Confluence keeps the old ones too.
    ///
    /// Built through [`DiagramKey::for_render`] — the constructor production
    /// code uses — rather than a hand-built `DiagramKey`, so this guards the
    /// actual composition (which endpoint/DPI a language maps to), not just
    /// the hash formula.
    #[test]
    fn png_attachment_hash_is_stable_for_a_known_source() {
        let key = DiagramKey::for_render(
            "@startuml\nAlice -> Bob\n@enduml",
            DiagramLanguage::PlantUml,
            DiagramFormat::Png,
        );
        assert_eq!(&key.compute_hash()[..12], "07399b44cd31");
    }

    /// A caller must not be able to collapse the SVG and PNG renders of one
    /// diagram onto a single cache entry / attachment: `for_render` has to
    /// actually thread `format` through, not just accept it.
    #[test]
    fn for_render_hashes_svg_and_png_differently() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let svg = DiagramKey::for_render(source, DiagramLanguage::PlantUml, DiagramFormat::Svg);
        let png = DiagramKey::for_render(source, DiagramLanguage::PlantUml, DiagramFormat::Png);
        assert_ne!(svg.compute_hash(), png.compute_hash());
    }

    /// Two languages that Kroki treats differently (distinct endpoint, and —
    /// for `PlantUML` vs. Mermaid — distinct render DPI) must not collide on
    /// the same key: `for_render` has to read both from `language`, not
    /// default one of them.
    #[test]
    fn for_render_hashes_differ_by_language() {
        let source = "A -> B";
        let plantuml =
            DiagramKey::for_render(source, DiagramLanguage::PlantUml, DiagramFormat::Png);
        let mermaid = DiagramKey::for_render(source, DiagramLanguage::Mermaid, DiagramFormat::Png);
        assert_ne!(plantuml.compute_hash(), mermaid.compute_hash());
    }

    /// Mermaid and `GraphViz` render at the same DPI (both are outside the
    /// `PlantUML` family), so unlike the previous test this isolates the
    /// endpoint alone: if `for_render` ever hardcoded or misread the
    /// endpoint, this is the test that would still catch it even though the
    /// DPI component of the key stayed the same.
    #[test]
    fn for_render_hashes_differ_by_endpoint_at_the_same_dpi() {
        let source = "A -> B";
        let mermaid = DiagramKey::for_render(source, DiagramLanguage::Mermaid, DiagramFormat::Png);
        let graphviz =
            DiagramKey::for_render(source, DiagramLanguage::GraphViz, DiagramFormat::Png);
        assert_ne!(mermaid.compute_hash(), graphviz.compute_hash());
    }
}
