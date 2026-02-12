//! S3 publishing for TechDocs sites.

use std::path::{Path, PathBuf};

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
    Io(#[from] std::io::Error),
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
    pub async fn publish(&self, directory: &Path) -> Result<usize, PublishError> {
        if !directory.is_dir() {
            return Err(PublishError::DirectoryNotFound(directory.to_path_buf()));
        }

        // TODO: Implement S3 upload
        let files = self.collect_files(directory)?;
        Ok(files.len())
    }

    fn collect_files(&self, directory: &Path) -> Result<Vec<(String, PathBuf)>, std::io::Error> {
        let mut files = Vec::new();
        self.walk_dir(directory, directory, &mut files)?;
        Ok(files)
    }

    fn walk_dir(
        &self,
        base: &Path,
        current: &Path,
        files: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), std::io::Error> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.walk_dir(base, &path, files)?;
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
}
