//! Diagram caching infrastructure.
//!
//! Provides a trait for diagram caching and implementations:
//! - [`DiagramCache`]: Trait for cache implementations
//! - [`NullCache`]: No-op cache (disabled caching)
//! - [`FileCache`]: File-based cache for Rust-only usage
//! - [`compute_diagram_hash`]: Content-based hash for cache keys

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::consts::DEFAULT_DPI;

/// Trait for diagram caching implementations.
///
/// Implementations must be thread-safe (`Send + Sync`) for use with parallel rendering.
pub trait DiagramCache: Send + Sync {
    /// Retrieve a cached diagram by content hash.
    ///
    /// # Arguments
    /// * `hash` - SHA-256 hash of diagram content
    /// * `format` - Output format ("svg" or "png")
    ///
    /// # Returns
    /// Cached content if found, `None` otherwise.
    fn get(&self, hash: &str, format: &str) -> Option<String>;

    /// Store a rendered diagram in the cache.
    ///
    /// # Arguments
    /// * `hash` - SHA-256 hash of diagram content
    /// * `format` - Output format ("svg" or "png")
    /// * `content` - Rendered diagram (SVG string or PNG data URI)
    fn set(&self, hash: &str, format: &str, content: &str);
}

/// No-op cache implementation.
///
/// Always returns cache misses and discards stored content.
/// Use when caching is disabled.
#[derive(Debug, Default, Clone)]
pub struct NullCache;

impl DiagramCache for NullCache {
    fn get(&self, _hash: &str, _format: &str) -> Option<String> {
        None
    }

    fn set(&self, _hash: &str, _format: &str, _content: &str) {}
}

/// File-based diagram cache.
///
/// Stores rendered diagrams as files in a directory.
/// File naming: `{hash}.{format}` (e.g., `abc123.svg`).
#[derive(Debug, Clone)]
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
    fn get(&self, hash: &str, format: &str) -> Option<String> {
        let path = self.cache_dir.join(format!("{hash}.{format}"));
        fs::read_to_string(path).ok()
    }

    fn set(&self, hash: &str, format: &str, content: &str) {
        let path = self.cache_dir.join(format!("{hash}.{format}"));
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // Silently ignore write errors - caching is non-critical and cache
        // misses are handled gracefully by re-rendering via Kroki.
        let _ = fs::write(path, content);
    }
}

impl DiagramCache for Arc<dyn DiagramCache> {
    fn get(&self, hash: &str, format: &str) -> Option<String> {
        (**self).get(hash, format)
    }

    fn set(&self, hash: &str, format: &str, content: &str) {
        (**self).set(hash, format, content);
    }
}

/// Compute a content hash for diagram caching.
///
/// The hash is computed from the combination of endpoint, format, DPI, and source.
/// This ensures that changes to any of these parameters result in a cache miss.
///
/// # Hash Format
///
/// SHA-256 of `"{endpoint}:{format}:{dpi}:{source}"`.
///
/// This matches the Python implementation in `cache.py` for seamless migration.
///
/// # Arguments
/// * `source` - Diagram source code (after preprocessing)
/// * `endpoint` - Kroki endpoint (e.g., "plantuml", "mermaid")
/// * `format` - Output format ("svg" or "png")
/// * `dpi` - DPI used for rendering (None = default 192)
///
/// # Returns
/// Hex-encoded SHA-256 hash string.
#[must_use]
pub fn compute_diagram_hash(source: &str, endpoint: &str, format: &str, dpi: Option<u32>) -> String {
    let dpi = dpi.unwrap_or(DEFAULT_DPI);
    let content = format!("{endpoint}:{format}:{dpi}:{source}");
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_cache_always_misses() {
        let cache = NullCache;
        assert!(cache.get("abc123", "svg").is_none());

        cache.set("abc123", "svg", "<svg></svg>");
        assert!(cache.get("abc123", "svg").is_none());
    }

    #[test]
    fn test_file_cache() {
        let temp_dir = std::env::temp_dir().join("docstage_test_cache");
        let _ = fs::remove_dir_all(&temp_dir);

        let cache = FileCache::new(temp_dir.clone());

        // Initially empty
        assert!(cache.get("abc123", "svg").is_none());

        // Store and retrieve
        cache.set("abc123", "svg", "<svg>test</svg>");
        assert_eq!(
            cache.get("abc123", "svg"),
            Some("<svg>test</svg>".to_string())
        );

        // Different format is a miss
        assert!(cache.get("abc123", "png").is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_compute_diagram_hash() {
        let hash1 = compute_diagram_hash("@startuml\nA -> B\n@enduml", "plantuml", "svg", None);
        let hash2 = compute_diagram_hash("@startuml\nA -> B\n@enduml", "plantuml", "svg", None);
        let hash3 = compute_diagram_hash("@startuml\nC -> D\n@enduml", "plantuml", "svg", None);

        // Same inputs produce same hash
        assert_eq!(hash1, hash2);
        // Different source produces different hash
        assert_ne!(hash1, hash3);
        // Hash is 64 hex characters (256 bits)
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_compute_diagram_hash_dpi_matters() {
        let hash_192 = compute_diagram_hash("source", "plantuml", "svg", Some(192));
        let hash_96 = compute_diagram_hash("source", "plantuml", "svg", Some(96));

        assert_ne!(hash_192, hash_96);
    }

    #[test]
    fn test_compute_diagram_hash_format_matters() {
        let hash_svg = compute_diagram_hash("source", "plantuml", "svg", None);
        let hash_png = compute_diagram_hash("source", "plantuml", "png", None);

        assert_ne!(hash_svg, hash_png);
    }

    #[test]
    fn test_compute_diagram_hash_format() {
        // Verify hash format: hex-encoded SHA-256 (64 characters)
        // Hash algorithm: sha256("{endpoint}:{format}:{dpi}:{source}")
        // This matches Python's implementation for cache compatibility
        let hash = compute_diagram_hash("test source", "plantuml", "svg", None);

        assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );
    }
}
