//! Confluence page types.

use serde::{Deserialize, Serialize};

/// Confluence page.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Page {
    /// Page ID.
    pub id: String,
    /// Content type (always "page").
    #[serde(rename = "type")]
    pub content_type: String,
    /// Page title.
    pub title: String,
    /// Version information.
    pub version: Version,
    /// Page body content.
    #[serde(default)]
    pub body: Option<Body>,
    /// Hypermedia links.
    #[serde(rename = "_links", default)]
    pub links: Option<Links>,
}

/// Page version.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Version {
    /// Version number.
    pub number: u32,
    /// Version message/comment.
    #[serde(default)]
    pub message: Option<String>,
}

/// Page body content.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Body {
    /// Storage format content.
    #[serde(default)]
    pub storage: Option<Storage>,
}

/// Storage format representation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Storage {
    /// HTML content in Confluence storage format.
    pub value: String,
    /// Content representation (always "storage").
    pub representation: String,
}

/// Hypermedia links.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Links {
    /// Web UI link.
    #[serde(default)]
    pub webui: Option<String>,
    /// API self link.
    #[serde(rename = "self", default)]
    pub self_link: Option<String>,
}
