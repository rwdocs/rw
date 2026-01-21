//! Comment operations for Confluence API.

use tracing::info;

use super::ConfluenceClient;
use crate::error::ConfluenceError;
use crate::types::CommentsResponse;

impl ConfluenceClient {
    /// Get all comments on a page.
    pub fn get_comments(&self, page_id: &str) -> Result<CommentsResponse, ConfluenceError> {
        let url = format!(
            "{}/content/{}/child/comment?expand=body.storage",
            self.api_url(),
            page_id
        );

        info!("Getting comments for page {}", page_id);

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

        let comments: CommentsResponse = body_reader.read_json()?;
        info!("Found {} comments on page {}", comments.size, page_id);
        Ok(comments)
    }

    /// Get inline comments with marker refs.
    pub fn get_inline_comments(&self, page_id: &str) -> Result<CommentsResponse, ConfluenceError> {
        let url = format!(
            "{}/content/{}/child/comment?expand=body.storage,extensions.inlineProperties,extensions.resolution&depth=all&location=inline",
            self.api_url(),
            page_id
        );

        info!("Getting inline comments for page {}", page_id);

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

        let comments: CommentsResponse = body_reader.read_json()?;
        info!(
            "Found {} inline comments on page {}",
            comments.size, page_id
        );
        Ok(comments)
    }

    /// Get footer (page-level) comments.
    pub fn get_footer_comments(&self, page_id: &str) -> Result<CommentsResponse, ConfluenceError> {
        let url = format!(
            "{}/content/{}/child/comment?expand=body.storage,extensions.resolution&depth=all&location=footer",
            self.api_url(),
            page_id
        );

        info!("Getting footer comments for page {}", page_id);

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

        let comments: CommentsResponse = body_reader.read_json()?;
        info!(
            "Found {} footer comments on page {}",
            comments.size, page_id
        );
        Ok(comments)
    }
}
