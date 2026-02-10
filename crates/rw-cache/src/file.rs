//! File-based cache implementation.
//!
//! [`FileCache`] stores cache entries as files on disk, organized into buckets
//! (subdirectories). Each entry uses a length-prefixed etag header to support
//! binary data:
//!
//! ```text
//! {etag_len}:{etag}{data_bytes}
//! ```
//!
//! On construction, [`FileCache`] validates a `VERSION` file in the cache root.
//! If the version mismatches or is missing, the entire cache directory is wiped
//! and recreated. This ensures stale caches from previous builds are never used.

use std::fs;
use std::path::PathBuf;

use crate::{Bucket, Cache};

/// File-based [`Cache`] rooted at a directory on disk.
///
/// Directory layout:
/// ```text
/// {root}/
/// +-- VERSION            # contains the cache version string
/// +-- pages/             # bucket "pages"
/// |   +-- my-page        # cache entry
/// +-- diagrams/          # bucket "diagrams"
///     +-- ...
/// ```
pub struct FileCache {
    root: PathBuf,
}

impl FileCache {
    /// Create a new file-based cache at `root`, validating the cache version.
    ///
    /// If the `VERSION` file inside `root` does not match `version`, the entire
    /// cache directory is removed and recreated with the new version. Errors
    /// during validation are logged but never fatal.
    #[must_use]
    pub fn new(root: PathBuf, version: &str) -> Self {
        validate_version(&root, version);
        Self { root }
    }
}

impl Cache for FileCache {
    fn bucket(&self, name: &str) -> Box<dyn Bucket> {
        Box::new(FileBucket {
            dir: self.root.join(name),
        })
    }
}

/// A single bucket backed by a directory on disk.
struct FileBucket {
    dir: PathBuf,
}

impl Bucket for FileBucket {
    fn get(&self, key: &str, etag: &str) -> Option<Vec<u8>> {
        let path = self.dir.join(key);
        let bytes = fs::read(&path).ok()?;

        // Parse length-prefixed etag header: {etag_len}:{etag}{data}
        let colon_pos = bytes.iter().position(|&b| b == b':')?;
        let len_str = std::str::from_utf8(&bytes[..colon_pos]).ok()?;
        let etag_len: usize = len_str.parse().ok()?;

        let data_start = colon_pos + 1 + etag_len;
        if bytes.len() < data_start {
            return None;
        }

        // If caller provides a non-empty etag, validate it
        if !etag.is_empty() {
            let stored_etag = std::str::from_utf8(&bytes[colon_pos + 1..data_start]).ok()?;
            if stored_etag != etag {
                return None;
            }
        }

        Some(bytes[data_start..].to_vec())
    }

    fn set(&self, key: &str, etag: &str, value: &[u8]) {
        let path = self.dir.join(key);

        // Silently ignore errors — cache is optional
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }

        let mut buf = Vec::with_capacity(etag.len() + value.len() + 16);
        buf.extend_from_slice(etag.len().to_string().as_bytes());
        buf.push(b':');
        buf.extend_from_slice(etag.as_bytes());
        buf.extend_from_slice(value);

        let _ = fs::write(&path, &buf);
    }
}

