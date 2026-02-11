//! Diagram cache key computation.
//!
//! Provides [`DiagramKey`] for computing content-based hashes used as cache keys.

use sha2::{Digest, Sha256};

/// Diagram parameters for cache key computation.
///
/// Contains all parameters that affect the rendered diagram output.
/// Used to compute a content-based hash for caching.
#[derive(Debug)]
pub struct DiagramKey<'a> {
    /// Diagram source code (after preprocessing).
    pub source: &'a str,
    /// Kroki endpoint (e.g., "plantuml", "mermaid").
    pub endpoint: &'a str,
    /// Output format ("svg" or "png").
    pub format: &'a str,
    /// DPI used for rendering.
    pub dpi: u32,
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
}
