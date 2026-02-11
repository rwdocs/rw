//! OAuth 1.0 token generation for Confluence.
//!
//! Handles the three-legged OAuth 1.0 flow for generating access tokens.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use percent_encoding::percent_decode_str;
use rsa::RsaPrivateKey;
use ureq::Agent;

use super::key::load_private_key_from_file;
use super::signature::create_authorization_header_for_token_flow;
use crate::error::ConfluenceError;

/// Default HTTP timeout in seconds.
const DEFAULT_TIMEOUT: u64 = 30;

/// Temporary credentials from request token phase.
#[derive(Debug, Clone)]
pub struct RequestToken {
    pub oauth_token: String,
    pub oauth_token_secret: String,
}

/// Final access credentials.
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub oauth_token: String,
    pub oauth_token_secret: String,
}

/// OAuth 1.0 token generator for Confluence.
///
/// Handles the three-legged OAuth flow:
/// 1. Request temporary credentials (request token)
/// 2. Generate authorization URL for user
/// 3. Exchange verifier for access credentials
pub struct OAuthTokenGenerator {
    agent: Agent,
    consumer_key: String,
    private_key: RsaPrivateKey,
    request_token_url: String,
    authorize_url: String,
    access_token_url: String,
}

impl OAuthTokenGenerator {
    /// Create a new token generator.
    ///
    /// # Arguments
    /// * `base_url` - Confluence server base URL
    /// * `consumer_key` - OAuth consumer key
    /// * `key_file` - Path to RSA private key file (PEM format)
    ///
    /// # Errors
    ///
    /// Returns an error if the private key file cannot be read or parsed.
    pub fn new(
        base_url: &str,
        consumer_key: &str,
        key_file: &Path,
    ) -> Result<Self, ConfluenceError> {
        let private_key = load_private_key_from_file(key_file)?;
        let base_url = base_url.trim_end_matches('/');

        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(DEFAULT_TIMEOUT)))
            .http_status_as_error(false)
            .build()
            .into();

        Ok(Self {
            agent,
            consumer_key: consumer_key.to_owned(),
            private_key,
            request_token_url: format!("{base_url}/plugins/servlet/oauth/request-token"),
            authorize_url: format!("{base_url}/plugins/servlet/oauth/authorize"),
            access_token_url: format!("{base_url}/plugins/servlet/oauth/access-token"),
        })
    }

    /// Step 1: Request temporary credentials.
    ///
    /// Returns request token and authorization URL for the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is invalid.
    pub fn request_token(&self) -> Result<(RequestToken, String), ConfluenceError> {
        let auth_header = create_authorization_header_for_token_flow(
            "POST",
            &self.request_token_url,
            &[],
            &self.consumer_key,
            None,
            Some("oob"),
            None,
            &self.private_key,
        );

        let response = self
            .agent
            .post(&self.request_token_url)
            .header("Authorization", &auth_header)
            .send(&[] as &[u8])
            .map_err(|e| ConfluenceError::OAuth(format!("Request token failed: {e}")))?;

        let status = response.status().as_u16();
        let mut body_reader = response.into_body();
        let body = body_reader
            .read_to_string()
            .map_err(|e| ConfluenceError::OAuth(format!("Failed to read response: {e}")))?;

        if status >= 400 {
            return Err(ConfluenceError::OAuth(format!(
                "Request token failed ({status}): {body}"
            )));
        }

        let params = parse_oauth_response(&body);
        let oauth_token = get_required_param(&params, "oauth_token")?;
        let oauth_token_secret = get_required_param(&params, "oauth_token_secret")?;

        let request_token = RequestToken {
            oauth_token,
            oauth_token_secret,
        };

        let auth_url = self.get_authorization_url(&request_token);
        Ok((request_token, auth_url))
    }

    /// Step 2: Get authorization URL for a request token.
    ///
    /// This is a convenience method; the URL is also returned by `request_token()`.
    #[must_use]
    pub fn get_authorization_url(&self, request_token: &RequestToken) -> String {
        format!(
            "{}?oauth_token={}",
            self.authorize_url, request_token.oauth_token
        )
    }

    /// Step 3: Exchange verifier for access token.
    ///
    /// Call this after user authorizes and provides the verifier code.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response is invalid.
    pub fn exchange_verifier(
        &self,
        request_token: &RequestToken,
        verifier: &str,
    ) -> Result<AccessToken, ConfluenceError> {
        let auth_header = create_authorization_header_for_token_flow(
            "POST",
            &self.access_token_url,
            &[],
            &self.consumer_key,
            Some(&request_token.oauth_token),
            None,
            Some(verifier),
            &self.private_key,
        );

        let response = self
            .agent
            .post(&self.access_token_url)
            .header("Authorization", &auth_header)
            .send(&[] as &[u8])
            .map_err(|e| ConfluenceError::OAuth(format!("Access token exchange failed: {e}")))?;

        let status = response.status().as_u16();
        let mut body_reader = response.into_body();
        let body = body_reader
            .read_to_string()
            .map_err(|e| ConfluenceError::OAuth(format!("Failed to read response: {e}")))?;

        if status >= 400 {
            return Err(ConfluenceError::OAuth(format!(
                "Access token exchange failed ({status}): {body}"
            )));
        }

        let params = parse_oauth_response(&body);
        let oauth_token = get_required_param(&params, "oauth_token")?;
        let oauth_token_secret = get_required_param(&params, "oauth_token_secret")?;

        Ok(AccessToken {
            oauth_token,
            oauth_token_secret,
        })
    }
}

