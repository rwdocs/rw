//! Confluence REST API client.
//!
//! Provides sync HTTP client for Confluence Server/Data Center REST API
//! with OAuth 1.0 RSA-SHA1 authentication.

mod attachments;
mod comments;
mod pages;

use std::time::Duration;

use ureq::Agent;

use crate::error::ConfluenceError;
use crate::oauth::OAuth1Auth;

/// Default HTTP timeout in seconds.
const DEFAULT_TIMEOUT: u64 = 30;

/// Confluence REST API client.
pub struct ConfluenceClient {
    agent: Agent,
    base_url: String,
    auth: OAuth1Auth,
}

impl ConfluenceClient {
    /// Create client with OAuth 1.0 authentication.
    fn new(base_url: &str, auth: OAuth1Auth) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(DEFAULT_TIMEOUT)))
            .http_status_as_error(false)
            .build()
            .into();

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth,
        }
    }

    /// Create client from config values (convenience constructor).
    pub fn from_config(
        base_url: &str,
        consumer_key: &str,
        private_key: &[u8],
        access_token: &str,
        access_secret: &str,
    ) -> Result<Self, ConfluenceError> {
        let auth = OAuth1Auth::new(consumer_key, private_key, access_token, access_secret)?;
        Ok(Self::new(base_url, auth))
    }

    /// Get the API base URL.
    fn api_url(&self) -> String {
        format!("{}/rest/api", self.base_url)
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
