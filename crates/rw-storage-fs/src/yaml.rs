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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metadata_empty() {
        let result = parse_metadata("");
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_parse_metadata_whitespace_only() {
        let result = parse_metadata("   \n\t  ");
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_parse_metadata_title_only() {
        let yaml = "title: My Page";
        let result = parse_metadata(yaml);
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert_eq!(meta.title, Some("My Page".to_owned()));
        assert!(meta.description.is_none());
        assert!(meta.page_type.is_none());
        assert!(meta.vars.is_empty());
    }

    #[test]
    fn test_parse_metadata_all_fields() {
        let yaml = r#"
title: "My Domain"
description: "Domain overview"
type: domain
vars:
  owner: team-a
  priority: 1
  tags:
    - important
    - core
"#;
        let result = parse_metadata(yaml);
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert_eq!(meta.title, Some("My Domain".to_owned()));
        assert_eq!(meta.description, Some("Domain overview".to_owned()));
        assert_eq!(meta.page_type, Some("domain".to_owned()));
        assert_eq!(meta.vars.get("owner"), Some(&serde_json::json!("team-a")));
        assert_eq!(meta.vars.get("priority"), Some(&serde_json::json!(1)));
        assert!(meta.vars.contains_key("tags"));
    }

    #[test]
    fn test_parse_metadata_invalid_yaml() {
        let yaml = "title: [invalid yaml";
        let result = parse_metadata(yaml);
        assert!(result.is_err());
    }
}
