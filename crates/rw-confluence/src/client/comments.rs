//! Comment operations for Confluence API.

use tracing::info;

use super::ConfluenceClient;
use crate::error::ConfluenceError;
use crate::types::CommentsResponse;

impl ConfluenceClient {
    /// Get all comments on a page.
    pub(crate) fn get_comments(&self, page_id: &str) -> Result<CommentsResponse, ConfluenceError> {
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
            .call()?;

        let status = response.status().as_u16();
        let mut body_reader = response.into_body();

        if status >= 400 {
            let error_body = body_reader
                .read_to_string()
                .unwrap_or_else(|_| "(unable to read error body)".to_owned());
            return Err(ConfluenceError::HttpResponse {
                status,
                body: error_body,
            });
        }

        let comments: CommentsResponse = body_reader.read_json()?;
        info!("Found {} comments on page {}", comments.size, page_id);
        Ok(comments)
    }
}