/// Validate the cache version, wiping the directory on mismatch.
fn validate_version(root: &PathBuf, version: &str) {
    let version_file = root.join("VERSION");

    // Try to read the existing version
    match fs::read_to_string(&version_file) {
        Ok(stored) if stored == version => {
            // Version matches — keep cache
            tracing::debug!("cache version matches: {version}");
            return;
        }
        Ok(stored) => {
            tracing::info!(
                "cache version mismatch (stored={stored}, current={version}), wiping cache"
            );
        }
        Err(_) => {
            tracing::info!("no cache VERSION file found, initializing cache");
        }
    }

    // Wipe and recreate
    if root.exists()
        && let Err(e) = fs::remove_dir_all(root)
    {
        tracing::warn!("failed to remove cache directory: {e}");
    }
    if let Err(e) = fs::create_dir_all(root) {
        tracing::warn!("failed to create cache directory: {e}");
        return;
    }
    if let Err(e) = fs::write(&version_file, version) {
        tracing::warn!("failed to write cache VERSION file: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_bucket_set_and_get() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        bucket.set("my-page", "etag1", b"<html>hello</html>");
        let result = bucket.get("my-page", "etag1");
        assert_eq!(result, Some(b"<html>hello</html>".to_vec()));
    }

    #[test]
    fn test_file_bucket_etag_match() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        bucket.set("key", "correct-etag", b"data");

        // Matching etag returns data
        assert_eq!(bucket.get("key", "correct-etag"), Some(b"data".to_vec()));

        // Mismatched etag returns None
        assert_eq!(bucket.get("key", "wrong-etag"), None);
    }

    #[test]
    fn test_file_bucket_empty_etag_skips_validation() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        bucket.set("key", "some-etag", b"data");

        // Empty etag on get always returns data regardless of stored etag
        assert_eq!(bucket.get("key", ""), Some(b"data".to_vec()));
    }

    #[test]
    fn test_file_bucket_get_nonexistent_key() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        assert_eq!(bucket.get("nonexistent", "etag"), None);
    }

    #[test]
    fn test_file_bucket_overwrite() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        bucket.set("key", "etag1", b"first");
        bucket.set("key", "etag2", b"second");

        // Old etag misses
        assert_eq!(bucket.get("key", "etag1"), None);
        // New etag hits
        assert_eq!(bucket.get("key", "etag2"), Some(b"second".to_vec()));
    }

    #[test]
    fn test_file_cache_buckets_are_isolated() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");

        let bucket_a = cache.bucket("alpha");
        let bucket_b = cache.bucket("beta");

        bucket_a.set("key", "etag", b"alpha-data");
        bucket_b.set("key", "etag", b"beta-data");

        assert_eq!(bucket_a.get("key", "etag"), Some(b"alpha-data".to_vec()));
        assert_eq!(bucket_b.get("key", "etag"), Some(b"beta-data".to_vec()));
    }

    #[test]
    fn test_file_bucket_nested_key() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        bucket.set("docs/guide/intro", "etag1", b"nested content");
        assert_eq!(
            bucket.get("docs/guide/intro", "etag1"),
            Some(b"nested content".to_vec())
        );
    }

    #[test]
    fn test_file_bucket_binary_data() {
        let tmp = TempDir::new().unwrap();
        let cache = FileCache::new(tmp.path().join("cache"), "v1");
        let bucket = cache.bucket("pages");

        // Binary data including \n, \r, null bytes, and high bytes
        let binary_data: Vec<u8> = vec![0x00, 0x01, 0x0A, 0x0D, 0xFF, 0xFE, 0x80, 0x7F];
        bucket.set("binary", "etag1", &binary_data);
        assert_eq!(bucket.get("binary", "etag1"), Some(binary_data));
    }

    #[test]
    fn test_version_match_keeps_cache() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("cache");

        // Create cache and populate it
        let cache = FileCache::new(root.clone(), "v1");
        let bucket = cache.bucket("pages");
        bucket.set("key", "etag1", b"preserved");

        // Recreate with same version — data persists
        let cache2 = FileCache::new(root, "v1");
        let bucket2 = cache2.bucket("pages");
        assert_eq!(bucket2.get("key", "etag1"), Some(b"preserved".to_vec()));
    }

    #[test]
    fn test_version_mismatch_wipes_cache() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("cache");

        // Create cache and populate it
        let cache = FileCache::new(root.clone(), "v1");
        let bucket = cache.bucket("pages");
        bucket.set("key", "etag1", b"will-be-wiped");

        // Recreate with different version — data gone
        let cache2 = FileCache::new(root.clone(), "v2");
        let bucket2 = cache2.bucket("pages");
        assert_eq!(bucket2.get("key", "etag1"), None);

        // VERSION file updated
        let version = fs::read_to_string(root.join("VERSION")).unwrap();
        assert_eq!(version, "v2");
    }

    #[test]
    fn test_missing_version_file_wipes_cache() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("cache");

        // Manually create cache dir with some orphan file but no VERSION
        fs::create_dir_all(root.join("pages")).unwrap();
        fs::write(root.join("pages/orphan"), b"stale data").unwrap();

        // Construct FileCache — orphan files should be gone
        let cache = FileCache::new(root.clone(), "v1");
        let bucket = cache.bucket("pages");
        assert_eq!(bucket.get("orphan", ""), None);

        // VERSION file created
        let version = fs::read_to_string(root.join("VERSION")).unwrap();
        assert_eq!(version, "v1");
    }

    #[test]
    fn test_nonexistent_root_creates_version() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("deeply/nested/cache");

        // Root doesn't exist yet
        assert!(!root.exists());

        let _cache = FileCache::new(root.clone(), "v1");

        // Directory and VERSION created
        assert!(root.exists());
        let version = fs::read_to_string(root.join("VERSION")).unwrap();
        assert_eq!(version, "v1");
    }
}
