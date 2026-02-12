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

/// Parse title, type, and description from YAML content.
///
/// Uses serde_yaml for correct handling of all YAML value styles
/// (quoted strings, block scalars `|`/`>`, etc.).
/// Returns `None` if the content is empty or not valid YAML.
pub(crate) fn extract_yaml_fields(content: &str) -> Option<YamlFields> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_yaml::from_str(trimmed).ok()
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

    // ── extract_yaml_fields tests ────────────────────────────────────

    #[test]
    fn test_extract_fields_simple_values() {
        let yaml = "title: My Title\ntype: domain\ndescription: Some description";
        let fields = extract_yaml_fields(yaml).unwrap();
        assert_eq!(fields.title, Some("My Title".to_owned()));
        assert_eq!(fields.page_type, Some("domain".to_owned()));
        assert_eq!(fields.description, Some("Some description".to_owned()));
    }

    #[test]
    fn test_extract_fields_quoted_values() {
        let yaml = "title: \"My Title\"\ndescription: 'A description'";
        let fields = extract_yaml_fields(yaml).unwrap();
        assert_eq!(fields.title, Some("My Title".to_owned()));
        assert_eq!(fields.description, Some("A description".to_owned()));
    }

    #[test]
    fn test_extract_fields_block_scalar_description() {
        let yaml = "title: My Title\ndescription: |\n  This is a\n  multiline description";
        let fields = extract_yaml_fields(yaml).unwrap();
        assert_eq!(fields.title, Some("My Title".to_owned()));
        assert_eq!(
            fields.description,
            Some("This is a\nmultiline description".to_owned())
        );
    }

    #[test]
    fn test_extract_fields_folded_scalar_description() {
        let yaml = "title: My Title\ndescription: >\n  This is a\n  folded description";
        let fields = extract_yaml_fields(yaml).unwrap();
        assert_eq!(fields.title, Some("My Title".to_owned()));
        assert!(fields.description.is_some());
    }

    #[test]
    fn test_extract_fields_missing_fields_are_none() {
        let yaml = "type: domain";
        let fields = extract_yaml_fields(yaml).unwrap();
        assert!(fields.title.is_none());
        assert_eq!(fields.page_type, Some("domain".to_owned()));
        assert!(fields.description.is_none());
    }

    #[test]
    fn test_extract_fields_empty_returns_none() {
        assert!(extract_yaml_fields("").is_none());
        assert!(extract_yaml_fields("   \n\t  ").is_none());
    }

    #[test]
    fn test_extract_fields_invalid_yaml_returns_none() {
        assert!(extract_yaml_fields("title: [invalid yaml").is_none());
    }

    // ── parse_metadata tests ─────────────────────────────────────────

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
