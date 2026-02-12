//! Static site builder for TechDocs.

/// Configuration for static site building.
pub struct BuildConfig;

/// Error returned by the static site builder.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Builds a static documentation site from a storage backend.
pub struct StaticSiteBuilder;
