//! Page metadata types for storage backends.
//!
//! Provides the [`Metadata`] struct for storing page-level configuration.
//! This module contains only data types - parsing and inheritance logic
//! is implemented by individual storage backends.
//!
//! # Metadata Fields
//!
//! - `title`: Custom page title (overrides H1 extraction)
//! - `description`: Page description for display
//! - `page_type`: Page type (e.g., "domain", "guide")
//! - `vars`: Custom variables for templating

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Page metadata loaded from sidecar files.
///
/// All fields are optional. When a field is `None`, it indicates the metadata
/// was not explicitly set for this page.
///
/// This struct is serialization-friendly and can be used by backends
/// that store metadata in various formats (YAML, JSON, TOML, etc.).
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
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

impl Metadata {
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
    /// Parsing error (format-specific).
    #[error("{0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_default() {
        let meta = Metadata::default();
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
        assert!(meta.page_type.is_none());
        assert!(meta.vars.is_empty());
    }

    #[test]
    fn test_is_empty_default() {
        let meta = Metadata::default();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_title() {
        let meta = Metadata {
            title: Some("Title".to_owned()),
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_description() {
        let meta = Metadata {
            description: Some("Desc".to_owned()),
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_page_type() {
        let meta = Metadata {
            page_type: Some("domain".to_owned()),
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn test_is_empty_with_vars() {
        let mut vars = HashMap::new();
        vars.insert("key".to_owned(), serde_json::json!("value"));
        let meta = Metadata {
            vars,
            ..Default::default()
        };
        assert!(!meta.is_empty());
    }

    #[test]
    fn test_metadata_equality() {
        let meta1 = Metadata {
            title: Some("Title".to_owned()),
            ..Default::default()
        };
        let meta2 = Metadata {
            title: Some("Title".to_owned()),
            ..Default::default()
        };
        assert_eq!(meta1, meta2);
    }

}
