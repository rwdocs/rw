//! YAML metadata parsing for filesystem storage.
//!
//! Provides functions for extracting metadata fields from YAML content
//! and full metadata parsing.

use rw_storage::{Metadata, MetadataError};

/// Extract a simple string field from YAML content.
///
/// Handles `field: Foo`, `field: "Foo"`, `field: 'Foo'`.
/// Returns `None` if no valid field is found.
fn extract_yaml_field(content: &str, field_name: &str) -> Option<String> {
    let prefix = format!("{field_name}:");
    content.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?.trim().trim_matches(['"', '\'']);
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Extract title from YAML content using simple string parsing.
///
/// Handles `title: Foo`, `title: "Foo"`, `title: 'Foo'`.
/// Returns `None` if no valid title field is found.
pub(crate) fn extract_yaml_title(content: &str) -> Option<String> {
    extract_yaml_field(content, "title")
}

/// Extract type from YAML content using simple string parsing.
///
/// Handles `type: Foo`, `type: "Foo"`, `type: 'Foo'`.
/// Returns `None` if no valid type field is found.
pub(crate) fn extract_yaml_type(content: &str) -> Option<String> {
    extract_yaml_field(content, "type")
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
    fn test_extract_yaml_title_simple() {
        assert_eq!(
            extract_yaml_title("title: My Title"),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn test_extract_yaml_title_quoted() {
        assert_eq!(
            extract_yaml_title("title: \"My Title\""),
            Some("My Title".to_string())
        );
        assert_eq!(
            extract_yaml_title("title: 'My Title'"),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn test_extract_yaml_title_with_other_fields() {
        let yaml = "type: domain\ntitle: My Title\ndescription: Some description";
        assert_eq!(extract_yaml_title(yaml), Some("My Title".to_string()));
    }

    #[test]
    fn test_extract_yaml_title_none() {
        assert_eq!(extract_yaml_title("type: domain"), None);
        assert_eq!(extract_yaml_title(""), None);
        assert_eq!(extract_yaml_title("title:"), None);
        assert_eq!(extract_yaml_title("title: "), None);
    }

    #[test]
    fn test_extract_yaml_type_simple() {
        assert_eq!(
            extract_yaml_type("type: domain"),
            Some("domain".to_string())
        );
    }

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
        assert_eq!(meta.title, Some("My Page".to_string()));
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
        assert_eq!(meta.title, Some("My Domain".to_string()));
        assert_eq!(meta.description, Some("Domain overview".to_string()));
        assert_eq!(meta.page_type, Some("domain".to_string()));
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
