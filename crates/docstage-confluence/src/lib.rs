//! Confluence integration for Docstage.
//!
//! This crate provides complete Confluence functionality:
//! - [`ConfluenceBackend`]: Render backend for Confluence XHTML storage format
//! - [`PageRenderer`]: Markdown to Confluence page rendering with diagrams
//! - [`ConfluenceClient`]: REST API client with OAuth 1.0 authentication
//! - [`PageUpdater`](updater::PageUpdater): Page update workflow with comment preservation
//!
//! # Page Rendering
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_confluence::PageRenderer;
//!
//! let renderer = PageRenderer::new()
//!     .prepend_toc(true)
//!     .extract_title(true);
//!
//! let result = renderer.render(
//!     "# Hello\n\n```plantuml\nA -> B\n```",
//!     Some("https://kroki.io"),
//!     Some(Path::new("/tmp/diagrams")),
//! );
//! ```
//!
//! # API Client
//!
//! ```ignore
//! use docstage_confluence::{ConfluenceClient, oauth};
//!
//! let key = oauth::read_private_key("private_key.pem")?;
//! let client = ConfluenceClient::from_config(
//!     "https://confluence.example.com",
//!     "consumer_key",
//!     &key,
//!     "access_token",
//!     "access_secret",
//! )?;
//!
//! let page = client.get_page("123", &["body.storage"])?;
//! println!("Page title: {}", page.title);
//! ```
//!
//! # Comment Preservation
//!
//! When updating a Confluence page, inline comments need to be preserved. The
//! [`preserve_comments`] function transfers comment markers from the old HTML
//! to the new HTML using tree-based comparison.
//!
//! ```ignore
//! use docstage_confluence::preserve_comments;
//!
//! let old_html = r#"<p><ac:inline-comment-marker ac:ref="abc">text</ac:inline-comment-marker></p>"#;
//! let new_html = "<p>text</p>";
//!
//! let result = preserve_comments(old_html, new_html);
//! assert!(result.html.contains("ac:inline-comment-marker"));
//! ```

// Render backend
mod backend;
pub use backend::ConfluenceBackend;

// Page renderer
mod renderer;
mod tags;
pub use docstage_renderer::RenderResult;
pub use renderer::PageRenderer;

// API client
mod client;
pub use client::ConfluenceClient;

// Comment preservation
mod comment_preservation;
pub use comment_preservation::{PreserveResult, TreeNode, UnmatchedComment, preserve_comments};

// OAuth
pub mod oauth;
pub use oauth::{AccessToken, OAuthTokenGenerator, RequestToken};

// Types
pub mod types;
pub use types::*;

// Page updater
pub mod updater;

// Errors
pub mod error;
pub use error::ConfluenceError;
