//! Publishes to an in-process fake S3 and reads the bundle back.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use md5::{Digest, Md5};
use parking_lot::Mutex;
use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage::{MockStorage, PageText, Storage};
use rw_storage_fs::FsStorage;
use rw_storage_s3::{BundlePublisher, S3Config, S3Storage};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

#[derive(Default)]
struct Bucket {
    objects: BTreeMap<String, Vec<u8>>,
    /// `"METHOD key"`, or `"LIST prefix"`.
    requests: Vec<String>,
    /// Answers every LIST with 403, as without `s3:ListBucket`.
    deny_list: bool,
}

type SharedBucket = Arc<Mutex<Bucket>>;

/// Path-style requests to bucket `bucket`: PUT, GET and an unpaginated
/// `ListObjectsV2`, whose `ETag`s are MD5s as S3 gives them.
async fn handle(
    State(bucket): State<SharedBucket>,
    method: Method,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let key = uri
        .path()
        .strip_prefix("/bucket/")
        .unwrap_or_default()
        .to_owned();
    let mut bucket = bucket.lock();
    if method == Method::GET && query.get("list-type").is_some_and(|t| t == "2") {
        let prefix = query.get("prefix").cloned().unwrap_or_default();
        bucket.requests.push(format!("LIST {prefix}"));
        if bucket.deny_list {
            return (
                StatusCode::FORBIDDEN,
                "<Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>",
            )
                .into_response();
        }
        let contents: String = bucket
            .objects
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(key, object)| {
                let etag = hex::encode(Md5::digest(object));
                [
                    "<Contents><Key>",
                    key,
                    "</Key><ETag>&quot;",
                    &etag,
                    "&quot;</ETag></Contents>",
                ]
                .concat()
            })
            .collect();
        return format!(
            "<ListBucketResult><Name>bucket</Name><Prefix>{prefix}</Prefix>\
             <IsTruncated>false</IsTruncated>{contents}</ListBucketResult>"
        )
        .into_response();
    }
    bucket.requests.push(format!("{method} {key}"));
    match method {
        Method::PUT => {
            bucket.objects.insert(key, body.to_vec());
            StatusCode::OK.into_response()
        }
        Method::GET => match bucket.objects.get(&key) {
            Some(object) => object.clone().into_response(),
            None => (
                StatusCode::NOT_FOUND,
                "<Error><Code>NoSuchKey</Code></Error>",
            )
                .into_response(),
        },
        _ => StatusCode::NOT_IMPLEMENTED.into_response(),
    }
}

/// Starts the fake on `rt` and returns a config pointing at it.
fn fake_s3(rt: &Runtime) -> (S3Config, SharedBucket) {
    let bucket = SharedBucket::default();
    let app = Router::new()
        .fallback(handle)
        .layer(DefaultBodyLimit::disable())
        .with_state(Arc::clone(&bucket));
    let listener = rt.block_on(TcpListener::bind("127.0.0.1:0")).expect("bind");
    let addr = listener.local_addr().expect("local addr");
    rt.spawn(async move { axum::serve(listener, app).await });
    let config = S3Config {
        bucket: "bucket".to_owned(),
        prefix: "entity".to_owned(),
        region: "us-east-1".to_owned(),
        endpoint: Some(format!("http://{addr}")),
        bucket_root_path: None,
        access_key_id: Some("id".to_owned()),
        secret_access_key: Some("secret".to_owned()),
    };
    (config, bucket)
}

/// The requests since the last call, sorted, after checking that the
/// manifest went up last.
fn publish_requests(bucket: &SharedBucket) -> Vec<String> {
    let mut requests = std::mem::take(&mut bucket.lock().requests);
    assert_eq!(
        requests.last().map(String::as_str),
        Some("PUT entity/manifest.json"),
        "{requests:?}"
    );
    requests.sort();
    requests
}

/// SHA-256 of `b"png"`.
const FILE: &str =
    "entity/files/8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c.png";

/// A page showing `docs/a.png`, and the files to publish.
fn site() -> (MockStorage, BTreeSet<String>) {
    let storage = MockStorage::new()
        .with_document("guide", "Guide")
        .with_content("guide", "# Guide\n![a](a.png)\n")
        .with_project_file("docs/a.png", b"png".as_slice());
    (storage, BTreeSet::from(["docs/a.png".to_owned()]))
}

#[test]
fn publish_round_trips_and_skips_files_already_in_the_bucket() {
    let rt = Arc::new(Runtime::new().expect("runtime"));
    let (config, bucket) = fake_s3(&rt);
    let (storage, files) = site();
    let publisher = BundlePublisher::new(config.clone());

    let report = rt
        .block_on(publisher.publish(&storage, Path::new(""), &[], &files, &BTreeMap::new()))
        .expect("publish");

    assert_eq!(report.uploaded, 3);
    assert_eq!(
        publish_requests(&bucket),
        [
            "LIST entity/".to_owned(),
            format!("PUT {FILE}"),
            "PUT entity/manifest.json".to_owned(),
            "PUT entity/pages/guide.json".to_owned(),
        ]
    );
    let reader = S3Storage::new(config, Arc::clone(&rt));
    reader.scan().expect("scan");
    assert_eq!(
        reader.read("guide").expect("read"),
        PageText {
            text: "# Guide\n![a](a.png)\n".to_owned(),
            includes: Some(BTreeMap::new()),
        }
    );
    assert_eq!(reader.read_file("docs/a.png").expect("read_file"), b"png");
    bucket.lock().requests.clear();

    let report = rt
        .block_on(publisher.publish(&storage, Path::new(""), &[], &files, &BTreeMap::new()))
        .expect("republish");

    assert_eq!(report.uploaded, 1);
    assert_eq!(
        publish_requests(&bucket),
        ["LIST entity/", "PUT entity/manifest.json"]
    );
}

