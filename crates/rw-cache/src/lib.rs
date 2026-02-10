//! Cache abstraction layer for RW.
//!
//! This crate provides generic caching traits that decouple cache consumers
//! from the underlying storage mechanism. Two traits form the core API:
//!
//! - [`Cache`]: Factory for named cache buckets
//! - [`CacheBucket`]: Key-value store with etag-based invalidation
//!
//! # Implementations
//!
//! - [`NullCache`] / [`NullCacheBucket`]: No-op implementations (always miss)
//! - [`FileCache`]: File-based implementation with version validation
//!
//! # Example
//!
//! ```
//! use rw_cache::{Cache, NullCache};
//!
//! let cache = NullCache;
//! let bucket = cache.bucket("pages");
//! bucket.set("my-page", "v1", b"<html>hello</html>");
//! assert_eq!(bucket.get("my-page", "v1"), None); // NullCache always misses
//! ```

mod file;
pub use file::FileCache;

/// A named partition within a [`Cache`].
///
/// Each bucket stores key-value pairs where values are invalidated by an etag.
/// The etag is an opaque string chosen by the caller (e.g., a file mtime, content
/// hash, or version string). A cache hit occurs only when both the key and etag
/// match.
pub trait CacheBucket: Send + Sync {
    /// Retrieve a cached value.
    ///
    /// Returns `Some(value)` if the key exists **and** was stored with the same
    /// `etag`. Returns `None` on cache miss or etag mismatch.
    ///
    /// If `etag` is an empty string, etag validation is skipped and the cached
    /// data is returned regardless of the stored etag.
    ///
    /// # Arguments
    ///
    /// * `key` - Cache key (e.g., document path)
    /// * `etag` - Expected etag for cache validity (empty string skips validation)
    fn get(&self, key: &str, etag: &str) -> Option<Vec<u8>>;

    /// Store a value in the cache.
    ///
    /// Overwrites any existing entry for the same key, regardless of the
    /// previous etag.
    ///
    /// # Arguments
    ///
    /// * `key` - Cache key (e.g., document path)
    /// * `etag` - Etag to associate with this entry
    /// * `value` - Raw bytes to cache
    fn set(&self, key: &str, etag: &str, value: &[u8]);
}

/// Factory for named cache [`CacheBucket`]s.
///
/// A `Cache` produces buckets that are logically isolated from each other.
/// For example, a file-based cache might store each bucket in a separate
/// subdirectory.
pub trait Cache: Send + Sync {
    /// Open or create a named bucket.
    ///
    /// Calling `bucket` multiple times with the same name may return
    /// independent handles that share the same underlying storage.
    ///
    /// # Arguments
    ///
    /// * `name` - Bucket name (e.g., "pages", "diagrams", "site")
    fn bucket(&self, name: &str) -> Box<dyn CacheBucket>;
}

/// No-op [`CacheBucket`] that never stores or retrieves data.
///
/// Every `get` returns `None`; every `set` is silently discarded.
/// Used as the bucket type for [`NullCache`].
pub struct NullCacheBucket;

impl CacheBucket for NullCacheBucket {
    fn get(&self, _key: &str, _etag: &str) -> Option<Vec<u8>> {
        None
    }

    fn set(&self, _key: &str, _etag: &str, _value: &[u8]) {}
}

/// No-op [`Cache`] that always returns [`NullCacheBucket`]s.
///
/// Use when caching is disabled. All operations are no-ops and all lookups
/// return `None`.
pub struct NullCache;

impl Cache for NullCache {
    fn bucket(&self, _name: &str) -> Box<dyn CacheBucket> {
        Box::new(NullCacheBucket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_cache_always_misses() {
        let cache = NullCache;
        let bucket = cache.bucket("pages");

        // A fresh bucket has no data
        assert_eq!(bucket.get("key", "etag1"), None);

        // Setting a value and reading it back still returns None
        bucket.set("key", "etag1", b"hello");
        assert_eq!(bucket.get("key", "etag1"), None);
    }

    #[test]
    fn test_null_cache_different_buckets_all_miss() {
        let cache = NullCache;

        for name in &["pages", "diagrams", "site", "meta"] {
            let bucket = cache.bucket(name);
            bucket.set("k", "v", b"data");
            assert_eq!(bucket.get("k", "v"), None, "bucket {name} should miss");
        }
    }
}
