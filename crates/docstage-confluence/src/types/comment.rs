//! Confluence comment types.

use serde::Deserialize;

use super::Body;

/// Confluence comment.
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    /// Comment ID.
    pub id: String,
    /// Comment title.
    pub title: String,
    /// Comment body content.
    pub body: Body,
    /// Extended properties.
    #[serde(default)]
    pub extensions: Option<Extensions>,
}

/// Comment extensions.
#[derive(Debug, Clone, Deserialize)]
pub struct Extensions {
    /// Inline comment properties.
    #[serde(rename = "inlineProperties", default)]
    pub inline_properties: Option<InlineProperties>,
    /// Resolution status.
    #[serde(default)]
    pub resolution: Option<Resolution>,
}

/// Inline comment properties.
#[derive(Debug, Clone, Deserialize)]
pub struct InlineProperties {
    /// Reference marker ID.
    #[serde(rename = "markerRef")]
    pub marker_ref: String,
    /// Original selected text.
    #[serde(rename = "originalSelection")]
    pub original_selection: String,
}

/// Comment resolution status.
#[derive(Debug, Clone, Deserialize)]
pub struct Resolution {
    /// Status ("open" or "resolved").
    pub status: String,
}

/// Comments API response.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentsResponse {
    /// List of comments.
    pub results: Vec<Comment>,
    /// Total count.
    pub size: usize,
}
