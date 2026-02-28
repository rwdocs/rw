//! S3 publishing for `TechDocs` sites.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rw_storage_s3::S3Config;
use rw_storage_s3::s3;

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
    config: S3Config,
}

impl S3Publisher {
    /// Create a new publisher with the given configuration.
    #[must_use]
    pub fn new(config: S3Config) -> Self {
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
        let client = s3::build_client(&self.config).await;

        for (relative_path, abs_path) in &files {
            let content_type = guess_content_type(relative_path);
            let body = fs::read(abs_path)?;

            s3::upload(&client, &self.config, relative_path, body, content_type)
                .await
                .map_err(PublishError::S3)?;
        }

        Ok(files.len())
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
        let config = S3Config {
            bucket: "bucket".to_owned(),
            prefix: "default/Component/arch".to_owned(),
            endpoint: None,
            region: "us-east-1".to_owned(),
            bucket_root_path: None,
        };
        assert_eq!(
            s3::build_key(&config, "index.html"),
            "default/Component/arch/index.html"
        );
    }

    #[test]
    fn build_key_with_root_path() {
        let config = S3Config {
            bucket: "bucket".to_owned(),
            prefix: "default/Component/arch".to_owned(),
            endpoint: None,
            region: "us-east-1".to_owned(),
            bucket_root_path: Some("techdocs".to_owned()),
        };
        assert_eq!(
            s3::build_key(&config, "index.html"),
            "techdocs/default/Component/arch/index.html"
        );
    }
}
