//! YAML metadata parsing for filesystem storage.
//!
//! Provides functions for extracting metadata fields from YAML content
//! and full metadata parsing.

use serde::Deserialize;

use rw_storage::{Metadata, MetadataError};

/// Parsed fields from a YAML metadata file.
///
/// Lightweight struct for the fields needed during scan — avoids full
/// [`Metadata`] parsing (which includes `vars` deep-merge).
#[derive(Deserialize)]
pub(crate) struct YamlFields {
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub page_type: Option<String>,
    pub description: Option<String>,
}

/// Parse full metadata from YAML content.
///
/// Returns metadata for valid YAML (empty content returns a default instance).
///
/// # Errors
///
/// Returns an error if the YAML is malformed.
pub(crate) fn parse_metadata(content: &str) -> Result<Metadata, MetadataError> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(Metadata::default());
    }

    serde_yaml::from_str(trimmed).map_err(|e| MetadataError::Parse(format!("Invalid YAML: {e}")))
}
