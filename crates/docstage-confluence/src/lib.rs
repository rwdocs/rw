//! Confluence integration for Docstage.
//!
//! This crate provides complete Confluence functionality:
//! - [`ConfluenceClient`]: REST API client with OAuth 1.0 authentication
//! - [`PageUpdater`](updater::PageUpdater): Page update workflow with comment preservation
//!
//! # API Client
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_confluence::ConfluenceClient;
//!
//! let client = ConfluenceClient::from_config(
//!     "https://confluence.example.com",
//!     "consumer_key",
//!     Path::new("private_key.pem"),
//!     "access_token",
//!     "access_secret",
//! )?;
//!
//! let page = client.get_page("123", &["body.storage"])?;
//! println!("Page title: {}", page.title);
//! ```

// Render backend (internal)
mod backend;

// Page renderer (internal)
mod renderer;
mod tags;

// API client
mod client;
pub use client::ConfluenceClient;

// Comment preservation (internal, used by PageUpdater)
mod comment_preservation;
pub use comment_preservation::UnmatchedComment;

// OAuth
pub mod oauth;

// Types (internal, exposed via result structs)
mod types;

// Page updater
pub mod updater;

// Errors
pub mod error;
pub use error::ConfluenceError;
