//! Page metadata support via YAML sidecar files.
//!
//! Provides [`PageMetadata`] for storing page-level configuration and
//! [`merge_metadata`] for implementing inheritance.
//!
//! # Metadata Files
//!
//! Metadata is stored in YAML sidecar files (default: `meta.yaml`) in the same
//! directory as the markdown file. The metadata applies to the page defined by
//! `index.md` in that directory, or to all pages in the directory if no index exists.
//!
//! # Inheritance
//!
//! Metadata is inherited from parent directories with specific rules:
//! - `title`: Never inherited (must be set explicitly per page)
//! - `description`: Never inherited (must be set explicitly per page)
//! - `page_type`: Never inherited
//! - `vars`: Deep merged (child values override parent keys)

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::RenderError;

/// Page metadata loaded from YAML sidecar files.
///
/// All fields are optional. When a field is `None`, it indicates the metadata
/// was not explicitly set for this page.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageMetadata {
    /// Custom page title (overrides H1 extraction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Page description for display in navigation or search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Page type (e.g., "domain", "guide", "api").
    /// When set, the page is registered as a section.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub page_type: Option<String>,

    /// Custom variables for templating or frontend use.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub vars: HashMap<String, serde_json::Value>,
}

impl PageMetadata {
    /// Parse metadata from YAML content.
    ///
    /// Returns metadata for valid YAML (empty content returns a default instance).
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is malformed.
    pub fn from_yaml(content: &str) -> Result<Self, MetadataError> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Ok(Self::default());
        }

        serde_yaml::from_str(trimmed)
            .map_err(|e| MetadataError::Parse(format!("Invalid YAML: {e}")))
    }

    /// Check if metadata has any non-default values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.page_type.is_none()
            && self.vars.is_empty()
    }
}

/// Error type for metadata operations.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// YAML parsing error.
    #[error("{0}")]
    Parse(String),
}

/// Merge child metadata with parent metadata following inheritance rules.
///
/// # Inheritance Rules
///
/// - `title`: Never inherited (child's value or `None`)
/// - `description`: Never inherited (child's value or `None`)
/// - `page_type`: Never inherited (child's value or `None`)
/// - `vars`: Deep merged (child values override parent keys)
#[must_use]
pub fn merge_metadata(parent: &PageMetadata, child: &PageMetadata) -> PageMetadata {
    // Start with child values for non-inherited fields
    let mut merged = PageMetadata {
        title: child.title.clone(),             // Never inherited
        description: child.description.clone(), // Never inherited
        page_type: child.page_type.clone(),     // Never inherited
        ..Default::default()
    };

    // Vars: deep merge (parent first, child overrides)
    let mut vars = parent.vars.clone();
    for (key, value) in &child.vars {
        vars.insert(key.clone(), value.clone());
    }
    merged.vars = vars;

    merged
}

/// Get the parent directory of a source file for metadata lookup.
#[must_use]
pub fn metadata_dir(source_path: &Path) -> Option<&Path> {
    source_path.parent()
}

/// Get the metadata file path for a directory.
#[must_use]
pub fn metadata_file_path(dir: &Path, meta_filename: &str) -> std::path::PathBuf {
    if dir.as_os_str().is_empty() {
        std::path::PathBuf::from(meta_filename)
    } else {
        dir.join(meta_filename)
    }
}

