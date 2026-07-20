//! S3 bundle format types.
//!
//! Defines the serialization format for documentation bundles stored in S3.
//! A bundle consists of a manifest (document index) and per-page bundles
//! (markdown content + resolved metadata).

use std::collections::HashMap;

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
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    /// Format version for forward compatibility.
    pub version: u32,
    /// All documents in the site.
    pub documents: Vec<Document>,
    /// Per-page modification times (seconds since Unix epoch).
    /// Populated at publish time from git commit timestamps.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mtimes: HashMap<String, f64>,
}

/// Per-page bundle containing content and resolved metadata.
///
/// Stored at `{prefix}/pages/{path}.json` in S3.
/// `PlantUML` `!include` directives are pre-resolved in the content.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PageBundle {
    /// Markdown content with includes resolved.
    pub content: String,
    /// The page's own resolved metadata.
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

impl From<Vec<Document>> for Manifest {
    /// Create a manifest with the current format version and no mtimes.
    fn from(documents: Vec<Document>) -> Self {
        Self {
            version: FORMAT_VERSION,
            documents,
            mtimes: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use rw_storage::mtime_to_datetime;

    use super::*;

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = Manifest::from(vec![
            Document {
                path: String::new(),
                title: "Home".to_owned(),
                has_content: true,
                page_kind: None,
                namespace: None,
                description: None,
                origin: None,
                pages: None,
                is_dir: true,
            },
            Document {
                path: "guide".to_owned(),
                title: "Guide".to_owned(),
                has_content: true,
                page_kind: Some("guide".to_owned()),
                namespace: None,
                description: Some("Getting started".to_owned()),
                origin: None,
                pages: None,
                is_dir: true,
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
                page_kind: None,
                pages: None,
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
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        };

        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("page_kind"));
        assert!(!json.contains("namespace"));
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

    #[test]
    fn test_manifest_without_mtimes_deserializes() {
        let json =
            r#"{"version":1,"documents":[{"path":"guide","title":"Guide","has_content":true}]}"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert!(manifest.mtimes.is_empty());
    }

    #[test]
    fn test_manifest_with_mtimes_roundtrips() {
        let mut manifest = Manifest::from(vec![Document {
            path: "guide".to_owned(),
            title: "Guide".to_owned(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        }]);
        manifest.mtimes.insert("guide".to_owned(), 1_713_000_000.0);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mtimes.get("guide"), Some(&1_713_000_000.0));
    }

    #[test]
    fn manifest_mtimes_convert_through_a_hand_edited_manifest() {
        // JSON has no NaN or Infinity literals and serde_json rejects
        // out-of-range numbers, so what actually reaches us from a hand-edited
        // or foreign manifest is a finite value — one that may still be
        // negative, or a wrong-unit epoch timestamp far outside chrono's range.
        let json = r#"{"version":1,"documents":[],"mtimes":{"past":-1.0,"nanos":1.75e18}}"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();

        assert_eq!(
            mtime_to_datetime(manifest.mtimes["nanos"]),
            DateTime::UNIX_EPOCH,
            "a nanosecond timestamp in a seconds field denotes no instant"
        );
        assert_eq!(
            mtime_to_datetime(manifest.mtimes["past"]).to_rfc3339(),
            "1969-12-31T23:59:59+00:00",
            "a negative mtime is a real instant before the epoch"
        );
    }

    #[test]
    fn test_manifest_without_pages_deserializes() {
        // Existing manifests in S3 won't have the `pages` field
        let json =
            r#"{"version":1,"documents":[{"path":"guide","title":"Guide","has_content":true}]}"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert!(manifest.documents[0].pages.is_none());
    }

    #[test]
    fn test_manifest_with_pages_roundtrips() {
        let manifest = Manifest::from(vec![Document {
            path: "guides".to_owned(),
            title: "Guides".to_owned(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned(),
            ]),
            is_dir: true,
        }]);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.documents[0].pages,
            Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned()
            ])
        );
    }
}
