//! S3 publishing for `TechDocs` sites.

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use aws_sdk_s3::Client;

/// Configuration for S3 publishing.
pub struct PublishConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// Backstage entity (e.g. "default/Component/arch").
    pub entity: String,
    /// S3-compatible endpoint URL.
    pub endpoint: Option<String>,
    /// AWS region.
    pub region: String,
    /// Optional prefix path within the bucket.
    pub bucket_root_path: Option<String>,
}

/// Error returned by the publisher.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),
}

/// Publishes a built site directory to S3.
pub struct S3Publisher {
    config: PublishConfig,
}

impl S3Publisher {
    /// Create a new publisher with the given configuration.
    #[must_use]
    pub fn new(config: PublishConfig) -> Self {
        Self { config }
    }

    /// Publish the directory to S3.
    ///
    /// Returns the number of files uploaded.
    pub async fn publish(&self, directory: &Path) -> Result<usize, PublishError> {
        if !directory.is_dir() {
            return Err(PublishError::DirectoryNotFound(directory.to_path_buf()));
        }

        let files = Self::collect_files(directory)?;
        let client = self.build_client().await;

        for (relative_path, abs_path) in &files {
            let key = self.build_key(relative_path);
            let content_type = guess_content_type(relative_path);
            let body = fs::read(abs_path)?;

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
        }

        Ok(files.len())
    }

    async fn build_client(&self) -> Client {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.config.region.clone()));

        if let Some(endpoint) = &self.config.endpoint {
            loader = loader.endpoint_url(endpoint);
        }

        let sdk_config = loader.load().await;

        // Custom endpoints (LocalStack, MinIO, Yandex Cloud) require path-style
        // addressing (e.g. endpoint/bucket/key) instead of the default
        // virtual-hosted-style (bucket.endpoint/key).
        if self.config.endpoint.is_some() {
            let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                .force_path_style(true)
                .build();
            return Client::from_conf(s3_config);
        }

        Client::new(&sdk_config)
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

    fn collect_files(directory: &Path) -> Result<Vec<(String, PathBuf)>, io::Error> {
        let mut files = Vec::new();
        walk_dir(directory, directory, &mut files)?;
        Ok(files)
    }
}

fn walk_dir(
    base: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), io::Error> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(base, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, path));
        }
    }
    Ok(())
}

/// Walk the error source chain and join all messages.
fn error_chain(err: &dyn Error) -> String {
    let mut msgs = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        msgs.push(s.to_string());
        source = s.source();
    }
    msgs.join(": ")
}

fn guess_content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_content_type_html() {
        assert_eq!(guess_content_type("index.html"), "text/html; charset=utf-8");
    }

    #[test]
    fn guess_content_type_css() {
        assert_eq!(
            guess_content_type("assets/styles.css"),
            "text/css; charset=utf-8"
        );
    }

    #[test]
    fn guess_content_type_json() {
        assert_eq!(
            guess_content_type("techdocs_metadata.json"),
            "application/json"
        );
    }

    #[test]
    fn guess_content_type_unknown() {
        assert_eq!(guess_content_type("file.xyz"), "application/octet-stream");
    }

    #[test]
    fn build_key_simple() {
        let publisher = S3Publisher::new(PublishConfig {
            bucket: "bucket".to_owned(),
            entity: "default/Component/arch".to_owned(),
            endpoint: None,
            region: "us-east-1".to_owned(),
            bucket_root_path: None,
        });
        assert_eq!(
            publisher.build_key("index.html"),
            "default/Component/arch/index.html"
        );
    }

    #[test]
    fn build_key_with_root_path() {
        let publisher = S3Publisher::new(PublishConfig {
            bucket: "bucket".to_owned(),
            entity: "default/Component/arch".to_owned(),
            endpoint: None,
            region: "us-east-1".to_owned(),
            bucket_root_path: Some("techdocs".to_owned()),
        });
        assert_eq!(
            publisher.build_key("index.html"),
            "techdocs/default/Component/arch/index.html"
        );
    }
}
