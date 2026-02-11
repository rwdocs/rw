//! Extension trait for [`CacheBucket`] with typed convenience methods.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::CacheBucket;

/// Typed convenience methods for [`CacheBucket`].
///
/// Provides `get_json`/`set_json` for serde-serializable types and
/// `get_string`/`set_string` for UTF-8 strings. These are implemented
/// as default methods on an extension trait so that:
///
/// - [`CacheBucket`] stays object-safe with no serde dependency
/// - Implementors only need to handle raw bytes
/// - Callers get ergonomic typed access via a blanket impl
///
/// # Example
///
/// ```
/// use rw_cache::{Cache, CacheBucketExt, NullCache};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct PageData { title: String }
///
/// let cache = NullCache;
/// let bucket = cache.bucket("pages");
///
/// bucket.set_json("page", "v1", &PageData { title: "Hello".into() });
/// let data: Option<PageData> = bucket.get_json("page", "v1");
/// ```
pub trait CacheBucketExt: CacheBucket {
    /// Retrieve a JSON-deserialized value from the cache.
    ///
    /// Returns `None` on cache miss, etag mismatch, or deserialization failure.
    fn get_json<T: DeserializeOwned>(&self, key: &str, etag: &str) -> Option<T> {
        let bytes = self.get(key, etag)?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Store a value as JSON in the cache.
    ///
    /// Silently does nothing if serialization fails.
    fn set_json<T: Serialize>(&self, key: &str, etag: &str, value: &T) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            self.set(key, etag, &bytes);
        }
    }

    /// Retrieve a cached UTF-8 string.
    ///
    /// Returns `None` on cache miss, etag mismatch, or invalid UTF-8.
    fn get_string(&self, key: &str, etag: &str) -> Option<String> {
        let bytes = self.get(key, etag)?;
        String::from_utf8(bytes).ok()
    }

    /// Store a string value in the cache.
    fn set_string(&self, key: &str, etag: &str, value: &str) {
        self.set(key, etag, value.as_bytes());
    }
}

impl<B: CacheBucket + ?Sized> CacheBucketExt for B {}
