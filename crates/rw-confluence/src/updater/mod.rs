//! Page updater for Confluence.
//!
//! This module provides the [`PageUpdater`] struct that encapsulates the entire
//! workflow for updating a Confluence page from markdown content:
//!
//! 1. Convert markdown to Confluence storage format
//! 2. Fetch current page content
//! 3. Preserve inline comments from current page
//! 4. Upload diagram attachments
//! 5. Update the page
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::Path;
//! use rw_confluence::{ConfluenceClient, PageUpdater, UpdateConfig};
//!
//! let client = ConfluenceClient::from_config(
//!     "https://confluence.example.com",
//!     "consumer_key",
//!     Path::new("private_key.pem"),
//!     "access_token",
//!     "access_secret",
//! )?;
//! let config = UpdateConfig {
//!     kroki_url: Some("https://kroki.io".to_owned()),
//!     include_dirs: vec![],
//!     dpi: 192,
//!     extract_title: true,
//! };
//! let updater = PageUpdater::new(&client, config);
//!
//! // Perform update
//! let result = updater.update("123", "# Title\n\nContent", Some("Update message"))?;
//!
//! // Or dry-run to preview changes
//! let dry_run = updater.dry_run("123", "# Title\n\nContent")?;
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;

mod error;
mod executor;
mod result;

pub use error::UpdateError;
pub use executor::PageUpdater;
pub use result::{DryRunResult, UpdateResult};

/// Configuration for updating a Confluence page from markdown.
pub struct UpdateConfig {
    /// Kroki server URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` directives.
    pub include_dirs: Vec<PathBuf>,
    /// DPI for diagram rendering.
    pub dpi: u32,
    /// Whether to extract title from first H1 heading.
    pub extract_title: bool,
}
