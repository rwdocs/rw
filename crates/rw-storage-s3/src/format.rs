//! S3 bundle format types.
//!
//! Defines the serialization format for documentation bundles stored in S3.
//! A bundle consists of a manifest (document index) and per-page bundles
//! containing rewritten markdown content.

use std::collections::HashMap;

use rw_storage::Document;
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

/// Per-page bundle containing rendered content.
///
/// Stored at `{prefix}/pages/{path}.json` in S3.
/// `PlantUML` `!include` directives are pre-resolved in the content.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageBundle {
    /// Markdown content with includes resolved.
    pub content: String,
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
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct LegacyDocument {
        path: String,
        title: String,
        has_content: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pages: Option<Vec<String>>,
        #[serde(default = "legacy_default_is_dir")]
        is_dir: bool,
    }

    fn legacy_default_is_dir() -> bool {
        true
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct LegacyManifest {
        version: u32,
        documents: Vec<LegacyDocument>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        mtimes: HashMap<String, f64>,
    }

    const LEGACY_MANIFEST_JSON: &str = r#"{"version":1,"documents":[{"path":"guide","title":"Guide","has_content":true,"page_kind":"domain","namespace":"payments","description":"Getting started","origin":"docs","pages":["intro"],"is_dir":false}],"mtimes":{"guide":1713000000.0}}"#;

    #[allow(clippy::too_many_arguments)]
    fn document_from_wire(
        path: impl Into<String>,
        has_content: bool,
        title: impl Into<String>,
        kind: Option<&str>,
        namespace: Option<&str>,
        description: Option<&str>,
        origin: Option<&str>,
        pages: Option<Vec<&str>>,
        is_dir: bool,
    ) -> Document {
        let mut wire = serde_json::json!({
            "path": path.into(),
            "title": title.into(),
            "has_content": has_content,
            "is_dir": is_dir,
        });

        if let Some(kind) = kind {
            wire["page_kind"] = serde_json::json!(kind);
        }
        if let Some(namespace) = namespace {
            wire["namespace"] = serde_json::json!(namespace);
        }
        if let Some(description) = description {
            wire["description"] = serde_json::json!(description);
        }
        if let Some(origin) = origin {
            wire["origin"] = serde_json::json!(origin);
        }
        if let Some(pages) = pages {
            wire["pages"] = serde_json::json!(pages);
        }

        serde_json::from_value(wire).unwrap()
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = Manifest::from(vec![
            document_from_wire("", true, "Home", None, None, None, None, None, true),
            document_from_wire(
                "guide",
                true,
                "Guide",
                Some("guide"),
                None,
                Some("Getting started"),
                None,
                None,
                true,
            ),
        ]);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest, deserialized);
        assert_eq!(deserialized.version, FORMAT_VERSION);
        assert_eq!(deserialized.documents.len(), 2);
    }

    #[test]
    fn manifest_new_reader_loads_the_legacy_flat_document_wire() {
        let old: Manifest = serde_json::from_str(LEGACY_MANIFEST_JSON).unwrap();

        assert_eq!(old.documents[0].meta.kind.as_deref(), Some("domain"));
        assert_eq!(old.documents[0].meta.namespace.as_deref(), Some("payments"));
        assert_eq!(
            old.documents[0].meta.description.as_deref(),
            Some("Getting started")
        );
        assert_eq!(
            old.documents[0].meta.pages.as_ref(),
            Some(&vec!["intro".to_owned()])
        );
        assert_eq!(old.documents[0].origin.as_deref(), Some("docs"));
        assert!(!old.documents[0].is_dir);
    }

    #[test]
    fn manifest_old_reader_loads_the_new_flattened_document_wire() {
        let new_manifest = Manifest {
            version: FORMAT_VERSION,
            documents: vec![document_from_wire(
                "guide",
                true,
                "Guide",
                Some("domain"),
                Some("payments"),
                Some("Getting started"),
                Some("docs"),
                Some(vec!["intro"]),
                false,
            )],
            mtimes: HashMap::from([("guide".to_owned(), 1_713_000_000.0)]),
        };

        let new_json = serde_json::to_string(&new_manifest).unwrap();
        let old_reader: LegacyManifest = serde_json::from_str(&new_json).unwrap();

        assert_eq!(old_reader.documents[0].title, "Guide");
        assert_eq!(old_reader.documents[0].page_kind.as_deref(), Some("domain"));
        assert_eq!(
            old_reader.documents[0].namespace.as_deref(),
            Some("payments")
        );
        assert_eq!(
            old_reader.documents[0].description.as_deref(),
            Some("Getting started")
        );
        assert_eq!(
            old_reader.documents[0].pages.as_ref(),
            Some(&vec!["intro".to_owned()])
        );
        assert_eq!(old_reader.documents[0].origin.as_deref(), Some("docs"));
        assert!(!old_reader.documents[0].is_dir);
    }

    #[test]
    fn test_page_bundle_serialization_roundtrip() {
        let bundle = PageBundle {
            content: "# Hello\n\nWorld".to_owned(),
        };

        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: PageBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(bundle, deserialized);
    }

    #[test]
    fn content_only_bundle_ignores_legacy_metadata() {
        let json = r##"{
      "content":"# Hello",
      "metadata":{"title":"Hello","kind":"domain","vars":{"team":"platform"}}
    }"##;
        let bundle: PageBundle = serde_json::from_str(json).unwrap();
        assert_eq!(bundle.content, "# Hello");
    }

    #[derive(Deserialize)]
    struct LegacyPageBundle {
        content: String,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
    }

    #[test]
    fn legacy_reader_loads_new_content_only_bundle_without_metadata() {
        let bundle = PageBundle {
            content: "# Hello".to_owned(),
        };

        let json = serde_json::to_string(&bundle).unwrap();
        let legacy: LegacyPageBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(legacy.content, "# Hello");
        assert!(legacy.metadata.is_none());
    }

    #[test]
    fn test_document_skips_none_fields() {
        let doc = document_from_wire("guide", true, "Guide", None, None, None, None, None, true);

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
        let mut manifest = Manifest::from(vec![document_from_wire(
            "guide", true, "Guide", None, None, None, None, None, true,
        )]);
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
        assert!(manifest.documents[0].meta.pages.is_none());
    }

    #[test]
    fn test_manifest_with_pages_roundtrips() {
        let manifest = Manifest::from(vec![document_from_wire(
            "guides",
            true,
            "Guides",
            None,
            None,
            None,
            None,
            Some(vec!["getting-started", "configuration"]),
            true,
        )]);

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.documents[0].meta.pages,
            Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned()
            ])
        );
    }
}
