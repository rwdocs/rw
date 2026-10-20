//! S3-backed cache implementation.
//!
//! Implements [`rw_cache::Cache`] and [`rw_cache::CacheBucket`] traits using S3
//! as the backing store. Cache entries are stored as S3 objects with etags in
//! user metadata (`cache-etag`).

use std::error::Error;

use aws_sdk_s3::Client;
use aws_sdk_s3::operation::get_object::GetObjectError;
use rw_cache::{Cache, CacheBucket};
use tokio::runtime::Handle;

const ETAG_METADATA_KEY: &str = "cache-etag";

/// S3-backed [`Cache`].
///
/// Creates [`CacheBucket`]s that store entries in S3 under
/// `{prefix}/cache/{version}/{bucket_name}/{key}.cache`.
///
/// Requires an existing S3 [`Client`] and tokio runtime [`Handle`],
/// allowing the caller to share these with other S3-backed components.
pub struct S3Cache {
    client: Client,
    runtime: Handle,
    bucket: String,
    prefix: String,
    version: String,
}

impl S3Cache {
    /// Create a new S3 cache using the given client and runtime handle.
    ///
    /// Entries are scoped to `version`: a cache built with another version
    /// neither reads nor overwrites them, so an upgrade that changes rendered
    /// output starts from an empty cache. Unlike [`rw_cache::FileCache`],
    /// entries of other versions are not deleted.
    pub fn new(
        client: Client,
        runtime: Handle,
        bucket: String,
        prefix: String,
        version: &str,
    ) -> Self {
        Self {
            client,
            runtime,
            bucket,
            prefix,
            version: version.to_owned(),
        }
    }
}

impl Cache for S3Cache {
    fn bucket(&self, name: &str) -> Box<dyn CacheBucket> {
        Box::new(S3CacheBucket {
            client: self.client.clone(),
            runtime: self.runtime.clone(),
            s3_bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            version: self.version.clone(),
            bucket_name: name.to_owned(),
        })
    }
}

/// Build the full S3 key for a cache entry.
///
/// Layout: `{prefix}/cache/{version}/{bucket_name}/{key}.cache`
/// Empty prefix omits the leading segment. The suffix keeps the root page's
/// empty key from ending in `/`, which S3 tools show as a folder.
fn build_cache_key(prefix: &str, version: &str, bucket_name: &str, key: &str) -> String {
    if prefix.is_empty() {
        format!("cache/{version}/{bucket_name}/{key}.cache")
    } else {
        format!("{prefix}/cache/{version}/{bucket_name}/{key}.cache")
    }
}

/// S3-backed [`CacheBucket`].
struct S3CacheBucket {
    client: Client,
    runtime: Handle,
    s3_bucket: String,
    prefix: String,
    version: String,
    bucket_name: String,
}

impl CacheBucket for S3CacheBucket {
    fn get(&self, key: &str, etag: &str) -> Option<Vec<u8>> {
        let s3_key = build_cache_key(&self.prefix, &self.version, &self.bucket_name, key);
        self.runtime.block_on(async {
            let resp = self
                .client
                .get_object()
                .bucket(&self.s3_bucket)
                .key(&s3_key)
                .send()
                .await
                .map_err(|e| {
                    if !matches!(e.as_service_error(), Some(GetObjectError::NoSuchKey(_))) {
                        tracing::debug!(key = %s3_key, error = &e as &dyn Error, "S3 cache get failed");
                    }
                })
                .ok()?;

            let stored_etag = resp
                .metadata()
                .and_then(|m| m.get(ETAG_METADATA_KEY))
                .map_or("", String::as_str);
            if stored_etag != etag {
                return None;
            }

            let bytes = resp
                .body
                .collect()
                .await
                .map_err(|e| {
                    tracing::debug!(key = %s3_key, error = &e as &dyn Error, "S3 cache body read failed");
                })
                .ok()?;
            Some(bytes.into_bytes().to_vec())
        })
    }

    fn set(&self, key: &str, etag: &str, value: &[u8]) {
        let s3_key = build_cache_key(&self.prefix, &self.version, &self.bucket_name, key);
        let _ = self.runtime.block_on(async {
            self.client
                .put_object()
                .bucket(&self.s3_bucket)
                .key(&s3_key)
                .body(value.to_vec().into())
                .metadata(ETAG_METADATA_KEY, etag)
                .content_type("application/octet-stream")
                .send()
                .await
                .map_err(|e| {
                    tracing::debug!(key = %s3_key, error = &e as &dyn Error, "S3 cache set failed");
                })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_key_is_built_from_prefix_version_bucket_and_key() {
        let s3_key = build_cache_key("default/Component/arch", "0.1.36", "diagrams", "abc123");
        assert_eq!(
            s3_key,
            "default/Component/arch/cache/0.1.36/diagrams/abc123.cache"
        );
    }

    #[test]
    fn s3_key_handles_empty_prefix() {
        let s3_key = build_cache_key("", "0.1.36", "diagrams", "abc123");
        assert_eq!(s3_key, "cache/0.1.36/diagrams/abc123.cache");
    }

    #[test]
    fn root_page_key_is_not_a_folder() {
        let s3_key = build_cache_key("site", "0.1.36", "pages", "");
        assert!(!s3_key.ends_with('/'), "{s3_key}");
    }

    #[test]
    fn versions_do_not_share_entries() {
        assert_ne!(
            build_cache_key("site", "0.1.36", "pages", "guide"),
            build_cache_key("site", "0.1.37", "pages", "guide"),
        );
    }
}
