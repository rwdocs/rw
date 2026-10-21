//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, embeds diagram
//! files, builds bundles, and uploads them to S3 with the files pages show.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aws_sdk_s3::Client;
use aws_sdk_s3::error::{DisplayErrorContext, SdkError};
use aws_sdk_s3::operation::put_object::PutObjectError;
use md5::Md5;
use rw_diagrams::diagram_language;
use rw_parser::{Rewrite, rewrite};
use rw_plantuml::{bundle_source, read_diagram_file};
use rw_renderer::{ProjectFiles, image_file};
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
/// (broken include paths, cyclic includes) and from diagram files that could
/// not be read, across every page, followed by one per file that was not
/// published.
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
    /// Number of objects uploaded (page bundles and files the bucket lacked,
    /// and the manifest).
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
    /// includes and embedded diagram files, streams to S3 each one the bucket
    /// doesn't already hold (uploads start as soon as each bundle is ready),
    /// uploads each of `files` once per key (content and extension) unless the
    /// bucket already holds that key, and returns a [`PublishReport`] with the
    /// upload count and the warnings (see [`PublishReport`] for what is and
    /// isn't captured).
    ///
    /// `project_dir` is the project root on disk, where diagram files are
    /// read from. `files` are project paths, as [`Storage::read_file`] takes
    /// them. One that storage doesn't serve or that the reader would refuse
    /// (not an allowlisted type, or larger than [`MAX_FILE_SIZE`]) is skipped
    /// with a warning.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        project_dir: &Path,
        include_dirs: &[PathBuf],
        files: &BTreeSet<String>,
    ) -> Result<PublishReport, BundlePublishError> {
        let documents = scan_in_path_order(storage)?;
        check_unique_bundle_keys(&documents)?;
        let mut uploads = Uploads::new(&self.config).await;
        let in_bucket = s3::list_etags(&uploads.client, &self.config)
            .await
            .unwrap_or_else(|err| {
                tracing::warn!(
                    "listing the bucket failed, uploading everything: {}",
                    DisplayErrorContext(&err)
                );
                HashMap::new()
            });

        let mut warnings = Vec::new();
        let bundle_hashes = build_bundles(
            storage,
            &documents,
            project_dir,
            include_dirs,
            &mut warnings,
            async |key, json| {
                // A PUT object's ETag is its MD5, except under SSE-KMS or
                // SSE-C, where none match and every bundle uploads.
                if in_bucket.get(&key) == Some(&hex::encode(Md5::digest(&json))) {
                    return Ok(());
                }
                uploads.spawn(key, json, "application/json").await
            },
        )
        .await?;
        let file_hashes = build_files(
            storage,
            files,
            in_bucket.into_keys().collect(),
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

/// Uploads running in the background: at most [`Self::MAX_CONCURRENT`],
/// holding at most [`Self::MAX_BYTES`] unless one body alone is larger, so
/// memory stays bounded by that rather than by the size of the site.
struct Uploads {
    client: Client,
    config: Arc<S3Config>,
    /// Each yields the length of the body it uploaded.
    tasks: JoinSet<Result<usize, BundlePublishError>>,
    bytes_in_flight: usize,
    spawned: usize,
}

impl Uploads {
    const MAX_CONCURRENT: usize = 32;
    const MAX_BYTES: usize = 64 << 20;

    async fn new(config: &S3Config) -> Self {
        Self {
            client: s3::build_client(config).await,
            config: Arc::new(config.clone()),
            tasks: JoinSet::new(),
            bytes_in_flight: 0,
            spawned: 0,
        }
    }

    /// Starts uploading `body` to `key`, first waiting for room.
    async fn spawn(
        &mut self,
        key: String,
        body: Vec<u8>,
        content_type: &'static str,
    ) -> Result<(), BundlePublishError> {
        let len = body.len();
        while self.tasks.len() >= Self::MAX_CONCURRENT
            || (!self.tasks.is_empty() && self.bytes_in_flight + len > Self::MAX_BYTES)
        {
            self.bytes_in_flight -= self
                .tasks
                .join_next()
                .await
                .expect("task set is non-empty")
                .expect("upload task panicked")?;
        }
        let client = self.client.clone();
        let config = Arc::clone(&self.config);
        self.tasks.spawn(async move {
            s3::upload(&client, &config, &key, body, content_type).await?;
            Ok(len)
        });
        self.bytes_in_flight += len;
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

/// `storage`'s documents sorted by path, since [`Storage::scan`] leaves the
/// order unspecified and the manifest and warnings must not change with it.
fn scan_in_path_order(storage: &dyn Storage) -> Result<Vec<Document>, StorageError> {
    let mut documents = storage.scan()?;
    documents.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(documents)
}

/// The manifest for `documents`, as scanned from `storage`, with `bundles`
/// mapping each page to the SHA-256 of its bundle and `files` each published
/// project file to the SHA-256 of its content.
fn build_manifest(
    storage: &dyn Storage,
    documents: Vec<Document>,
    bundles: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
) -> Manifest {
    let mut mtimes = BTreeMap::new();
    for doc in &documents {
        if let Ok(mtime) = storage.mtime(&doc.path) {
            mtimes.insert(doc.path.clone(), mtime);
        }
    }

    let mut manifest = Manifest::new(documents, storage.source_dir_in_project());
    manifest.mtimes = mtimes;
    manifest.bundles = bundles;
    manifest.files = files;
    manifest
}

/// Build one page bundle per document that has content, handing each
/// `(key, bundle_json)` pair to `on_bundle` as soon as it is ready.
///
/// Returns each page's URL path mapped to the SHA-256 of its bundle JSON, and
/// adds the deduplicated `!include` and diagram file warnings to `warnings`.
///
/// Bundles are handed over one at a time instead of collected so a caller that
/// uploads them can keep only a bounded number in memory at once, whatever the
/// size of the site. Construction is sequential because include-resolution
/// warnings accumulate across pages, in document order.
async fn build_bundles(
    storage: &dyn Storage,
    documents: &[Document],
    project_dir: &Path,
    include_dirs: &[PathBuf],
    warnings: &mut Vec<String>,
    mut on_bundle: impl AsyncFnMut(String, Vec<u8>) -> Result<(), BundlePublishError>,
) -> Result<BTreeMap<String, String>, BundlePublishError> {
    let mut hashes = BTreeMap::new();
    let files = storage
        .source_dir_in_project()
        .map(|_| ProjectFiles::new(|_| true));

    for doc in documents {
        if !doc.has_content {
            continue;
        }

        let content = storage.read(&doc.path)?;
        let mut bundling = Bundling {
            project_dir,
            include_dirs,
            files: files.as_ref(),
            page: &doc.path,
            source_path: doc.source.clone(),
            warnings,
        };
        // Parsed as the site's page renderer parses it: wikilinks, and
        // `HtmlBackend`'s directives.
        let resolved_content = rewrite(&content, true, true, &mut bundling).into_owned();

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

/// What a page's bundle needs that the S3 reader can't read: each fence's
/// `!include`s inlined, and each lone image naming a diagram file replaced by
/// a fence of that file.
struct Bundling<'a> {
    project_dir: &'a Path,
    include_dirs: &'a [PathBuf],
    /// `None` when the source directory is outside the project, where the
    /// renderer places no image either.
    files: Option<&'a ProjectFiles>,
    page: &'a str,
    source_path: Option<String>,
    warnings: &'a mut Vec<String>,
}

impl Rewrite for Bundling<'_> {
    fn fence(&mut self, language: &str, body: &str) -> Option<String> {
        bundle_source(language, body, None, self.include_dirs, self.warnings)
    }

    fn lone_image(&mut self, dest: &str, _alt: &str) -> Option<(String, String)> {
        // A refusal is the render's to report, as for any image: publish
        // renders every page for the images it shows.
        let path = image_file(self.files?, self.source_path.as_deref(), dest)?.ok()?;
        let language = diagram_language(&path)?;
        let content = read_diagram_file(self.project_dir, &path, None)
            .inspect_err(|err| {
                self.warnings
                    .push(format!("/{}: image `{dest}`: {}", self.page, err.message));
            })
            .ok()?;
        let bundled = bundle_source(
            language,
            &content.text,
            Some(&content.file()),
            self.include_dirs,
            self.warnings,
        );
        Some((language.to_owned(), bundled.unwrap_or(content.text)))
    }
}

/// Reads each of `paths` from `storage` and hands `(key, bytes, content_type)`
/// to `on_file` once per key (content and extension) not in `in_bucket`, so
/// identical content under one extension uploads once, and content already
/// published not at all.
///
/// Returns each published path mapped to the SHA-256 of its content, and adds
/// a warning to `warnings` for each path it skips.
async fn build_files(
    storage: &dyn Storage,
    paths: &BTreeSet<String>,
    in_bucket: HashSet<String>,
    warnings: &mut Vec<String>,
    mut on_file: impl AsyncFnMut(String, Vec<u8>, &'static str) -> Result<(), BundlePublishError>,
) -> Result<BTreeMap<String, String>, BundlePublishError> {
    let mut hashes = BTreeMap::new();
    let mut keys = in_bucket;

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
    use rw_parser::{Event, Parser};
    use rw_storage::MockStorage;

    async fn run_build_bundles(
        storage: &dyn Storage,
        project_dir: &Path,
        include_dirs: &[PathBuf],
    ) -> (
        BTreeMap<String, String>,
        Vec<String>,
        Vec<(String, Vec<u8>)>,
    ) {
        let documents = storage.scan().unwrap();
        let mut warnings = Vec::new();
        let mut uploads = Vec::new();
        let hashes = build_bundles(
            storage,
            &documents,
            project_dir,
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

        let (_, warnings, bundles) = run_build_bundles(
            &storage,
            include_dir.path(),
            &[include_dir.path().to_path_buf()],
        )
        .await;

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
            run_build_bundles(&storage, temp.path(), &[temp.path().to_path_buf()]).await;
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
        let (_, warnings, bundles) = run_build_bundles(&storage, Path::new(""), &[]).await;
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

        let (hashes, _, bundles) = run_build_bundles(&storage, Path::new(""), &[]).await;

        let uploaded = |key: &str| {
            let (_, json) = bundles.iter().find(|(k, _)| k == key).unwrap();
            hex::encode(Sha256::digest(json))
        };
        assert_eq!(
            hashes,
            BTreeMap::from([
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
            run_build_bundles(&storage, include_dir.path(), &include_dirs)
                .await
                .0["guide"]
                .clone()
        };

        assert_ne!(hash_with("A -> B").await, hash_with("C -> D").await);
    }

    #[tokio::test]
    async fn editing_a_diagram_file_changes_the_bundle_hash() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("docs")).unwrap();
        let storage = MockStorage::new()
            .with_source_dir_in_project("docs")
            .with_document("guide", "Guide")
            .with_content("guide", "![Flow](../flow.mmd)\n");
        let hash_with = async |diagram: &str| {
            std::fs::write(project.path().join("docs/flow.mmd"), diagram).unwrap();
            run_build_bundles(&storage, project.path(), &[]).await.0["guide"].clone()
        };

        assert_ne!(
            hash_with("graph TD\nA-->B\n").await,
            hash_with("graph TD\nC-->D\n").await
        );
    }

    /// The bundle content of the page `guide` (`docs/guide/index.md`) holding
    /// `markdown`, in a project of `files`, and the publish warnings.
    async fn guide_bundle(files: &[(&str, &str)], markdown: &str) -> (String, Vec<String>) {
        let project = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = project.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let storage = MockStorage::new()
            .with_source_dir_in_project("docs")
            .with_document("guide", "Guide")
            .with_content("guide", markdown);
        let (_, warnings, bundles) = run_build_bundles(&storage, project.path(), &[]).await;
        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        (bundle.content, warnings)
    }

    const PAYMENT: (&str, &str) = (
        "docs/guide/flows/payment.puml",
        "@startuml\n!include parts.iuml\nAlice -> Bob\n@enduml\n",
    );
    const PARTS: (&str, &str) = ("docs/guide/flows/parts.iuml", "participant Bob\n");

    #[tokio::test]
    async fn a_lone_diagram_file_becomes_a_fence_with_its_includes_inlined() {
        let (content, warnings) = guide_bundle(
            &[PAYMENT, PARTS],
            "# Guide\n\n![Payment \"flow\"](flows/payment.puml)\n\nAfter.\n",
        )
        .await;

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(
            content,
            "# Guide\n\n\
             ```plantuml {alt=\"Payment \"flow\"\"}\n\
             @startuml\nparticipant Bob\nAlice -> Bob\n@enduml\n\
             ```\n\nAfter.\n"
        );
        let mut parser = Parser::new(&content, true, true);
        let code_block = loop {
            if let Event::CodeBlock(payload) = parser.next().expect("a code block") {
                break payload;
            }
        };
        assert_eq!(code_block.language.as_deref(), Some("plantuml"));
        assert_eq!(
            code_block.attrs.map,
            [("alt".to_owned(), "Payment \"flow\"".to_owned())]
        );
        assert_eq!(
            code_block.source,
            "@startuml\nparticipant Bob\nAlice -> Bob\n@enduml\n"
        );
    }

    #[tokio::test]
    async fn diagram_files_are_found_as_the_renderer_finds_images() {
        let (content, warnings) = guide_bundle(
            &[
                ("docs/guide/my flow.mmd", "graph TD\nA-->B\n"),
                ("docs/up.MMD", "graph LR\nC-->D\n"),
                ("diagrams/root.dot", "digraph { a -> b }\n"),
            ],
            "![](my%20flow.mmd?v=2#top)\n\n![Up](../up.MMD)\n\n![Root](/diagrams/root.dot)\n",
        )
        .await;

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(
            content,
            "```mermaid\ngraph TD\nA-->B\n```\n\n\
             ```mermaid {alt=\"Up\"}\ngraph LR\nC-->D\n```\n\n\
             ```graphviz {alt=\"Root\"}\ndigraph { a -> b }\n```\n"
        );
    }

    #[tokio::test]
    async fn lone_images_in_a_tab_and_a_list_item_become_fences() {
        let (content, warnings) = guide_bundle(
            &[("docs/guide/a.mmd", "graph TD\n")],
            "::::tabs\n:::tab[A]\n![Tab](a.mmd)\n:::\n::::\n\n- Item\n\n  ![Item](a.mmd)\n",
        )
        .await;

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(
            content,
            "::::tabs\n:::tab[A]\n```mermaid {alt=\"Tab\"}\ngraph TD\n```\n:::\n::::\n\n\
             - Item\n\n  ```mermaid {alt=\"Item\"}\n  graph TD\n  ```\n"
        );
    }

    #[tokio::test]
    async fn images_that_are_not_lone_diagram_files_are_left_alone() {
        let markdown = "\
Here: ![inline](a.mmd)

![Photo](photo.png)

![Remote](https://example.com/a.mmd)

- ![tight](a.mmd)

![Escaped](a%2Fb.mmd)

![Outside](../../../a.mmd)
";
        let (content, warnings) = guide_bundle(
            &[
                ("docs/guide/a.mmd", "graph TD\n"),
                ("docs/guide/photo.png", "png"),
            ],
            markdown,
        )
        .await;

        assert_eq!(content, markdown);
        // The renderer reports the refused ones, as for any image.
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[tokio::test]
    async fn an_unreadable_diagram_file_warns_and_stays_an_image() {
        let markdown = "![Missing](missing.puml)\n\n![Folder](dir.puml)\n";
        let (content, warnings) = guide_bundle(&[("docs/guide/dir.puml/x", "")], markdown).await;

        assert_eq!(content, markdown);
        assert_eq!(
            warnings,
            [
                "/guide: image `missing.puml`: diagram file 'docs/guide/missing.puml' not found",
                "/guide: image `dir.puml`: diagram file 'docs/guide/dir.puml' is not a file",
            ]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_diagram_file_linking_out_of_the_project_warns() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.puml"), "A -> B\n").unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("docs")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.puml"),
            project.path().join("docs/link.puml"),
        )
        .unwrap();
        let storage = MockStorage::new()
            .with_source_dir_in_project("docs")
            .with_document("", "Home")
            .with_content("", "![Link](link.puml)\n");

        let (_, warnings, bundles) = run_build_bundles(&storage, project.path(), &[]).await;

        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        assert_eq!(bundle.content, "![Link](link.puml)\n");
        assert_eq!(
            warnings,
            ["/: image `link.puml`: diagram file 'docs/link.puml' is outside the project"]
        );
    }

    #[tokio::test]
    async fn a_diagram_file_includes_only_plantuml_files_outside_include_dirs() {
        let (content, warnings) = guide_bundle(
            &[
                (".env", "SECRET=1\n"),
                (
                    "docs/guide/leak.puml",
                    "@startuml\n!include ../../.env\n@enduml\n",
                ),
            ],
            "![Leak](leak.puml)\n",
        )
        .await;

        assert_eq!(
            content,
            "```plantuml {alt=\"Leak\"}\n@startuml\n!include ../../.env\n@enduml\n```\n"
        );
        assert_eq!(
            warnings,
            ["Include file is not a PlantUML file: '../../.env'"]
        );
    }

    /// A file of the name wins; the reader expands one no file answers from
    /// page metadata, as `rw serve` does.
    #[tokio::test]
    async fn a_diagram_files_c4_metadata_include_inlines_only_a_real_file() {
        let flow = (
            "docs/guide/flow.puml",
            "!include systems/sys_payments_api.iuml\n!include systems/sys_billing.iuml\n",
        );
        let api = (
            "docs/guide/systems/sys_payments_api.iuml",
            "System(x, \"X\")\n",
        );

        let (content, warnings) = guide_bundle(&[flow, api], "![Flow](flow.puml)\n").await;

        assert_eq!(
            content,
            "```plantuml {alt=\"Flow\"}\nSystem(x, \"X\")\n!include systems/sys_billing.iuml\n```\n"
        );
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[tokio::test]
    async fn publishing_a_diagram_file_twice_writes_the_same_bundle() {
        let project = tempfile::tempdir().unwrap();
        for (path, content) in [PAYMENT, PARTS] {
            let path = project.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let storage = MockStorage::new()
            .with_source_dir_in_project("docs")
            .with_document("guide", "Guide")
            .with_content("guide", "![Payment](flows/payment.puml)\n");

        let first = run_build_bundles(&storage, project.path(), &[]).await;
        let second = run_build_bundles(&storage, project.path(), &[]).await;

        assert_eq!(first.0, second.0);
        assert_eq!(first.2, second.2);
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
            .publish(&storage, Path::new(""), &[], &BTreeSet::new())
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
        let bundles = BTreeMap::from([("guide".to_owned(), PNG_SHA256.to_owned())]);
        let files = BTreeMap::from([("site/docs/a.png".to_owned(), PNG_SHA256.to_owned())]);

        let manifest = build_manifest(
            &storage,
            storage.scan().unwrap(),
            bundles.clone(),
            files.clone(),
        );

        assert_eq!(
            manifest.mtimes,
            BTreeMap::from([("guide".to_owned(), 1_713_000_000.0)])
        );
        assert_eq!(manifest.source_dir.as_deref(), Some("site/docs"));
        assert_eq!(manifest.bundles, bundles);
        assert_eq!(manifest.files, files);
    }

    #[test]
    fn republishing_the_same_docs_writes_the_same_manifest_bytes() {
        let paths: Vec<String> = (0..10).map(|i| format!("page-{i}")).collect();
        let manifest_json = |scan_order: Vec<&String>| {
            let storage = scan_order.iter().fold(MockStorage::new(), |storage, path| {
                storage
                    .with_document(path.as_str(), path.as_str())
                    .with_mtime(path.as_str(), 1_713_000_000.0)
            });
            let hashes = || {
                scan_order
                    .iter()
                    .map(|&path| (path.clone(), PNG_SHA256.to_owned()))
                    .collect()
            };
            let documents = scan_in_path_order(&storage).unwrap();
            let manifest = build_manifest(&storage, documents, hashes(), hashes());
            String::from_utf8(serde_json::to_vec(&manifest).unwrap()).unwrap()
        };

        assert_eq!(
            manifest_json(paths.iter().collect()),
            manifest_json(paths.iter().rev().collect())
        );
    }

    /// SHA-256 of `b"png"`.
    const PNG_SHA256: &str = "8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c";

    async fn run_build_files(
        storage: &dyn Storage,
        paths: &[&str],
    ) -> (
        BTreeMap<String, String>,
        Vec<String>,
        Vec<(String, Vec<u8>, &'static str)>,
    ) {
        let paths = paths.iter().map(|&path| path.to_owned()).collect();
        let mut warnings = Vec::new();
        let mut uploads = Vec::new();
        let hashes = build_files(
            storage,
            &paths,
            HashSet::new(),
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
            BTreeMap::from([
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
    async fn build_files_hashes_but_does_not_upload_keys_already_in_the_bucket() {
        let storage = MockStorage::new()
            .with_project_file("docs/a.png", b"png".as_slice())
            .with_project_file("docs/b.svg", b"<svg/>".as_slice());
        let paths = BTreeSet::from(["docs/a.png".to_owned(), "docs/b.svg".to_owned()]);
        let mut uploads = Vec::new();

        let hashes = build_files(
            &storage,
            &paths,
            HashSet::from([format!("files/{PNG_SHA256}.png")]),
            &mut Vec::new(),
            async |key, _, _| {
                uploads.push(key);
                Ok(())
            },
        )
        .await
        .unwrap();

        assert_eq!(hashes["docs/a.png"], PNG_SHA256);
        assert_eq!(uploads, [format!("files/{}.svg", hashes["docs/b.svg"])]);
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
            BTreeMap::from([("docs/a.png".to_owned(), PNG_SHA256.to_owned())])
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
            BTreeMap::from([("docs/a.png".to_owned(), PNG_SHA256.to_owned())])
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

        let err = build_files(
            &storage,
            &paths,
            HashSet::new(),
            &mut Vec::new(),
            async |_, _, _| Ok(()),
        )
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
