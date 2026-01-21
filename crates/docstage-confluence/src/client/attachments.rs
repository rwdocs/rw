//! Attachment operations for Confluence API.

use rand::Rng;
use tracing::info;

use super::ConfluenceClient;
use crate::error::ConfluenceError;
use crate::types::{Attachment, AttachmentsResponse};

impl ConfluenceClient {
    /// Upload or update attachment (upsert by filename).
    pub fn upload_attachment(
        &self,
        page_id: &str,
        filename: &str,
        data: &[u8],
        content_type: &str,
        comment: Option<&str>,
    ) -> Result<Attachment, ConfluenceError> {
        // Check if attachment already exists
        let existing = self.find_attachment_by_name(page_id, filename)?;

        let url = if let Some(ref att) = existing {
            info!(
                "Updating existing attachment '{}' (id={})",
                filename, att.id
            );
            format!(
                "{}/content/{}/child/attachment/{}/data",
                self.api_url(),
                page_id,
                att.id
            )
        } else {
            info!(
                "Uploading new attachment '{}' to page {}",
                filename, page_id
            );
            format!("{}/content/{}/child/attachment", self.api_url(), page_id)
        };

        let uri: ureq::http::Uri = url.parse().unwrap();
        let auth_header = self.auth.sign("POST", &uri);

        // Build multipart form data manually
        let boundary = format!(
            "----DocstageFormBoundary{:016x}",
            rand::rng().random::<u64>()
        );
        let mut body = Vec::new();

        // Add file part
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
                filename
            )
            .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", content_type).as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");

        // Add comment if provided
        if let Some(c) = comment {
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"comment\"\r\n\r\n");
            body.extend_from_slice(c.as_bytes());
            body.extend_from_slice(b"\r\n");
        }

        // End boundary
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &auth_header)
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={}", boundary),
            )
            .header("X-Atlassian-Token", "nocheck")
            .header("Accept", "application/json")
            .send(&body[..])
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

        // Response is a list for new uploads, single object for updates
        if existing.is_some() {
            Ok(body_reader.read_json()?)
        } else {
            let response: AttachmentsResponse = body_reader.read_json()?;
            response
                .results
                .into_iter()
                .next()
                .ok_or_else(|| ConfluenceError::Http {
                    status: 200,
                    body: "Empty attachment response".to_string(),
                })
        }
    }

    /// List attachments on a page.
    pub fn get_attachments(&self, page_id: &str) -> Result<AttachmentsResponse, ConfluenceError> {
        let url = format!("{}/content/{}/child/attachment", self.api_url(), page_id);

        info!("Getting attachments for page {}", page_id);

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

    /// Find attachment by filename on a page.
    fn find_attachment_by_name(
        &self,
        page_id: &str,
        filename: &str,
    ) -> Result<Option<Attachment>, ConfluenceError> {
        let attachments = self.get_attachments(page_id)?;
        Ok(attachments
            .results
            .into_iter()
            .find(|a| a.title == filename))
    }
}
