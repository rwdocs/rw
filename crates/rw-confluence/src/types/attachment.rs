//! Confluence attachment types.

use serde::Deserialize;

/// Confluence attachment.
///
/// Only includes fields that are actually used.
/// Serde ignores unknown fields from the API response.
#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// Attachment title/filename.
    pub title: String,
}

/// Attachments API response.
///
/// Only includes `results` since we only need the attachment list.
/// Serde ignores unknown fields like `size` from the API response.
#[derive(Debug, Clone, Deserialize)]
pub struct AttachmentsResponse {
    /// List of attachments.
    pub results: Vec<Attachment>,
}