#[test]
fn a_published_page_renders_its_includes_from_s3() {
    let project = tempfile::tempdir().expect("tempdir");
    let root = project.path();
    for (name, contents) in [
        ("docs/guide.md", "# Guide\n\n::include[_partials/note.md]\n"),
        (
            "docs/_partials/note.md",
            "Note text.\n\n![Logo](logo.png)\n\n![Flow](flow.mmd)\n\n::include[/README.md]\n",
        ),
        ("docs/_partials/logo.png", "png"),
        ("docs/_partials/flow.mmd", "graph TD\nA-->B\n"),
        ("README.md", "Readme text.\n"),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    }
    let local = Arc::new(FsStorage::new(root.to_path_buf(), root.join("docs")));
    let (mut files, mut includes) = (BTreeSet::new(), BTreeMap::new());
    let site = Site::new(
        Arc::clone(&local) as Arc<dyn Storage>,
        Arc::new(NullCache),
        PageRendererConfig::default(),
    );
    for page in site.list_pages().expect("pages") {
        let render = site.render(&page.path).expect("render");
        files.extend(render.files);
        includes.insert(page.path, render.includes);
    }
    let rt = Arc::new(Runtime::new().expect("runtime"));
    let (config, _) = fake_s3(&rt);
    let report = rt
        .block_on(BundlePublisher::new(config.clone()).publish(
            &*local,
            root,
            &[],
            &files,
            &includes,
        ))
        .expect("publish");
    assert_eq!(report.warnings, [] as [String; 0]);

    let s3 = Site::new(
        Arc::new(S3Storage::new(config, rt)),
        Arc::new(NullCache),
        PageRendererConfig::default(),
    );
    let page = s3.render("guide").expect("render from S3");

    assert_eq!(page.warnings, [] as [String; 0]);
    for expected in [
        "Note text.",
        r#"src="/_api/files/docs/_partials/logo.png""#,
        "graph TD",
        "Readme text.",
    ] {
        assert!(page.html.contains(expected), "{expected}: {}", page.html);
    }
}

#[test]
fn republish_uploads_only_bundles_unlike_the_bucket_copy() {
    let rt = Runtime::new().expect("runtime");
    let (config, bucket) = fake_s3(&rt);
    let publisher = BundlePublisher::new(config);
    let storage = |guide: &str| {
        MockStorage::new()
            .with_document("guide", "Guide")
            .with_content("guide", guide)
            .with_document("other", "Other")
            .with_content("other", "# Other\n")
    };
    let publish = |storage: &MockStorage| {
        let report = rt
            .block_on(publisher.publish(
                storage,
                Path::new(""),
                &[],
                &BTreeSet::new(),
                &BTreeMap::new(),
            ))
            .expect("publish");
        (report.uploaded, publish_requests(&bucket))
    };
    publish(&storage("# Guide\n"));

    assert_eq!(
        publish(&storage("# Guide, edited\n")),
        (
            2,
            vec![
                "LIST entity/".to_owned(),
                "PUT entity/manifest.json".to_owned(),
                "PUT entity/pages/guide.json".to_owned(),
            ]
        )
    );

    // As an interrupted publish leaves it: the manifest predates the bundle.
    bucket
        .lock()
        .objects
        .insert("entity/pages/guide.json".to_owned(), b"{}".to_vec());
    assert_eq!(
        publish(&storage("# Guide, edited\n")),
        (
            2,
            vec![
                "LIST entity/".to_owned(),
                "PUT entity/manifest.json".to_owned(),
                "PUT entity/pages/guide.json".to_owned(),
            ]
        )
    );
}

#[test]
fn publish_uploads_every_file_when_listing_is_denied() {
    let rt = Runtime::new().expect("runtime");
    let (config, bucket) = fake_s3(&rt);
    {
        let mut bucket = bucket.lock();
        bucket.deny_list = true;
        bucket.objects.insert(FILE.to_owned(), b"png".to_vec());
    }
    let (storage, files) = site();

    let report = rt
        .block_on(BundlePublisher::new(config).publish(
            &storage,
            Path::new(""),
            &[],
            &files,
            &BTreeMap::new(),
        ))
        .expect("publish");

    assert_eq!(report.uploaded, 3);
    assert_eq!(
        publish_requests(&bucket),
        [
            "LIST entity/".to_owned(),
            format!("PUT {FILE}"),
            "PUT entity/manifest.json".to_owned(),
            "PUT entity/pages/guide.json".to_owned(),
        ]
    );
}
