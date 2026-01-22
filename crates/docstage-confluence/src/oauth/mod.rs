//! OAuth 1.0 RSA-SHA1 authentication for Confluence.
//!
//! This module provides OAuth 1.0 authentication with RSA-SHA1 signatures,
//! as required by Confluence Server/Data Center.
//!
//! # Example
//!
//! ```ignore
//! use docstage_confluence::oauth::{OAuth1Auth, read_private_key};
//!
//! let key = read_private_key("private_key.pem")?;
//! let auth = OAuth1Auth::new("consumer_key", &key, "access_token", "access_secret")?;
//! ```

mod key;
mod signature;
mod token_generator;

pub use key::read_private_key;
pub use token_generator::{AccessToken, OAuthTokenGenerator, RequestToken};

use rsa::RsaPrivateKey;
use ureq::http::{Request, Uri};

use crate::error::ConfluenceError;
use signature::create_authorization_header;

/// OAuth 1.0 RSA-SHA1 authentication.
pub struct OAuth1Auth {
    consumer_key: String,
    private_key: RsaPrivateKey,
    access_token: String,
}

impl OAuth1Auth {
    /// Create auth instance from config values.
    ///
    /// # Arguments
    /// * `consumer_key` - OAuth consumer key
    /// * `private_key_pem` - PEM-encoded RSA private key bytes
    /// * `access_token` - OAuth access token
    /// * `access_secret` - OAuth access token secret (unused in RSA-SHA1, kept for API compat)
    pub fn new(
        consumer_key: &str,
        private_key_pem: &[u8],
        access_token: &str,
        _access_secret: &str,
    ) -> Result<Self, ConfluenceError> {
        let private_key = key::load_private_key(private_key_pem)?;
        Ok(Self {
            consumer_key: consumer_key.to_string(),
            private_key,
            access_token: access_token.to_string(),
        })
    }

    /// Sign an HTTP request by computing OAuth signature and returning Authorization header value.
    ///
    /// # Arguments
    /// * `method` - HTTP method (GET, POST, PUT, etc.)
    /// * `uri` - Full request URI (including query string)
    pub fn sign(&self, method: &str, uri: &Uri) -> String {
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

    /// Create signed request builder with Authorization header.
    pub fn sign_request<B>(&self, request: Request<B>) -> Request<B> {
        let method = request.method().as_str();
        let uri = request.uri().clone();
        let auth_header = self.sign(method, &uri);

        let (mut parts, body) = request.into_parts();
        parts
            .headers
            .insert("Authorization", auth_header.parse().unwrap());
        Request::from_parts(parts, body)
    }
}
