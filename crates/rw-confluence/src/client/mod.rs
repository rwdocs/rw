//! Confluence REST API client.
//!
//! Provides sync HTTP client for Confluence Server/Data Center REST API
//! with OAuth 1.0 RSA-SHA1 authentication.

mod attachments;
mod comments;
mod pages;

use std::path::Path;
use std::time::Duration;

use ureq::Agent;

use crate::error::ConfluenceError;
use crate::oauth::OAuth1Auth;
use crate::oauth::key::load_private_key_from_file;

/// Default HTTP timeout in seconds.
const DEFAULT_TIMEOUT: u64 = 30;

/// Confluence REST API client.
pub struct ConfluenceClient {
    agent: Agent,
    base_url: String,
    auth: OAuth1Auth,
}

impl ConfluenceClient {
    /// Create client from config values.
    ///
    /// # Arguments
    /// * `base_url` - Confluence server base URL
    /// * `consumer_key` - OAuth consumer key
    /// * `key_file` - Path to RSA private key file (PEM format)
    /// * `access_token` - OAuth access token
    /// * `access_secret` - OAuth access token secret
    ///
    /// # Errors
    ///
    /// Returns [`ConfluenceError::RsaKey`] if the private key file cannot be loaded.
    pub fn from_config(
        base_url: &str,
        consumer_key: &str,
        key_file: &Path,
        access_token: &str,
        _access_secret: &str,
    ) -> Result<Self, ConfluenceError> {
        let private_key = load_private_key_from_file(key_file)?;

        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(DEFAULT_TIMEOUT)))
            .http_status_as_error(false)
            .build()
            .into();

        Ok(Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_owned(),
            auth: OAuth1Auth::new(consumer_key, private_key, access_token),
        })
    }

    /// Get the API base URL.
    fn api_url(&self) -> String {
        format!("{}/rest/api", self.base_url)
    }
}
