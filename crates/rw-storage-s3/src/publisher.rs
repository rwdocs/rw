//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3 with the files pages show.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use aws_sdk_s3::Client;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use rw_parser::rewrite_fences;
use rw_plantuml::bundle_source;
use rw_storage::{
    Document, MAX_FILE_SIZE, Storage, StorageError, StorageErrorKind, file_content_type,
};
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;

use crate::format::{self, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

/// Errors that can occur during publishing.
#[derive(Debug, thiserror::Error)]
pub enum BundlePublishError {
    #[error("Storage error")]
    Storage(#[from] StorageError),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("S3 error")]
    S3(#[source] Box<SdkError<PutObjectError>>),
    #[error("pages {first:?} and {second:?} both publish to {key:?}; rename one of them")]
    DuplicateBundleKey {
        key: String,
        first: String,
        second: String,
    },
}

/// Publisher that builds and uploads documentation bundles to S3.
pub struct BundlePublisher {
    config: S3Config,
}

/// Outcome of a publish run.
///
/// Warnings are accumulated from `PlantUML` `!include` resolution
/// (broken include paths, cyclic includes) across every page, followed by
/// one per file that was not published.
/// Runtime diagnostics such as unknown attributes or invalid `format`
/// values fire later, when a page is actually rendered, and are not
/// captured here.
///
/// Repeated identical warnings are deduplicated so a missing shared
/// include referenced by many pages reads as a single entry.
///
/// `rw backstage publish --strict` exits non-zero when this vector
/// is non-empty.
#[derive(Debug, Clone)]
pub struct PublishReport {
    /// Number of objects uploaded (page bundles, files and the manifest).
    pub uploaded: usize,
    /// Deduplicated warnings accumulated across all pages and files.
    pub warnings: Vec<String>,
}

impl BundlePublisher {
    #[must_use]
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }

    /// Publish documentation from a storage backend to S3.
    ///
    /// Scans the storage, builds bundles with pre-resolved `PlantUML`
    /// includes, streams them to S3 (uploads start as soon as each bundle
    /// is ready), uploads each of `files` once per key (content and
    /// extension), and returns a [`PublishReport`] with the upload count and
    /// the warnings (see [`PublishReport`] for what is and isn't captured).
    ///
    /// `files` are project paths, as [`Storage::read_file`] takes them. One
    /// that storage doesn't serve or that the reader would refuse (not an
    /// allowlisted type, or larger than [`MAX_FILE_SIZE`]) is skipped with a
    /// warning.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
        files: &BTreeSet<String>,
    ) -> Result<PublishReport, BundlePublishError> {
        let documents = storage.scan()?;
        check_unique_bundle_keys(&documents)?;
        let mut uploads = Uploads::new(&self.config).await;

        let mut warnings = Vec::new();
        let bundle_hashes = build_bundles(
            storage,
            &documents,
            include_dirs,
            &mut warnings,
            async |key, json| uploads.spawn(key, json, "application/json").await,
        )
        .await?;
        let file_hashes = build_files(
            storage,
            files,
            &mut warnings,
            async |key, bytes, content_type| uploads.spawn(key, bytes, content_type).await,
        )
        .await?;
        let uploaded = uploads.finish().await?;

        // Upload manifest last so readers don't see a manifest referencing
        // objects that haven't been uploaded yet.
        let manifest = build_manifest(storage, documents, bundle_hashes, file_hashes);
        let manifest_json = serde_json::to_vec(&manifest)?;
        s3::upload(
            &uploads.client,
            &self.config,
            MANIFEST_KEY,
            manifest_json,
            "application/json",
        )
        .await?;

        Ok(PublishReport {
            uploaded: uploaded + 1,
            warnings,
        })
    }
}

/// Uploads running in the background, at most [`Self::MAX_CONCURRENT`] at
/// once, so memory stays bounded by that rather than by the size of the site.
struct Uploads {
    client: Client,
    config: Arc<S3Config>,
    tasks: JoinSet<Result<(), BundlePublishError>>,
    spawned: usize,
}

impl Uploads {
    const MAX_CONCURRENT: usize = 32;

    async fn new(config: &S3Config) -> Self {
        Self {
            client: s3::build_client(config).await,
            config: Arc::new(config.clone()),
            tasks: JoinSet::new(),
            spawned: 0,
        }
    }

    /// Starts uploading `body` to `key`, first waiting for a slot.
    async fn spawn(
        &mut self,
        key: String,
        body: Vec<u8>,
        content_type: &'static str,
    ) -> Result<(), BundlePublishError> {
        if self.tasks.len() >= Self::MAX_CONCURRENT {
            self.tasks
                .join_next()
                .await
                .expect("task set is non-empty")
                .expect("upload task panicked")?;
        }
        let client = self.client.clone();
        let config = Arc::clone(&self.config);
        self.tasks
            .spawn(async move { s3::upload(&client, &config, &key, body, content_type).await });
        self.spawned += 1;
        Ok(())
    }

    /// Waits for every upload, and returns how many were started.
    async fn finish(&mut self) -> Result<usize, BundlePublishError> {
        while let Some(result) = self.tasks.join_next().await {
            result.expect("upload task panicked")?;
        }
        Ok(self.spawned)
    }
}

/// The manifest for `documents`, as scanned from `storage`, with `bundles`
/// mapping each page to the SHA-256 of its bundle and `files` each published
/// project file to the SHA-256 of its content.
fn build_manifest(
    storage: &dyn Storage,
    documents: Vec<Document>,
    bundles: HashMap<String, String>,
    files: HashMap<String, String>,
) -> Manifest {
    let mut mtimes = HashMap::new();
    for doc in &documents {
        if let Ok(mtime) = storage.mtime(&doc.path) {
            mtimes.insert(doc.path.clone(), mtime);
        }
    }

    let mut manifest = Manifest::from(documents);
    manifest.mtimes = mtimes;
    manifest.source_dir = storage.source_dir_in_project();
    manifest.bundles = bundles;
    manifest.files = files;
    manifest
}

/// Build one page bundle per document that has content, handing each
/// `(key, bundle_json)` pair to `on_bundle` as soon as it is ready.
///
/// Returns each page's URL path mapped to the SHA-256 of its bundle JSON, and
/// adds the deduplicated `!include` resolution warnings to `warnings`.
///
/// Bundles are handed over one at a time instead of collected so a caller that
/// uploads them can keep only a bounded number in memory at once, whatever the
/// size of the site. Construction is sequential because include-resolution
/// warnings accumulate across pages, in document order.
async fn build_bundles(
    storage: &dyn Storage,
    documents: &[Document],
    include_dirs: &[PathBuf],
    warnings: &mut Vec<String>,
    mut on_bundle: impl AsyncFnMut(String, Vec<u8>) -> Result<(), BundlePublishError>,
) -> Result<HashMap<String, String>, BundlePublishError> {
    let mut hashes = HashMap::new();

    for doc in documents {
        if !doc.has_content {
            continue;
        }

        let content = storage.read(&doc.path)?;
        let resolved_content = rewrite_fences(&content, |lang, src| {
            bundle_source(lang, src, include_dirs, warnings)
        });

        let bundle = PageBundle {
            content: resolved_content,
        };

        let bundle_json = serde_json::to_vec(&bundle)?;
        let key = format::page_bundle_key(&doc.path);

        hashes.insert(doc.path.clone(), hex::encode(Sha256::digest(&bundle_json)));
        on_bundle(key, bundle_json).await?;
    }

    let mut seen = HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok(hashes)
}

/// Reads each of `paths` from `storage` and hands `(key, bytes, content_type)`
/// to `on_file` once per key (content and extension), so identical content
/// under one extension uploads once.
///
/// Returns each published path mapped to the SHA-256 of its content, and adds
/// a warning to `warnings` for each path it skips.
async fn build_files(
    storage: &dyn Storage,
    paths: &BTreeSet<String>,
    warnings: &mut Vec<String>,
    mut on_file: impl AsyncFnMut(String, Vec<u8>, &'static str) -> Result<(), BundlePublishError>,
) -> Result<HashMap<String, String>, BundlePublishError> {
    let mut hashes = HashMap::new();
    let mut keys = HashSet::new();

    for path in paths {
        let bytes = match storage.read_file(path) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind == StorageErrorKind::NotFound => None,
            Err(err) => return Err(err.into()),
        };
        // The reader refuses these whatever storage served.
        let file = bytes
            .filter(|bytes| bytes.len() as u64 <= MAX_FILE_SIZE)
            .zip(file_content_type(path));
        let Some((bytes, content_type)) = file else {
            warnings.push(format!(
                "image `{path}` not published: missing, outside the project, \
                 not a supported image type, or over {} MiB",
                MAX_FILE_SIZE >> 20
            ));
            continue;
        };

        let sha256 = hex::encode(Sha256::digest(&bytes));
        let key = format::file_key(path, &sha256).expect("an allowlisted path and a SHA-256");
        hashes.insert(path.clone(), sha256);
        if keys.insert(key.clone()) {
            on_file(key, bytes, content_type).await?;
        }
    }

    Ok(hashes)
}

/// Fails when two pages with content map to one bundle key.
///
/// Uploads run concurrently, so pages sharing a key would race. Only the
/// root's `pages/_index.json` can be shared, with a page at `_index`.
fn check_unique_bundle_keys(documents: &[Document]) -> Result<(), BundlePublishError> {
    let mut owners: HashMap<String, &str> = HashMap::new();
    for doc in documents.iter().filter(|doc| doc.has_content) {
        let key = format::page_bundle_key(&doc.path);
        if let Some(&owner) = owners.get(&key) {
            let (first, second) = if owner <= doc.path.as_str() {
                (owner, doc.path.as_str())
            } else {
                (doc.path.as_str(), owner)
            };
            return Err(BundlePublishError::DuplicateBundleKey {
                first: format!("/{first}"),
                second: format!("/{second}"),
                key,
            });
        }
        owners.insert(key, &doc.path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_storage::MockStorage;

    async fn run_build_bundles(
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> (HashMap<String, String>, Vec<String>, Vec<(String, Vec<u8>)>) {
        let documents = storage.scan().unwrap();
        let mut warnings = Vec::new();
        let mut uploads = Vec::new();
        let hashes = build_bundles(
            storage,
            &documents,
            include_dirs,
            &mut warnings,
            async |key, json| {
                uploads.push((key, json));
                Ok(())
            },
        )
        .await
        .unwrap();
        (hashes, warnings, uploads)
    }

    /// Exercises everything `publish()` does apart from the S3 calls: the
    /// bundle-building loop, the fence rewrite that inlines `!include`s, and
    /// the dedup that folds one broken shared include into a single warning.
    /// Pages `a` and `b` share a broken include (two raw warnings, one
    /// deduplicated); page `c` has a resolvable one and must come out inlined.
    #[tokio::test]
    async fn build_bundles_inlines_includes_and_dedups_warnings() {
        let include_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(include_dir.path().join("shared.iuml"), "Bob -> Charlie")
            .expect("write include");

        let broken = "\
# Page

```plantuml
@startuml
!include nonexistent.iuml
A -> B
@enduml
```
";
        let resolvable = "\
# Page

```plantuml
@startuml
!include shared.iuml
@enduml
```
";
        let storage = MockStorage::new()
            .with_document("a", "A")
            .with_content("a", broken)
            .with_document("b", "B")
            .with_content("b", broken)
            .with_document("c", "C")
            .with_content("c", resolvable);

        let (_, warnings, bundles) =
            run_build_bundles(&storage, &[include_dir.path().to_path_buf()]).await;

        assert_eq!(bundles.len(), 3);
        assert_eq!(warnings.len(), 1, "deduplicated warnings: {warnings:?}");
        assert!(
            warnings[0].contains("Include file not found")
                && warnings[0].contains("nonexistent.iuml"),
            "unexpected warning: {}",
            warnings[0],
        );

        let (key, json) = bundles
            .iter()
            .find(|(key, _)| key == "pages/c.json")
            .expect("a bundle per page with content");
        let bundle: PageBundle = serde_json::from_slice(json).expect("valid page bundle JSON");
        assert!(
            bundle.content.contains("Bob -> Charlie"),
            "{key} kept the include unresolved: {}",
            bundle.content
        );
        assert!(
            !bundle.content.contains("!include"),
            "{key} still carries an !include: {}",
            bundle.content
        );
    }

    #[tokio::test]
    async fn build_bundles_preserves_declared_name_includes_direct_and_nested() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("shared.iuml"),
            "!include systems/ext/sys_payments_api.iuml\nAlice -> Bob",
        )
        .unwrap();
        let document: Document = serde_json::from_value(serde_json::json!({
            "path":"systems/payments-guide", "title":"Payments documentation",
            "has_content":true, "page_kind":"system", "namespace":"commerce", "name":"payments-api"
        }))
        .unwrap();
        let storage = MockStorage::new().with_scanned_document(document).with_content(
            "systems/payments-guide",
            "# Payments\n\n```plantuml\n@startuml\n!include systems/sys_payments_api.iuml\n!include shared.iuml\n@enduml\n```\n",
        );
        let (_, warnings, bundles) =
            run_build_bundles(&storage, &[temp.path().to_path_buf()]).await;
        assert_eq!(bundles.len(), 1);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(bundles[0].0, "pages/systems/payments-guide.json");
        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        assert!(
            bundle
                .content
                .contains("!include systems/sys_payments_api.iuml")
        );
        assert!(
            bundle
                .content
                .contains("!include systems/ext/sys_payments_api.iuml")
        );
        assert!(bundle.content.contains("Alice -> Bob"));
        assert!(!bundle.content.contains("!include shared.iuml"));
        assert!(!bundle.content.contains("System("));
    }

    #[tokio::test]
    async fn build_bundles_skips_virtual_pages_and_keeps_raw_content() {
        let storage = MockStorage::new()
            .with_document("selected", "Selected")
            .with_virtual_page("virtual", "title: Virtual")
            .with_content(
                "selected",
                "---\nattrs: {owner: raw, unexpected: true}\n---\n# Body\n",
            );
        let (_, warnings, bundles) = run_build_bundles(&storage, &[]).await;
        assert_eq!(bundles.len(), 1);
        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(bundles[0].0, "pages/selected.json");
        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        assert!(bundle.content.contains("owner: raw"));
    }

    #[tokio::test]
    async fn build_bundles_hashes_each_uploaded_bundle_by_page() {
        let storage = MockStorage::new()
            .with_document("", "Home")
            .with_content("", "# Home\n")
            .with_document("guide", "Guide")
            .with_content("guide", "# Guide\n")
            .with_virtual_page("virtual", "title: Virtual");

        let (hashes, _, bundles) = run_build_bundles(&storage, &[]).await;

        let uploaded = |key: &str| {
            let (_, json) = bundles.iter().find(|(k, _)| k == key).unwrap();
            hex::encode(Sha256::digest(json))
        };
        assert_eq!(
            hashes,
            HashMap::from([
                (String::new(), uploaded("pages/_index.json")),
                ("guide".to_owned(), uploaded("pages/guide.json")),
            ])
        );
    }

    #[tokio::test]
    async fn editing_an_included_file_changes_the_bundle_hash() {
        let include_dir = tempfile::tempdir().unwrap();
        let include_dirs = [include_dir.path().to_path_buf()];
        let storage = MockStorage::new()
            .with_document("guide", "Guide")
            .with_content("guide", "```plantuml\n!include shared.iuml\n```\n");
        let hash_with = async |include: &str| {
            std::fs::write(include_dir.path().join("shared.iuml"), include).unwrap();
            run_build_bundles(&storage, &include_dirs).await.0["guide"].clone()
        };

        assert_ne!(hash_with("A -> B").await, hash_with("C -> D").await);
    }

    /// `docs/_index.md` scans to `_index`, whose bundle key is the root's.
    /// The endpoint refuses connections, so reaching the network would
    /// surface an S3 error instead.
    #[tokio::test]
    async fn publish_rejects_shared_bundle_key_before_uploading() {
        let storage = MockStorage::new()
            .with_document("_index", "Underscore")
            .with_content("_index", "# Underscore\n")
            .with_document("", "Home")
            .with_content("", "# Home\n");
        let publisher = BundlePublisher::new(S3Config {
            bucket: "bucket".to_owned(),
            prefix: "default/component/x".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: Some("http://127.0.0.1:1".to_owned()),
            bucket_root_path: None,
            access_key_id: Some("id".to_owned()),
            secret_access_key: Some("secret".to_owned()),
        });

        let err = publisher
            .publish(&storage, &[], &BTreeSet::new())
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            r#"pages "/" and "/_index" both publish to "pages/_index.json"; rename one of them"#
        );
    }

    #[test]
    fn build_manifest_records_mtimes_the_source_dir_bundles_and_files() {
        let storage = MockStorage::new()
            .with_document("guide", "Guide")
            .with_mtime("guide", 1_713_000_000.0)
            .with_source_dir_in_project("site/docs");
        let bundles = HashMap::from([("guide".to_owned(), PNG_SHA256.to_owned())]);
        let files = HashMap::from([("site/docs/a.png".to_owned(), PNG_SHA256.to_owned())]);

        let manifest = build_manifest(
            &storage,
            storage.scan().unwrap(),
            bundles.clone(),
            files.clone(),
        );

        assert_eq!(
            manifest.mtimes,
            HashMap::from([("guide".to_owned(), 1_713_000_000.0)])
        );
        assert_eq!(manifest.source_dir.as_deref(), Some("site/docs"));
        assert_eq!(manifest.bundles, bundles);
        assert_eq!(manifest.files, files);
    }

    /// SHA-256 of `b"png"`.
    const PNG_SHA256: &str = "8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c";

    async fn run_build_files(
        storage: &dyn Storage,
        paths: &[&str],
    ) -> (
        HashMap<String, String>,
        Vec<String>,
        Vec<(String, Vec<u8>, &'static str)>,
    ) {
        let paths = paths.iter().map(|&path| path.to_owned()).collect();
        let mut warnings = Vec::new();
        let mut uploads = Vec::new();
        let hashes = build_files(
            storage,
            &paths,
            &mut warnings,
            async |key, bytes, content_type| {
                uploads.push((key, bytes, content_type));
                Ok(())
            },
        )
        .await
        .unwrap();
        (hashes, warnings, uploads)
    }

    #[tokio::test]
    async fn build_files_uploads_each_content_once_per_extension() {
        let storage = MockStorage::new()
            .with_project_file("docs/a.png", b"png".as_slice())
            .with_project_file("docs/copy.PNG", b"png".as_slice())
            .with_project_file("docs/as.jpg", b"png".as_slice())
            .with_project_file(".github/logo.svg", b"<svg/>".as_slice());

        let (hashes, warnings, uploads) = run_build_files(
            &storage,
            &[
                "docs/a.png",
                "docs/copy.PNG",
                "docs/as.jpg",
                ".github/logo.svg",
            ],
        )
        .await;

        assert_eq!(warnings, [] as [String; 0]);
        let svg = "d4dc56669143034f31aa309635d4113d9ad76a02b1739da22c965ed2049be9e6";
        assert_eq!(
            hashes,
            HashMap::from([
                ("docs/a.png".to_owned(), PNG_SHA256.to_owned()),
                ("docs/copy.PNG".to_owned(), PNG_SHA256.to_owned()),
                ("docs/as.jpg".to_owned(), PNG_SHA256.to_owned()),
                (".github/logo.svg".to_owned(), svg.to_owned()),
            ])
        );
        assert_eq!(
            uploads,
            [
                (
                    format!("files/{svg}.svg"),
                    b"<svg/>".to_vec(),
                    "image/svg+xml"
                ),
                (
                    format!("files/{PNG_SHA256}.png"),
                    b"png".to_vec(),
                    "image/png"
                ),
                (
                    format!("files/{PNG_SHA256}.jpg"),
                    b"png".to_vec(),
                    "image/jpeg"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn build_files_skips_missing_and_oversized_files_with_a_warning() {
        let too_big = usize::try_from(MAX_FILE_SIZE + 1).unwrap();
        let storage = MockStorage::new()
            .with_project_file("docs/a.png", b"png".as_slice())
            .with_project_file("docs/huge.png", vec![0; too_big]);

        let (hashes, warnings, uploads) = run_build_files(
            &storage,
            &["docs/a.png", "docs/missing.png", "docs/huge.png"],
        )
        .await;

        assert_eq!(
            hashes,
            HashMap::from([("docs/a.png".to_owned(), PNG_SHA256.to_owned())])
        );
        assert_eq!(uploads.len(), 1);
        assert_eq!(
            warnings,
            [
                "image `docs/huge.png` not published: missing, outside the project, \
                 not a supported image type, or over 32 MiB",
                "image `docs/missing.png` not published: missing, outside the project, \
                 not a supported image type, or over 32 MiB",
            ]
        );
    }

    /// Serves every path, so only the publisher's own allowlist can refuse one.
    struct ServesAnything;

    impl Storage for ServesAnything {
        fn scan(&self) -> Result<Vec<Document>, StorageError> {
            Ok(Vec::new())
        }

        fn read(&self, path: &str) -> Result<String, StorageError> {
            Err(StorageError::not_found(path))
        }

        fn read_file(&self, _: &str) -> Result<Vec<u8>, StorageError> {
            Ok(b"png".to_vec())
        }

        fn has_file(&self, _: &str) -> bool {
            true
        }

        fn mtime(&self, path: &str) -> Result<f64, StorageError> {
            Err(StorageError::not_found(path))
        }

        fn source_dir_in_project(&self) -> Option<String> {
            None
        }
    }

    #[tokio::test]
    async fn build_files_skips_a_file_off_the_allowlist_that_storage_serves() {
        let (hashes, warnings, uploads) =
            run_build_files(&ServesAnything, &["docs/a.png", ".env"]).await;

        assert_eq!(
            hashes,
            HashMap::from([("docs/a.png".to_owned(), PNG_SHA256.to_owned())])
        );
        assert_eq!(uploads.len(), 1);
        assert_eq!(
            warnings,
            ["image `.env` not published: missing, outside the project, \
              not a supported image type, or over 32 MiB"]
        );
    }

    #[tokio::test]
    async fn build_files_fails_on_a_storage_error() {
        let storage = MockStorage::new()
            .with_unreadable_project_file("docs/a.png", StorageErrorKind::PermissionDenied);
        let paths = BTreeSet::from(["docs/a.png".to_owned()]);

        let err = build_files(&storage, &paths, &mut Vec::new(), async |_, _, _| Ok(()))
            .await
            .unwrap_err();

        assert!(
            matches!(&err, BundlePublishError::Storage(err) if err.kind == StorageErrorKind::PermissionDenied),
            "{err:?}"
        );
    }

    #[test]
    fn virtual_root_does_not_claim_its_bundle_key() {
        let storage = MockStorage::new()
            .with_virtual_page("", "title: Home")
            .with_document("_index", "Underscore");

        check_unique_bundle_keys(&storage.scan().unwrap()).unwrap();
    }
}
