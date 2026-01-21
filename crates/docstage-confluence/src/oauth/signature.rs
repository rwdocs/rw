//! OAuth 1.0 signature generation (RFC 5849).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_encode};
use rand::Rng;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{SignatureEncoding, Signer};
use sha1::Sha1;

/// OAuth unreserved characters: A-Z a-z 0-9 - . _ ~
const OAUTH_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Percent-encode string per RFC 3986.
fn oauth_encode(input: &str) -> String {
    percent_encode(input.as_bytes(), OAUTH_ENCODE_SET).to_string()
}

/// Generate cryptographically random nonce (32 hex characters).
fn generate_nonce() -> String {
    let bytes: [u8; 16] = rand::rng().random();
    hex::encode(bytes)
}

/// Generate Unix timestamp.
fn generate_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// Sign data with RSA-SHA1 and return base64-encoded signature.
fn sign_rsa_sha1(private_key: &RsaPrivateKey, data: &str) -> String {
    let signing_key = SigningKey::<Sha1>::new(private_key.clone());
    let signature = signing_key.sign(data.as_bytes());
    BASE64_STANDARD.encode(signature.to_bytes())
}

/// Build OAuth signature base string per RFC 5849 Section 3.4.1.
///
/// Format: `HTTP_METHOD&encoded_base_url&encoded_parameters`
fn build_signature_base_string(
    method: &str,
    base_url: &str,
    params: &BTreeMap<String, String>,
) -> String {
    // Normalize parameters: encode keys/values, sort by key then value
    let param_string = params
        .iter()
        .map(|(k, v)| format!("{}={}", oauth_encode(k), oauth_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    format!(
        "{}&{}&{}",
        method.to_uppercase(),
        oauth_encode(base_url),
        oauth_encode(&param_string)
    )
}

/// Create OAuth Authorization header value.
///
/// # Arguments
/// * `method` - HTTP method (GET, POST, etc.)
/// * `base_url` - URL without query string (scheme://host/path)
/// * `query_params` - Query parameters to include in signature
/// * `consumer_key` - OAuth consumer key
/// * `access_token` - OAuth access token
/// * `private_key` - RSA private key for signing
pub fn create_authorization_header(
    method: &str,
    base_url: &str,
    query_params: &[(String, String)],
    consumer_key: &str,
    access_token: &str,
    private_key: &RsaPrivateKey,
) -> String {
    let mut params = BTreeMap::new();
    params.insert("oauth_consumer_key".to_string(), consumer_key.to_string());
    params.insert("oauth_nonce".to_string(), generate_nonce());
    params.insert("oauth_signature_method".to_string(), "RSA-SHA1".to_string());
    params.insert("oauth_timestamp".to_string(), generate_timestamp());
    params.insert("oauth_token".to_string(), access_token.to_string());
    params.insert("oauth_version".to_string(), "1.0".to_string());

    // Include query parameters in signature (RFC 5849 Section 3.4.1.3)
    for (key, value) in query_params {
        params.insert(key.clone(), value.clone());
    }

    let base_string = build_signature_base_string(method, base_url, &params);
    let signature = sign_rsa_sha1(private_key, &base_string);

    // Only include OAuth params in the header (not query params)
    let mut oauth_params = BTreeMap::new();
    oauth_params.insert("oauth_consumer_key".to_string(), consumer_key.to_string());
    oauth_params.insert("oauth_nonce".to_string(), params["oauth_nonce"].clone());
    oauth_params.insert(
        "oauth_signature_method".to_string(),
        "RSA-SHA1".to_string(),
    );
    oauth_params.insert("oauth_timestamp".to_string(), params["oauth_timestamp"].clone());
    oauth_params.insert("oauth_token".to_string(), access_token.to_string());
    oauth_params.insert("oauth_version".to_string(), "1.0".to_string());
    oauth_params.insert("oauth_signature".to_string(), signature);

    // Build header: OAuth key1="value1", key2="value2", ...
    let header_parts: Vec<String> = oauth_params
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, oauth_encode(v)))
        .collect();

    format!("OAuth {}", header_parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_encode_unreserved() {
        // Unreserved characters should not be encoded
        assert_eq!(oauth_encode("abc123"), "abc123");
        assert_eq!(oauth_encode("ABC"), "ABC");
        assert_eq!(oauth_encode("-._~"), "-._~");
    }

    #[test]
    fn test_oauth_encode_reserved() {
        // Reserved characters should be encoded
        assert_eq!(oauth_encode(" "), "%20");
        assert_eq!(oauth_encode("&"), "%26");
        assert_eq!(oauth_encode("="), "%3D");
        assert_eq!(oauth_encode("/"), "%2F");
    }

    #[test]
    fn test_nonce_uniqueness() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();
        assert_ne!(nonce1, nonce2);
        assert_eq!(nonce1.len(), 32);
    }

    #[test]
    fn test_signature_base_string() {
        let mut params = BTreeMap::new();
        params.insert("oauth_consumer_key".to_string(), "test_key".to_string());
        params.insert("oauth_nonce".to_string(), "123456".to_string());

        let base = build_signature_base_string("GET", "https://example.com/api", &params);

        assert!(base.starts_with("GET&"));
        assert!(base.contains("https%3A%2F%2Fexample.com%2Fapi"));
    }
}
