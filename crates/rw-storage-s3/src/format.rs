//! S3 bundle format types.
//!
//! Defines the serialization format for documentation bundles stored in S3.
//! A bundle consists of a manifest (document index) and per-page bundles
//! containing rewritten markdown content.

use std::collections::HashMap;
use std::sync::Arc;

use rw_sections::{Namespace, Section};
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

impl Manifest {
    /// Drops each `namespace` and `name` outside the namespace grammar, and each
    /// `kind` a ref could not parse back, with a warning.
    ///
    /// The site treats storage metadata as already validated and panics on an
    /// invalid identifier, but a manifest from an older or foreign publisher,
    /// or a hand-edited one, never passed through `rw-meta` resolution.
    pub(crate) fn drop_invalid_identifiers(&mut self) {
        let is_identifier = |raw: &str| raw.parse::<Namespace>().is_ok();
        for document in &mut self.documents {
            let meta = Arc::make_mut(&mut document.meta);
            let path = &document.path;
            drop_invalid(path, "kind", &mut meta.kind, Section::is_valid_kind);
            drop_invalid(path, "namespace", &mut meta.namespace, is_identifier);
            drop_invalid(path, "name", &mut meta.name, is_identifier);
        }
    }
}

fn drop_invalid(
    path: &str,
    field: &str,
    value: &mut Option<String>,
    is_valid: impl Fn(&str) -> bool,
) {
    if let Some(raw) = value.take_if(|raw| !is_valid(raw)) {
        tracing::warn!("manifest page {path:?}: dropping invalid {field} {raw:?}");
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use parking_lot::Mutex;
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

    #[test]
    fn attrs_transport_depth_boundary_roundtrips_manifest_bytes() {
        for shape in ["array", "object", "mixed"] {
            for (leaf, leaf_value) in [
                ("null", serde_json::Value::Null),
                ("[]", serde_json::json!([])),
                ("{}", serde_json::json!({})),
            ] {
                let mut expected = leaf_value;
                for level in usize::from(leaf != "null")..123 {
                    expected = if shape == "array" || (shape == "mixed" && level % 2 == 0) {
                        serde_json::Value::Array(vec![expected])
                    } else {
                        serde_json::json!({"child": expected})
                    };
                }
                let value = serde_json::to_string(&expected).unwrap();
                let resolved = rw_meta::Meta::resolve_with_diagnostics(
                    None,
                    Some(&format!("attrs: {{value: {value}}}")),
                    "guide",
                );
                assert_eq!(resolved.diagnostics, [] as [rw_meta::Diagnostic; 0]);
                assert_eq!(resolved.meta.attrs["value"], expected, "{shape}/{leaf}");
                let manifest = Manifest::from(vec![Document {
                    path: "guide".to_owned(),
                    has_content: true,
                    meta: Arc::new(resolved.meta),
                    origin: None,
                    is_dir: false,
                    readme: false,
                    diagnostics: Vec::new().into(),
                }]);
                let bytes = serde_json::to_vec(&manifest).unwrap();
                let decoded: Manifest = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    decoded.documents[0].meta.attrs["value"], expected,
                    "{shape}/{leaf}"
                );
                assert_eq!(decoded, manifest, "{shape}/{leaf}");
            }
        }
    }

    #[test]
    fn manifest_new_reader_loads_the_legacy_flat_document_wire() {
        let old: Manifest = serde_json::from_str(LEGACY_MANIFEST_JSON).unwrap();

        assert_eq!(serde_json::to_string(&old).unwrap(), LEGACY_MANIFEST_JSON);
        assert_eq!(old.documents[0].meta.name, None);
        assert_eq!(old.version, 1);
        assert!(old.documents[0].meta.attrs.is_empty());
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
        let new: Manifest = serde_json::from_str(LEGACY_MANIFEST_JSON).unwrap();

        let old_reader: LegacyManifest =
            serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();

        assert_eq!(
            old_reader,
            serde_json::from_str::<LegacyManifest>(LEGACY_MANIFEST_JSON).unwrap()
        );
    }

    #[test]
    fn attrs_are_additive_in_version_one_and_ignored_by_old_readers() {
        let mut manifest: Manifest = serde_json::from_str(LEGACY_MANIFEST_JSON).unwrap();
        let attrs =
            serde_json::json!({"owner": null, "nested": {"audiences": ["operators", true, 3]}});
        Arc::make_mut(&mut manifest.documents[0].meta).attrs =
            serde_json::from_value(attrs.clone()).unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        let wire: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(wire["version"], 1);
        assert_eq!(wire["documents"][0]["attrs"], attrs);
        let current: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(current, manifest);
        let old: LegacyManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&old).unwrap(), LEGACY_MANIFEST_JSON);
        Arc::make_mut(&mut manifest.documents[0].meta).attrs.clear();
        assert_eq!(
            serde_json::to_string(&manifest).unwrap(),
            LEGACY_MANIFEST_JSON
        );
    }

    #[test]
    fn declared_name_manifest_roundtrip_is_syntactic_not_legacy_semantic_parity() {
        let mut wire: serde_json::Value = serde_json::from_str(LEGACY_MANIFEST_JSON).unwrap();
        wire["documents"][0]["name"] = serde_json::json!("payments-api");
        let new: Manifest = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(new.version, 1);
        assert_eq!(new.documents[0].meta.name.as_deref(), Some("payments-api"));
        assert_eq!(serde_json::to_value(&new).unwrap(), wire);
        let legacy: LegacyManifest = serde_json::from_value(wire).unwrap();
        let lost = serde_json::to_value(legacy).unwrap();
        assert!(lost["documents"][0].get("name").is_none());
        let reread: Manifest = serde_json::from_value(lost).unwrap();
        assert_eq!(reread.documents[0].meta.name, None);
        assert_eq!(reread.documents[0].path, "guide");
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
        let manifest: Manifest = serde_json::from_str(r#"{"version":1,"documents":[]}"#).unwrap();
        assert!(manifest.mtimes.is_empty());
    }

    #[derive(Clone, Default)]
    struct Logs(Arc<Mutex<Vec<u8>>>);

    impl Write for Logs {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn invalid_manifest_identifiers_are_dropped_with_a_warning() {
        let json = r#"{"version":1,"documents":[
            {"path":"systems","title":"Systems","has_content":true,"page_kind":"domain","namespace":"bad/ns","name":""},
            {"path":"systems/api","title":"API","has_content":true,"page_kind":"system","namespace":"payments","name":"payments-api"},
            {"path":"empty","title":"Empty","has_content":true,"page_kind":""},
            {"path":"colon","title":"Colon","has_content":true,"page_kind":"a:b"}
        ]}"#;
        let mut manifest: Manifest = serde_json::from_str(json).unwrap();
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();

        tracing::subscriber::with_default(subscriber, || manifest.drop_invalid_identifiers());

        let invalid = &manifest.documents[0].meta;
        assert_eq!(invalid.namespace, None);
        assert_eq!(invalid.name, None);
        assert_eq!(invalid.kind.as_deref(), Some("domain"));
        let valid = &manifest.documents[1].meta;
        assert_eq!(valid.namespace.as_deref(), Some("payments"));
        assert_eq!(valid.name.as_deref(), Some("payments-api"));
        assert_eq!(valid.kind.as_deref(), Some("system"));
        assert_eq!(manifest.documents[2].meta.kind, None);
        assert_eq!(manifest.documents[3].meta.kind, None);
        let logs = String::from_utf8(logs.0.lock().clone()).unwrap();
        assert!(
            logs.contains(r#"manifest page "systems": dropping invalid namespace "bad/ns""#),
            "{logs}"
        );
        assert!(
            logs.contains(r#"manifest page "systems": dropping invalid name """#),
            "{logs}"
        );
        assert!(
            logs.contains(r#"manifest page "empty": dropping invalid kind """#),
            "{logs}"
        );
        assert!(
            logs.contains(r#"manifest page "colon": dropping invalid kind "a:b""#),
            "{logs}"
        );
        assert_eq!(logs.lines().count(), 4, "{logs}");
    }
}
