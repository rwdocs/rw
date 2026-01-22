//! Confluence integration for Docstage.
//!
//! This crate provides complete Confluence functionality:
//! - [`ConfluenceClient`]: REST API client with OAuth 1.0 authentication
//! - [`PageUpdater`]: Page update workflow with comment preservation
//! - [`OAuthTokenGenerator`]: Three-legged OAuth flow for token generation
//!
//! # Example
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_confluence::{ConfluenceClient, PageUpdater, UpdateConfig};
//!
//! let client = ConfluenceClient::from_config(
//!     "https://confluence.example.com",
//!     "consumer_key",
//!     Path::new("private_key.pem"),
//!     "access_token",
//!     "access_secret",
//! )?;
//!
//! let config = UpdateConfig { /* ... */ };
//! let updater = PageUpdater::new(&client, config);
//! let result = updater.update("123", "# Title\n\nContent", Some("Update"))?;
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
mod oauth;
pub use oauth::{AccessToken, OAuthTokenGenerator, RequestToken};

// Types (internal, exposed via result structs)
mod types;

// Page updater
mod updater;
pub use updater::{DryRunResult, PageUpdater, UpdateConfig, UpdateError, UpdateResult};

// Errors
mod error;
pub use error::ConfluenceError;
