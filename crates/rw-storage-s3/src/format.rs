//! S3 bundle format types.
//!
//! Defines the serialization format for documentation bundles stored in S3.
//! A bundle consists of a manifest (document index) and per-page bundles
//! (markdown content + resolved metadata).

use rw_storage::{Document, Metadata};
use serde::{Deserialize, Serialize};

/// Current bundle format version.
pub const FORMAT_VERSION: u32 = 1;

/// S3 key for the manifest file (relative to prefix).
pub(crate) const MANIFEST_KEY: &str = "manifest.json";

/// Manifest containing the document index.
///
/// Stored at `{prefix}/manifest.json` in S3.
/// Contains everything needed for `Storage::scan()`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Format version for forward compatibility.
    pub version: u32,
    /// All documents in the site.
    pub documents: Vec<Document>,
}

/// Per-page bundle containing content and resolved metadata.
///
/// Stored at `{prefix}/pages/{path}.json` in S3.
/// `PlantUML` `!include` directives are pre-resolved in the content.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PageBundle {
    /// Markdown content with includes resolved.
    pub content: String,
    /// Fully resolved metadata (inheritance already applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Convert a URL path to the S3 key for its page bundle.
///
/// Root path (`""`) maps to `pages/_index.json`.
/// Other paths map to `pages/{path}.json`.
#[must_use]
pub(crate) fn page_bundle_key(path: &str) -> String {
    if path.is_empty() {
        "pages/_index.json".to_owned()
    } else {
        format!("pages/{path}.json")
    }
}

impl Manifest {
    /// Create a new manifest with the current format version.
    #[must_use]
    pub fn new(documents: Vec<Document>) -> Self {
        Self {
            version: FORMAT_VERSION,
            documents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = Manifest::new(vec![
            Document {
                path: String::new(),
                title: "Home".to_owned(),
                has_content: true,
                page_type: None,
                description: None,
            },
            Document {
                path: "guide".to_owned(),
                title: "Guide".to_owned(),
                has_content: true,
                page_type: Some("guide".to_owned()),
                description: Some("Getting started".to_owned()),
            },
        ]);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest, deserialized);
        assert_eq!(deserialized.version, FORMAT_VERSION);
        assert_eq!(deserialized.documents.len(), 2);
    }

    #[test]
    fn test_page_bundle_serialization_roundtrip() {
        let bundle = PageBundle {
            content: "# Hello\n\nWorld".to_owned(),
            metadata: Some(Metadata {
                title: Some("Hello".to_owned()),
                description: None,
                page_type: None,
                vars: [("team".to_owned(), serde_json::json!("platform"))]
                    .into_iter()
                    .collect(),
            }),
        };

        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: PageBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(bundle, deserialized);
    }

    #[test]
    fn test_page_bundle_no_metadata() {
        let bundle = PageBundle {
            content: "# Hello".to_owned(),
            metadata: None,
        };

        let json = serde_json::to_string(&bundle).unwrap();
        assert!(!json.contains("metadata"));

        let deserialized: PageBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle, deserialized);
    }

    #[test]
    fn test_document_skips_none_fields() {
        let doc = Document {
            path: "guide".to_owned(),
            title: "Guide".to_owned(),
            has_content: true,
            page_type: None,
            description: None,
        };

        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("page_type"));
        assert!(!json.contains("description"));
    }

    #[test]
    fn test_page_bundle_key_root() {
        assert_eq!(page_bundle_key(""), "pages/_index.json");
    }

    #[test]
    fn test_page_bundle_key_simple() {
        assert_eq!(page_bundle_key("guide"), "pages/guide.json");
    }

    #[test]
    fn test_page_bundle_key_nested() {
        assert_eq!(
            page_bundle_key("domain/billing"),
            "pages/domain/billing.json"
        );
    }
}
