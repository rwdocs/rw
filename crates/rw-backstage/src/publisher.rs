//! Backstage bundle publisher.
//!
//! Scans local documentation, resolves PlantUML includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::path::PathBuf;
use std::sync::LazyLock;

use aws_sdk_s3::Client;
use regex::Regex;
use rw_diagrams::resolve_plantuml_includes;
use rw_storage::{Document, Storage};

use crate::format::{self, Manifest, ManifestDocument, PageBundle};

static PLANTUML_FENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\s*`{3,})\s*(?:plantuml|puml|c4plantuml)\b[^\n]*\n").unwrap()
});

/// Configuration for publishing documentation bundles to S3.
#[derive(Debug, Clone)]
pub struct PublishConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// Backstage entity identifier (e.g., `"default/Component/arch"`).
    pub entity: String,
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
pub struct BackstagePublisher {
    config: PublishConfig,
}

impl BackstagePublisher {
    #[must_use]
    pub fn new(config: PublishConfig) -> Self {
        Self { config }
    }

    /// Publish documentation from a storage backend to S3.
    ///
    /// Scans the storage for documents, builds bundles with pre-resolved
    /// PlantUML includes, and uploads everything to S3.
    ///
    /// Returns the number of files uploaded.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> Result<usize, PublishError> {
        let client = self.build_client().await;
        let documents = storage.scan()?;

        let manifest = build_manifest(&documents);
        let manifest_json = serde_json::to_vec(&manifest)?;
        self.upload(&client, "manifest.json", manifest_json, "application/json")
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
            self.upload(&client, &key, bundle_json, "application/json")
                .await?;

            uploaded += 1;
            tracing::debug!(path = %doc.path, "Published page bundle");
        }

        Ok(uploaded)
    }

    async fn build_client(&self) -> Client {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.config.region.clone()));

        if let Some(endpoint) = &self.config.endpoint {
            loader = loader.endpoint_url(endpoint);
        }

        let sdk_config = loader.load().await;

        if self.config.endpoint.is_some() {
            let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                .force_path_style(true)
                .build();
            return Client::from_conf(s3_config);
        }

        Client::new(&sdk_config)
    }

    async fn upload(
        &self,
        client: &Client,
        relative_key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), PublishError> {
        let key = self.build_key(relative_key);
        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .body(body.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| PublishError::S3(error_chain(&e)))?;
        tracing::debug!(key = %key, "Uploaded");
        Ok(())
    }

    fn build_key(&self, relative_path: &str) -> String {
        let mut parts = Vec::new();
        if let Some(root) = &self.config.bucket_root_path {
            parts.push(root.as_str());
        }
        parts.push(&self.config.entity);
        parts.push(relative_path);
        parts.join("/")
    }
}

/// Build a manifest from scanned documents.
fn build_manifest(documents: &[Document]) -> Manifest {
    let docs = documents
        .iter()
        .map(|d| ManifestDocument {
            path: d.path.clone(),
            title: d.title.clone(),
            has_content: d.has_content,
            page_type: d.page_type.clone(),
            description: d.description.clone(),
        })
        .collect();
    Manifest::new(docs)
}

/// Resolve PlantUML `!include` directives within markdown code fences.
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
            let resolved = resolve_plantuml_includes(code_content, include_dirs);
            result.push_str(&resolved);
            remaining = &remaining[close_pos..];
        } else {
            result.push_str(remaining);
            return result;
        }
    }

    result.push_str(remaining);
    result
}

/// Find the position of a closing fence in the remaining text.
fn find_closing_fence(text: &str, fence: &str) -> Option<usize> {
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == fence {
            let offset: usize = text.lines().take(i).map(|l| l.len() + 1).sum();
            return Some(offset);
        }
    }
    None
}

fn error_chain(err: &dyn std::error::Error) -> String {
    let mut msgs = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        msgs.push(s.to_string());
        source = s.source();
    }
    msgs.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_manifest() {
        let documents = vec![
            Document {
                path: String::new(),
                title: "Home".to_owned(),
                has_content: true,
                page_type: None,
                description: None,
            },
            Document {
                path: "guide".to_owned(),
                title: "Guide".to_owned(),
                has_content: true,
                page_type: Some("guide".to_owned()),
                description: Some("A guide".to_owned()),
            },
        ];

        let manifest = build_manifest(&documents);

        assert_eq!(manifest.version, format::FORMAT_VERSION);
        assert_eq!(manifest.documents.len(), 2);
        assert_eq!(manifest.documents[0].path, "");
        assert_eq!(manifest.documents[1].page_type, Some("guide".to_owned()));
    }

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
