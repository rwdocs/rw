//! Page operations for Confluence API.

use serde_json::json;
use tracing::info;

use super::ConfluenceClient;
use crate::error::ConfluenceError;
use crate::types::Page;

impl ConfluenceClient {
    /// Get page by ID with optional field expansion.
    pub(crate) fn get_page(&self, page_id: &str, expand: &[&str]) -> Result<Page, ConfluenceError> {
        let mut url = format!("{}/content/{}", self.api_url(), page_id);

        if !expand.is_empty() {
            url.push_str("?expand=");
            url.push_str(&expand.join(","));
        }

        info!("Getting page {}", page_id);

        let uri: ureq::http::Uri = url.parse().unwrap();
        let auth_header = self.auth.sign("GET", &uri);

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| ConfluenceError::Http {
                status: 0,
                body: e.to_string(),
            })?;

        let status = response.status().as_u16();
        let mut body_reader = response.into_body();

        if status >= 400 {
            let error_body = body_reader
                .read_to_string()
                .unwrap_or_else(|_| "(unable to read error body)".to_string());
            return Err(ConfluenceError::Http {
                status,
                body: error_body,
            });
        }

        Ok(body_reader.read_json()?)
    }

    /// Update existing page (auto-increments version).
    pub(crate) fn update_page(
        &self,
        page_id: &str,
        title: &str,
        body: &str,
        version: u32,
        message: Option<&str>,
    ) -> Result<Page, ConfluenceError> {
        let url = format!("{}/content/{}", self.api_url(), page_id);

        let mut payload = json!({
            "type": "page",
            "title": title,
            "body": {
                "storage": {
                    "value": body,
                    "representation": "storage"
                }
            },
            "version": {"number": version + 1}
        });

        if let Some(msg) = message {
            payload["version"]["message"] = json!(msg);
        }

        info!(
            "Updating page {} from version {} to {}",
            page_id,
            version,
            version + 1
        );

        let uri: ureq::http::Uri = url.parse().unwrap();
        let auth_header = self.auth.sign("PUT", &uri);

        let payload_bytes = serde_json::to_vec(&payload)?;

        let response = self
            .agent
            .put(&url)
            .header("Authorization", &auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(&payload_bytes[..])
            .map_err(|e| ConfluenceError::Http {
                status: 0,
                body: e.to_string(),
            })?;

        let status = response.status().as_u16();
        let mut body_reader = response.into_body();

        if status >= 400 {
            let error_body = body_reader
                .read_to_string()
                .unwrap_or_else(|_| "(unable to read error body)".to_string());
            return Err(ConfluenceError::Http {
                status,
                body: error_body,
            });
        }

        let page: Page = body_reader.read_json()?;
        info!(
            "Updated page {} to version {}",
            page_id, page.version.number
        );
        Ok(page)
    }

    /// Get web URL for page.
    pub(crate) fn get_page_url(&self, page_id: &str) -> Result<String, ConfluenceError> {
        let page = self.get_page(page_id, &[])?;

        if let Some(links) = &page.links
            && let Some(webui) = &links.webui
        {
            return Ok(format!("{}{}", self.base_url, webui));
        }

        Ok(format!(
            "{}/pages/viewpage.action?pageId={}",
            self.base_url, page_id
        ))
    }
}
