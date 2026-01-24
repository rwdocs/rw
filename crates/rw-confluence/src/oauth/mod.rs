//! OAuth 1.0 RSA-SHA1 authentication for Confluence.
//!
//! This module provides OAuth 1.0 authentication with RSA-SHA1 signatures,
//! as required by Confluence Server/Data Center.

pub(crate) mod key;
mod signature;
mod token_generator;

pub use token_generator::{AccessToken, OAuthTokenGenerator, RequestToken};

use rsa::RsaPrivateKey;
use ureq::http::Uri;

use signature::create_authorization_header;

/// OAuth 1.0 RSA-SHA1 authentication (internal use only).
pub(crate) struct OAuth1Auth {
    consumer_key: String,
    private_key: RsaPrivateKey,
    access_token: String,
}

impl OAuth1Auth {
    /// Create auth instance with pre-loaded private key.
    pub(crate) fn new(consumer_key: &str, private_key: RsaPrivateKey, access_token: &str) -> Self {
        Self {
            consumer_key: consumer_key.to_string(),
            private_key,
            access_token: access_token.to_string(),
        }
    }

    /// Sign an HTTP request by computing OAuth signature and returning Authorization header value.
    ///
    /// # Arguments
    /// * `method` - HTTP method (GET, POST, PUT, etc.)
    /// * `uri` - Full request URI (including query string)
    pub(crate) fn sign(&self, method: &str, uri: &Uri) -> String {
        // Base URL excludes query string (RFC 5849 Section 3.4.1.2)
        let base_url = format!(
            "{}://{}{}",
            uri.scheme_str().unwrap_or("https"),
            uri.host().unwrap_or(""),
            uri.path()
        );

        // Parse query parameters to include in signature (RFC 5849 Section 3.4.1.3)
        let query_params: Vec<(String, String)> = uri
            .query()
            .map(|q| {
                q.split('&')
                    .filter_map(|param| {
                        let mut parts = param.splitn(2, '=');
                        let key = parts.next()?;
                        let value = parts.next().unwrap_or("");
                        Some((key.to_string(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        create_authorization_header(
            method,
            &base_url,
            &query_params,
            &self.consumer_key,
            &self.access_token,
            &self.private_key,
        )
    }
}
