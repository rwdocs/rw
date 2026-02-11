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
//! use rw_config::DiagramsConfig;
//!
//! let client = ConfluenceClient::from_config(
//!     "https://confluence.example.com",
//!     "consumer_key",
//!     Path::new("private_key.pem"),
//!     "access_token",
//!     "access_secret",
//! )?;
//! let config = UpdateConfig {
//!     diagrams: DiagramsConfig::default(),
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

mod error;
mod executor;
mod result;

pub use error::UpdateError;
pub use executor::PageUpdater;
pub use result::{DryRunResult, UpdateResult};

use rw_config::DiagramsConfig;

/// Configuration for updating a Confluence page from markdown.
pub struct UpdateConfig {
    /// Diagram rendering configuration (Kroki URL, include directories, etc.).
    pub diagrams: DiagramsConfig,
    /// Whether to extract title from first H1 heading.
    pub extract_title: bool,
}
