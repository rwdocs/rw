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

/// Build OAuth Authorization header from OAuth params.
fn build_authorization_header(oauth_params: &BTreeMap<String, String>) -> String {
    let header_parts: Vec<String> = oauth_params
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, oauth_encode(v)))
        .collect();
    format!("OAuth {}", header_parts.join(", "))
}

/// Create OAuth Authorization header value.
///
/// # Arguments
/// * `method` - HTTP method (GET, POST, etc.)
/// * `base_url` - URL without query string (<scheme://host/path>)
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
    let nonce = generate_nonce();
    let timestamp = generate_timestamp();

    let mut oauth_params = BTreeMap::new();
    oauth_params.insert("oauth_consumer_key".to_string(), consumer_key.to_string());
    oauth_params.insert("oauth_nonce".to_string(), nonce);
    oauth_params.insert("oauth_signature_method".to_string(), "RSA-SHA1".to_string());
    oauth_params.insert("oauth_timestamp".to_string(), timestamp);
    oauth_params.insert("oauth_token".to_string(), access_token.to_string());
    oauth_params.insert("oauth_version".to_string(), "1.0".to_string());

    // Build signature params: OAuth params + query params (RFC 5849 Section 3.4.1.3)
    let mut signature_params = oauth_params.clone();
    for (key, value) in query_params {
        signature_params.insert(key.clone(), value.clone());
    }

    let base_string = build_signature_base_string(method, base_url, &signature_params);
    let signature = sign_rsa_sha1(private_key, &base_string);
    oauth_params.insert("oauth_signature".to_string(), signature);

    build_authorization_header(&oauth_params)
}

