//! Diagram caching infrastructure.
//!
//! Provides a trait for diagram caching and implementations:
//! - [`DiagramCache`]: Trait for cache implementations
//! - [`NullCache`]: No-op cache (disabled caching)
//! - [`FileCache`]: File-based cache for Rust-only usage

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Diagram parameters for cache key computation.
///
/// Contains all parameters that affect the rendered diagram output.
/// Used to compute a content-based hash for caching.
#[derive(Debug, Clone, Copy)]
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
    pub(crate) fn compute_hash(&self) -> String {
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

/// Trait for diagram caching implementations.
///
/// Implementations must be thread-safe (`Send + Sync`) for use with parallel rendering.
pub trait DiagramCache: Send + Sync {
    /// Retrieve a cached diagram.
    ///
    /// # Arguments
    /// * `key` - Diagram parameters that uniquely identify the cached content
    ///
    /// # Returns
    /// Cached content if found, `None` otherwise.
    fn get(&self, key: DiagramKey<'_>) -> Option<String>;

    /// Store a rendered diagram in the cache.
    ///
    /// # Arguments
    /// * `key` - Diagram parameters that uniquely identify the content
    /// * `content` - Rendered diagram (SVG string or PNG data URI)
    fn set(&self, key: DiagramKey<'_>, content: &str);
}

/// No-op cache implementation.
///
/// Always returns cache misses and discards stored content.
/// Use when caching is disabled.
#[derive(Debug, Default)]
pub struct NullCache;

impl DiagramCache for NullCache {
    fn get(&self, _key: DiagramKey<'_>) -> Option<String> {
        None
    }

    fn set(&self, _key: DiagramKey<'_>, _content: &str) {}
}

/// File-based diagram cache.
///
/// Stores rendered diagrams as files in a directory.
/// File naming: `{hash}.{format}` (e.g., `abc123.svg`).
#[derive(Debug)]
pub struct FileCache {
    cache_dir: PathBuf,
}

impl FileCache {
    /// Create a new file cache.
    ///
    /// # Arguments
    /// * `cache_dir` - Directory to store cached diagrams
    #[must_use]
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }
}

impl DiagramCache for FileCache {
    fn get(&self, key: DiagramKey<'_>) -> Option<String> {
        let hash = key.compute_hash();
        let path = self.cache_dir.join(format!("{}.{}", hash, key.format));
        fs::read_to_string(path).ok()
    }

    fn set(&self, key: DiagramKey<'_>, content: &str) {
        let hash = key.compute_hash();
        let path = self.cache_dir.join(format!("{}.{}", hash, key.format));
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // Silently ignore write errors - caching is non-critical and cache
        // misses are handled gracefully by re-rendering via Kroki.
        let _ = fs::write(path, content);
    }
}

impl DiagramCache for Arc<dyn DiagramCache> {
    fn get(&self, key: DiagramKey<'_>) -> Option<String> {
        (**self).get(key)
    }

    fn set(&self, key: DiagramKey<'_>, content: &str) {
        (**self).set(key, content);
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
    fn test_null_cache_always_misses() {
        let cache = NullCache;
        let key = make_key("@startuml\nA -> B\n@enduml", "plantuml", "svg");

        assert!(cache.get(key.clone()).is_none());

        cache.set(key.clone(), "<svg></svg>");
        assert!(cache.get(key).is_none());
    }

    #[test]
    fn test_file_cache() {
        let temp_dir = std::env::temp_dir().join("docstage_test_cache");
        let _ = fs::remove_dir_all(&temp_dir);

        let cache = FileCache::new(temp_dir.clone());

        let key_svg = make_key("@startuml\nA -> B\n@enduml", "plantuml", "svg");
        let key_png = DiagramKey {
            format: "png",
            ..key_svg.clone()
        };

        // Initially empty
        assert!(cache.get(key_svg.clone()).is_none());

        // Store and retrieve
        cache.set(key_svg.clone(), "<svg>test</svg>");
        assert_eq!(
            cache.get(key_svg.clone()),
            Some("<svg>test</svg>".to_string())
        );

        // Different format is a miss
        assert!(cache.get(key_png).is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
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
        let key_96 = DiagramKey {
            dpi: 96,
            ..key_192.clone()
        };

        assert_ne!(key_192.compute_hash(), key_96.compute_hash());
    }

    #[test]
    fn test_diagram_key_hash_format_matters() {
        let key_svg = make_key("source", "plantuml", "svg");
        let key_png = DiagramKey {
            format: "png",
            ..key_svg.clone()
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
