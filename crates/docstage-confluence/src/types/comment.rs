//! Confluence comment types.

use serde::Deserialize;

/// Comments API response.
///
/// Only includes `size` since we only need the comment count.
/// Serde ignores unknown fields, so the `results` array from the API is skipped.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentsResponse {
    /// Total count of comments.
    pub size: usize,
}
