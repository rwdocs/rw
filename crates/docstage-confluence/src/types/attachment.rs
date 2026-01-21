//! Confluence attachment types.

use serde::Deserialize;

/// Confluence attachment.
#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// Attachment title/filename.
    pub title: String,
    /// Content type (always "attachment").
    #[serde(rename = "type")]
    pub content_type: String,
}

/// Attachments API response.
#[derive(Debug, Clone, Deserialize)]
pub struct AttachmentsResponse {
    /// List of attachments.
    pub results: Vec<Attachment>,
    /// Total count.
    #[serde(default)]
    pub size: usize,
}