impl From<MetadataError> for RenderError {
    fn from(e: MetadataError) -> Self {
        RenderError::Io(std::io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PageMetadata parsing tests

    #[test]
    fn test_parse_empty_yaml() {
        let result = PageMetadata::from_yaml("");
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_parse_whitespace_only() {
        let result = PageMetadata::from_yaml("   \n\t  ");
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_parse_title_only() {
        let yaml = "title: My Page";
        let result = PageMetadata::from_yaml(yaml);
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert_eq!(meta.title, Some("My Page".to_string()));
        assert!(meta.description.is_none());
        assert!(meta.page_type.is_none());
        assert!(meta.vars.is_empty());
    }

    #[test]
    fn test_parse_all_fields() {
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
        let result = PageMetadata::from_yaml(yaml);
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
    fn test_parse_invalid_yaml() {
        let yaml = "title: [invalid yaml";
        let result = PageMetadata::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_field_ignored() {
        let yaml = r"
title: Test
unknown_field: value
";
        let result = PageMetadata::from_yaml(yaml).unwrap();
        assert_eq!(result.title, Some("Test".to_string()));
    }

    // Merge tests

    #[test]
    fn test_merge_empty_parent_and_child() {
        let parent = PageMetadata::default();
        let child = PageMetadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_title_not_inherited() {
        let parent = PageMetadata {
            title: Some("Parent Title".to_string()),
            ..Default::default()
        };
        let child = PageMetadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(merged.title.is_none(), "title should not be inherited");
    }

    #[test]
    fn test_merge_title_child_wins() {
        let parent = PageMetadata {
            title: Some("Parent Title".to_string()),
            ..Default::default()
        };
        let child = PageMetadata {
            title: Some("Child Title".to_string()),
            ..Default::default()
        };
        let merged = merge_metadata(&parent, &child);
        assert_eq!(merged.title, Some("Child Title".to_string()));
    }

    #[test]
    fn test_merge_description_not_inherited() {
        let parent = PageMetadata {
            description: Some("Parent description".to_string()),
            ..Default::default()
        };
        let child = PageMetadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(
            merged.description.is_none(),
            "description should not be inherited"
        );
    }

    #[test]
    fn test_merge_description_child_preserved() {
        let parent = PageMetadata {
            description: Some("Parent description".to_string()),
            ..Default::default()
        };
        let child = PageMetadata {
            description: Some("Child description".to_string()),
            ..Default::default()
        };
        let merged = merge_metadata(&parent, &child);
        assert_eq!(merged.description, Some("Child description".to_string()));
    }

    #[test]
    fn test_merge_page_type_not_inherited() {
        let parent = PageMetadata {
            page_type: Some("domain".to_string()),
            ..Default::default()
        };
        let child = PageMetadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(
            merged.page_type.is_none(),
            "page_type should not be inherited"
        );
    }

    #[test]
    fn test_merge_vars_deep_merged() {
        let mut parent_vars = HashMap::new();
        parent_vars.insert("key1".to_string(), serde_json::json!("parent1"));
        parent_vars.insert("key2".to_string(), serde_json::json!("parent2"));

        let mut child_vars = HashMap::new();
        child_vars.insert("key2".to_string(), serde_json::json!("child2"));
        child_vars.insert("key3".to_string(), serde_json::json!("child3"));

        let parent = PageMetadata {
            vars: parent_vars,
            ..Default::default()
        };
        let child = PageMetadata {
            vars: child_vars,
            ..Default::default()
        };

        let merged = merge_metadata(&parent, &child);

        assert_eq!(merged.vars.get("key1"), Some(&serde_json::json!("parent1")));
        assert_eq!(merged.vars.get("key2"), Some(&serde_json::json!("child2")));
        assert_eq!(merged.vars.get("key3"), Some(&serde_json::json!("child3")));
    }

    // is_empty tests

    #[test]
    fn test_is_empty_default() {
        let meta = PageMetadata::default();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_title() {
        let meta = PageMetadata {
            title: Some("Title".to_string()),
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_vars() {
        let mut vars = HashMap::new();
        vars.insert("key".to_string(), serde_json::json!("value"));
        let meta = PageMetadata {
            vars,
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    // metadata_dir tests

    #[test]
    fn test_metadata_dir_root_file() {
        let path = Path::new("guide.md");
        assert_eq!(metadata_dir(path), Some(Path::new("")));
    }

    #[test]
    fn test_metadata_dir_nested_file() {
        let path = Path::new("domain/guide.md");
        assert_eq!(metadata_dir(path), Some(Path::new("domain")));
    }

    #[test]
    fn test_metadata_dir_index_file() {
        let path = Path::new("domain/index.md");
        assert_eq!(metadata_dir(path), Some(Path::new("domain")));
    }

    // metadata_file_path tests

    #[test]
    fn test_metadata_file_path_root() {
        let path = metadata_file_path(Path::new(""), "meta.yaml");
        assert_eq!(path, std::path::PathBuf::from("meta.yaml"));
    }

    #[test]
    fn test_metadata_file_path_nested() {
        let path = metadata_file_path(Path::new("domain"), "meta.yaml");
        assert_eq!(path, std::path::PathBuf::from("domain/meta.yaml"));
    }
}
