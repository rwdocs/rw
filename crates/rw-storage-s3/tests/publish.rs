//! Publishes to an in-process fake S3 and reads the bundle back.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use rw_storage::{MockStorage, Storage};
use rw_storage_s3::{BundlePublisher, S3Config, S3Storage};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

#[derive(Default)]
struct Bucket {
    objects: BTreeMap<String, Vec<u8>>,
    /// `"METHOD key"`, or `"LIST prefix"`.
    requests: Vec<String>,
}

type SharedBucket = Arc<Mutex<Bucket>>;

/// Path-style requests to bucket `bucket`: PUT, GET and an unpaginated
/// `ListObjectsV2`.
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
        let contents: String = bucket
            .objects
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .map(|key| ["<Contents><Key>", key, "</Key></Contents>"].concat())
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

fn sorted_requests(bucket: &SharedBucket) -> Vec<String> {
    let mut requests = std::mem::take(&mut bucket.lock().requests);
    requests.sort();
    requests
}

#[test]
fn publish_round_trips_and_skips_files_already_in_the_bucket() {
    let rt = Arc::new(Runtime::new().expect("runtime"));
    let (config, bucket) = fake_s3(&rt);
    let storage = MockStorage::new()
        .with_document("guide", "Guide")
        .with_content("guide", "# Guide\n![a](a.png)\n")
        .with_project_file("docs/a.png", b"png".as_slice());
    let files = BTreeSet::from(["docs/a.png".to_owned()]);
    let publisher = BundlePublisher::new(config.clone());
    // SHA-256 of `b"png"`.
    let file = "entity/files/8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c.png";

    let report = rt
        .block_on(publisher.publish(&storage, &[], &files))
        .expect("publish");

    assert_eq!(report.uploaded, 3);
    assert_eq!(
        sorted_requests(&bucket),
        [
            "LIST entity/files/".to_owned(),
            format!("PUT {file}"),
            "PUT entity/manifest.json".to_owned(),
            "PUT entity/pages/guide.json".to_owned(),
        ]
    );
    let reader = S3Storage::new(config, Arc::clone(&rt));
    reader.scan().expect("scan");
    assert_eq!(
        reader.read("guide").expect("read"),
        "# Guide\n![a](a.png)\n"
    );
    assert_eq!(reader.read_file("docs/a.png").expect("read_file"), b"png");
    sorted_requests(&bucket);

    let report = rt
        .block_on(publisher.publish(&storage, &[], &files))
        .expect("republish");

    assert_eq!(report.uploaded, 2);
    assert_eq!(
        sorted_requests(&bucket),
        [
            "LIST entity/files/",
            "PUT entity/manifest.json",
            "PUT entity/pages/guide.json",
        ]
    );
}