/// Create OAuth Authorization header for token generation flow.
///
/// Unlike `create_authorization_header`, this supports:
/// - Optional `oauth_token` (absent for request token phase)
/// - Optional `oauth_callback` (for request token phase)
/// - Optional `oauth_verifier` (for access token phase)
///
/// # Arguments
/// * `method` - HTTP method (GET, POST, etc.)
/// * `base_url` - URL without query string (<scheme://host/path>)
/// * `query_params` - Query parameters to include in signature
/// * `consumer_key` - OAuth consumer key
/// * `oauth_token` - OAuth token (None for request token phase)
/// * `oauth_callback` - OAuth callback ("oob" for request token phase)
/// * `oauth_verifier` - OAuth verifier (for access token phase)
/// * `private_key` - RSA private key for signing
#[allow(clippy::too_many_arguments)]
pub fn create_authorization_header_for_token_flow(
    method: &str,
    base_url: &str,
    query_params: &[(String, String)],
    consumer_key: &str,
    oauth_token: Option<&str>,
    oauth_callback: Option<&str>,
    oauth_verifier: Option<&str>,
    private_key: &RsaPrivateKey,
) -> String {
    let mut oauth_params = BTreeMap::new();
    oauth_params.insert("oauth_consumer_key".to_string(), consumer_key.to_string());
    oauth_params.insert("oauth_nonce".to_string(), generate_nonce());
    oauth_params.insert("oauth_signature_method".to_string(), "RSA-SHA1".to_string());
    oauth_params.insert("oauth_timestamp".to_string(), generate_timestamp());
    oauth_params.insert("oauth_version".to_string(), "1.0".to_string());

    // Optional parameters for token flow
    if let Some(token) = oauth_token {
        oauth_params.insert("oauth_token".to_string(), token.to_string());
    }
    if let Some(callback) = oauth_callback {
        oauth_params.insert("oauth_callback".to_string(), callback.to_string());
    }
    if let Some(verifier) = oauth_verifier {
        oauth_params.insert("oauth_verifier".to_string(), verifier.to_string());
    }

    // Build signature params: OAuth params + query params (RFC 5849 Section 3.4.1.3)
    let mut signature_params = oauth_params.clone();
    for (key, value) in query_params {
        signature_params.insert(key.clone(), value.clone());
    }

    let base_string = build_signature_base_string(method, base_url, &signature_params);
    let signature = sign_rsa_sha1(private_key, &base_string);
    oauth_params.insert("oauth_signature".to_string(), signature);

    build_authorization_header(&oauth_params)
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

    fn test_key() -> RsaPrivateKey {
        use rsa::pkcs8::DecodePrivateKey;
        const TEST_KEY: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDXyzisgwj5oXOk
9bXXMCiqDbT70Tkwonl8c7P0Eec1cfCSjqw2cT9oi8zuXlZSmgsh9zPwab/0Uc5j
PFnW5wD5MIFARtSk2BKt8goiej3U7CMp0QL3hXb+ejMaP7kGZ9uYRjnQToou2J2/
02UBRSXrvMNwkvhBlIXtz0Fh6IveWvMEtEQcgn0wn+mc4cEf+zun2kFZ1mia8twI
BduiZPEUetskIMTxfhocwuZYwRJaVbPYh/QM9m2KjfvOWxRcakaKD5+fi8Jb5Oqm
tz27ZYv6M21HnGuOTlRAeIbgP4rv6p7JX3F4sBECl2oonjUQtUg/cjDOWp6JXNch
u+7hr6H5AgMBAAECggEAAl59S0uO/CqdGekGq4ugTqmi3IbiAVovSkH87keKCcir
8vf1BQ3+O7gZMl6/xN1jFObhX5jRni2NvgIqHFVh6dpx+NIuQHcM0XMQUGuWJTHI
ewuL5ErHUSjnSbj8X4khXI0c0mAiXTxMkxAPklF/hpSGcsRyTEoEpGU7mwcSDgld
a2PcPiI1PgfgBggHuD0y9EhFAM4Bs29plLudCWmtEOppgSCGwdNmhA0mQY58xVEA
JMUq4h5ANztz+GqGakMebGvIpssdu+JXLg9RtPthH3PNUg8UNQXBFtE62YOUIIIn
oyGWQSoApfqjUYNSsWSxl66+NdeB2kw9r9o71XihAQKBgQDttragQmkqQzRZ4CLx
jhG+zb92zGIjTRiHe1bVVu/cOWPaFhTmjsc+tWcWFLzvPTOkcJ3/hZzxSFuAgcg7
dZVsivgyTCfcTHixranllKfJhZ3/F+ZOcoSkiqBzr1EFLFP87XdTf2kQhFgpBNGo
E81fMgbfsQRmd+Fimo8N0uCOQQKBgQDoZNcqhoC6jxc3iBFEiIMgLAmccx8N0dC3
xEwxg/RJ1njg1z3mcZoX6Ec+2NU7jlwR+mTUlS2aVHYDFZqOnVicQCEvkQbYt7De
omodKKrdYN0HDZcQcQQtGvTV6ASIOUJBVbB5gOyx3gi196ERzZ/diGhUpHbiNhi5
ssoT3V2VuQKBgEhhUPw9HG5s5hzTnXA1lPunBDx1ARDEocpm6Mqu3PwOUXQPMy/8
m3hhndDgYaLq3LWeQM2T7nSdVpcrbT+Fjwjsy6PtAloWws0/FrM771byI2iP62VJ
g0/ikfaHlEDh/XTPDX1UFzabRYi/2eK2nNr2jZdA/BkDOZJfg11vL0bBAoGAWod9
8kj3OLWpO66721C6k/vTuqh1/nIvtoa3j8pxjZoI+L2glXbHqmyH5Imfd1Xbs/0w
7kc2vpoMZuMxlEDjVer9goQigKX+NpxabgV7mkWzlJ3MrVD5aYDIw9NggJidoMn6
tzpr+lYeWpSeoErT7f7HdcGjtjeQpjZp1hcz77ECgYEA4QxMNusdXfNwxeemDxs2
9S1pQ8Vrzvw8ACcJBZTluKvGuO3hoPMSu8ywt1Sew74a9QbkkfbPmqujc62FHo1+
o6Ypn8ZrOCbdrwdSpQu37/7pcDFMq/HAyf2I43wreDAcYktu33ZiEDTkyYM0ygv/
PmtLs+m8nwD5m6Eay2zt00Q=
-----END PRIVATE KEY-----";
        RsaPrivateKey::from_pkcs8_pem(TEST_KEY).unwrap()
    }

    #[test]
    fn test_token_flow_signature_without_token() {
        let key = test_key();
        let header = create_authorization_header_for_token_flow(
            "POST",
            "https://example.com/oauth/request-token",
            &[],
            "consumer_key",
            None,
            Some("oob"),
            None,
            &key,
        );

        assert!(header.starts_with("OAuth "));
        assert!(header.contains("oauth_callback"));
        assert!(!header.contains("oauth_token="));
        assert!(!header.contains("oauth_verifier"));
    }

    #[test]
    fn test_token_flow_signature_with_verifier() {
        let key = test_key();
        let header = create_authorization_header_for_token_flow(
            "POST",
            "https://example.com/oauth/access-token",
            &[],
            "consumer_key",
            Some("request_token"),
            None,
            Some("verifier_code"),
            &key,
        );

        assert!(header.starts_with("OAuth "));
        assert!(header.contains("oauth_token="));
        assert!(header.contains("oauth_verifier="));
        assert!(!header.contains("oauth_callback"));
    }
}
