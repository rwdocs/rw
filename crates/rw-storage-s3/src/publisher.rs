//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::path::PathBuf;
use std::sync::LazyLock;

use aws_sdk_s3::Client;
use regex::Regex;
use rw_diagrams::resolve_plantuml_includes;
use rw_storage::Storage;

use crate::format::{self, Manifest, PageBundle};
use crate::s3::{self, S3Config};

static PLANTUML_FENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\s*`{3,})\s*(?:plantuml|puml|c4plantuml)\b[^\n]*\n").unwrap()
});

/// Configuration for publishing documentation bundles to S3.
#[derive(Debug, Clone)]
pub struct PublishConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// S3 key prefix (e.g., `"default/Component/arch"`).
    pub prefix: String,
    /// AWS region (default: `"us-east-1"`).
    pub region: String,
    /// Optional S3-compatible endpoint URL.
    pub endpoint: Option<String>,
    /// Optional prefix path within the bucket.
    pub bucket_root_path: Option<String>,
}

/// Errors that can occur during publishing.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("Storage error: {0}")]
    Storage(#[from] rw_storage::StorageError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Publisher that builds and uploads documentation bundles to S3.
pub struct BundlePublisher {
    config: PublishConfig,
}

impl BundlePublisher {
    #[must_use]
    pub fn new(config: PublishConfig) -> Self {
        Self { config }
    }

    /// Publish documentation from a storage backend to S3.
    ///
    /// Scans the storage for documents, builds bundles with pre-resolved
    /// `PlantUML` includes, and uploads everything to S3.
    ///
    /// Returns the number of files uploaded.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> Result<usize, PublishError> {
        let s3_config = self.s3_config();
        let client = s3::build_client(&s3_config).await;
        let documents = storage.scan()?;

        let manifest = Manifest::new(documents.clone());
        let manifest_json = serde_json::to_vec(&manifest)?;
        self.upload(
            &client,
            &s3_config,
            "manifest.json",
            manifest_json,
            "application/json",
        )
        .await?;

        let mut uploaded = 1; // manifest

        for doc in &documents {
            if !doc.has_content {
                continue;
            }

            let content = storage.read(&doc.path)?;
            let resolved_content = resolve_markdown_includes(&content, include_dirs);
            let metadata = storage.meta(&doc.path)?;

            let bundle = PageBundle {
                content: resolved_content,
                metadata,
            };

            let bundle_json = serde_json::to_vec(&bundle)?;
            let key = format::page_bundle_key(&doc.path);
            self.upload(&client, &s3_config, &key, bundle_json, "application/json")
                .await?;

            uploaded += 1;
            tracing::debug!(path = %doc.path, "Published page bundle");
        }

        Ok(uploaded)
    }

    fn s3_config(&self) -> S3Config {
        S3Config {
            region: self.config.region.clone(),
            endpoint: self.config.endpoint.clone(),
            bucket_root_path: self.config.bucket_root_path.clone(),
            prefix: self.config.prefix.clone(),
        }
    }

    async fn upload(
        &self,
        client: &Client,
        s3_config: &S3Config,
        relative_key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), PublishError> {
        let key = s3::build_key(s3_config, relative_key);
        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .body(body.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| PublishError::S3(s3::error_chain(&e)))?;
        tracing::debug!(key = %key, "Uploaded");
        Ok(())
    }
}

/// Resolve `PlantUML` `!include` directives within markdown code fences.
///
/// Finds all plantuml/puml/c4plantuml code blocks and resolves `!include`
/// directives by reading files from `include_dirs`.
fn resolve_markdown_includes(markdown: &str, include_dirs: &[PathBuf]) -> String {
    if include_dirs.is_empty() || !markdown.contains("!include") {
        return markdown.to_owned();
    }

    let mut result = String::with_capacity(markdown.len());
    let mut remaining = markdown;

    while let Some(fence_match) = PLANTUML_FENCE.find(remaining) {
        result.push_str(&remaining[..fence_match.end()]);
        remaining = &remaining[fence_match.end()..];

        let fence_line = fence_match.as_str();
        let backtick_count = fence_line
            .trim_start()
            .chars()
            .take_while(|&c| c == '`')
            .count();
        let closing_fence = "`".repeat(backtick_count);

        if let Some(close_pos) = find_closing_fence(remaining, &closing_fence) {
            let code_content = &remaining[..close_pos];
            let resolve_result = resolve_plantuml_includes(code_content, include_dirs);
            for warning in &resolve_result.warnings {
                tracing::warn!("{warning}");
            }
            result.push_str(&resolve_result.source);
            remaining = &remaining[close_pos..];
        } else {
            result.push_str(remaining);
            return result;
        }
    }

    result.push_str(remaining);
    result
}

/// Find the byte position of a closing fence in the remaining text.
fn find_closing_fence(text: &str, fence: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.lines() {
        if line.trim() == fence {
            return Some(offset);
        }
        // Advance past this line + its newline character.
        // If the line doesn't end with '\n' (last line), we still advance past it.
        offset += line.len();
        if text.as_bytes().get(offset) == Some(&b'\n') {
            offset += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_markdown_includes_no_plantuml() {
        let md = "# Hello\n\nSome text\n\n```rust\nfn main() {}\n```\n";
        let result = resolve_markdown_includes(md, &[PathBuf::from("/tmp")]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_resolve_markdown_includes_no_include_dirs() {
        let md = "```plantuml\n@startuml\n!include foo.puml\n@enduml\n```\n";
        let result = resolve_markdown_includes(md, &[]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_resolve_markdown_includes_no_include_directive() {
        let md = "```plantuml\n@startuml\nA -> B\n@enduml\n```\n";
        let result = resolve_markdown_includes(md, &[PathBuf::from("/tmp")]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_find_closing_fence_basic() {
        let text = "@startuml\nA -> B\n@enduml\n```\nmore text";
        let pos = find_closing_fence(text, "```");
        assert!(pos.is_some());
        assert_eq!(&text[pos.unwrap()..].lines().next().unwrap(), &"```");
    }

    #[test]
    fn test_find_closing_fence_none() {
        let text = "@startuml\nA -> B\n@enduml\n";
        let pos = find_closing_fence(text, "```");
        assert!(pos.is_none());
    }
}