/// Parse OAuth URL-encoded response body.
fn parse_oauth_response(body: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in body.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            params.insert(
                percent_decode_str(key).decode_utf8_lossy().into_owned(),
                percent_decode_str(value).decode_utf8_lossy().into_owned(),
            );
        }
    }
    params
}

/// Extract required parameter from OAuth response.
fn get_required_param(
    params: &HashMap<String, String>,
    key: &str,
) -> Result<String, ConfluenceError> {
    params
        .get(key)
        .cloned()
        .ok_or_else(|| ConfluenceError::OAuth(format!("Missing parameter: {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oauth_response() {
        let body = "oauth_token=abc123&oauth_token_secret=xyz789&oauth_callback_confirmed=true";
        let params = parse_oauth_response(body);

        assert_eq!(params.get("oauth_token"), Some(&"abc123".to_owned()));
        assert_eq!(
            params.get("oauth_token_secret"),
            Some(&"xyz789".to_owned())
        );
        assert_eq!(
            params.get("oauth_callback_confirmed"),
            Some(&"true".to_owned())
        );
    }

    #[test]
    fn test_parse_oauth_response_with_encoded_values() {
        let body = "oauth_token=abc%2B123&oauth_token_secret=xyz%3D789";
        let params = parse_oauth_response(body);

        assert_eq!(params.get("oauth_token"), Some(&"abc+123".to_owned()));
        assert_eq!(
            params.get("oauth_token_secret"),
            Some(&"xyz=789".to_owned())
        );
    }

    #[test]
    fn test_get_required_param_missing() {
        let params = HashMap::new();
        let result = get_required_param(&params, "oauth_token");

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing parameter")
        );
    }

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

    fn create_test_key_file() -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(TEST_KEY.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_endpoints_from_base_url() {
        let key_file = create_test_key_file();
        let generator = OAuthTokenGenerator::new(
            "https://confluence.example.com",
            "consumer_key",
            key_file.path(),
        )
        .unwrap();

        assert!(generator.request_token_url.contains("request-token"));
        assert!(generator.authorize_url.contains("authorize"));
        assert!(generator.access_token_url.contains("access-token"));
    }

    #[test]
    fn test_endpoints_strips_trailing_slash() {
        let key_file = create_test_key_file();
        let generator = OAuthTokenGenerator::new(
            "https://confluence.example.com/",
            "consumer_key",
            key_file.path(),
        )
        .unwrap();

        assert!(!generator.request_token_url.contains("//plugins"));
    }

    #[test]
    fn test_authorization_url() {
        let key_file = create_test_key_file();
        let generator = OAuthTokenGenerator::new(
            "https://confluence.example.com",
            "consumer_key",
            key_file.path(),
        )
        .unwrap();

        let request_token = RequestToken {
            oauth_token: "test_token".to_owned(),
            oauth_token_secret: "test_secret".to_owned(),
        };

        let auth_url = generator.get_authorization_url(&request_token);
        assert!(auth_url.contains("oauth_token=test_token"));
        assert!(
            auth_url.starts_with("https://confluence.example.com/plugins/servlet/oauth/authorize")
        );
    }
}
