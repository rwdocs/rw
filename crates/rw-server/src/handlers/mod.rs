//! HTTP request handlers.

use std::panic;
use std::sync::Arc;

use rw_site::Site;
use tokio::task::{self, JoinError};

pub(crate) mod comments;
pub(crate) mod config;
pub(crate) mod internal;
pub(crate) mod navigation;
pub(crate) mod pages;

/// Runs `f` on the blocking pool: a site call may rescan storage or wait on
/// Kroki, which would stall every request sharing its async worker.
///
/// A panic in `f` resumes here, so it reaches the request as if `f` had run
/// inline.
pub(crate) async fn with_site<T, E>(
    site: &Arc<Site>,
    f: impl FnOnce(&Site) -> Result<T, E> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: From<JoinError> + Send + 'static,
{
    let site = Arc::clone(site);
    match task::spawn_blocking(move || f(&site)).await {
        Ok(result) => result,
        Err(err) if err.is_panic() => panic::resume_unwind(err.into_panic()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use axum::http::{Method, StatusCode};
    use rw_storage::MockStorage;
    use serde_json::{Value, json};
    use tokio::sync::Notify;
    use tokio::time;

    use crate::testing::TestServer;

    /// Sends a request whose site call gets stuck in the first storage scan,
    /// then checks `/_api/config` is served before the scan is released. The
    /// test runtime has one thread, so a site call made on it would starve
    /// every other request until the scan gives up.
    async fn assert_other_requests_served_while_stuck(
        method: Method,
        path: &str,
        body: Option<Value>,
        expected: StatusCode,
    ) {
        let storage = MockStorage::new()
            .with_file("release-1.2", "Release", "# Release\n\nShipped notes.")
            .with_mtime("release-1.2", 1000.0);
        let entered = Arc::new(Notify::new());
        let release = Arc::new(AtomicBool::new(false));
        let released_in_time = Arc::new(AtomicBool::new(false));
        let scanned = AtomicBool::new(false);
        storage.set_scan_hook(Some(Box::new({
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let released_in_time = Arc::clone(&released_in_time);
            move || {
                if scanned.swap(true, Ordering::SeqCst) {
                    return;
                }
                entered.notify_one();
                let deadline = Instant::now() + Duration::from_secs(2);
                while !release.load(Ordering::SeqCst) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(5));
                }
                released_in_time.store(release.load(Ordering::SeqCst), Ordering::SeqCst);
            }
        })));
        let server = Arc::new(TestServer::with_storage(storage).await);

        let stuck = tokio::spawn({
            let server = Arc::clone(&server);
            let path = path.to_owned();
            async move { server.request(method, &path, body, &[]).await }
        });
        time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("request never reached the storage scan");
        let config = server.get("/_api/config").await;
        release.store(true, Ordering::SeqCst);
        let stuck = stuck.await.unwrap();

        assert_eq!(config.status, StatusCode::OK);
        assert!(
            released_in_time.load(Ordering::SeqCst),
            "{path}: /_api/config waited for the stuck site call"
        );
        assert_eq!(stuck.status, expected, "{path}: {}", stuck.text());
    }

    #[tokio::test]
    async fn page_render_does_not_block_other_requests() {
        assert_other_requests_served_while_stuck(
            Method::GET,
            "/_api/pages/release-1.2",
            None,
            StatusCode::OK,
        )
        .await;
        assert_other_requests_served_while_stuck(
            Method::GET,
            "/_api/pages/",
            None,
            StatusCode::NOT_FOUND,
        )
        .await;
    }

    #[tokio::test]
    async fn navigation_does_not_block_other_requests() {
        assert_other_requests_served_while_stuck(
            Method::GET,
            "/_api/navigation",
            None,
            StatusCode::OK,
        )
        .await;
    }

    #[tokio::test]
    async fn quote_resolution_does_not_block_other_requests() {
        let body = json!({
            "documentId": "section:default/root#release-1.2",
            "body": "Typo?",
            "quote": "Shipped",
        });
        assert_other_requests_served_while_stuck(
            Method::POST,
            "/_api/comments",
            Some(body),
            StatusCode::CREATED,
        )
        .await;
    }

    #[tokio::test]
    async fn dotted_spa_route_does_not_block_other_requests() {
        let expected = if rw_assets::get("index.html").is_some() {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        assert_other_requests_served_while_stuck(Method::GET, "/release-1.2", None, expected).await;
    }

    #[tokio::test]
    #[should_panic(expected = "MockStorage::scan: induced panic")]
    async fn site_panic_reaches_the_request() {
        let storage = MockStorage::new();
        storage.set_scan_panic(true);
        let server = TestServer::with_storage(storage).await;

        server.get("/_api/pages/guide").await;
    }
}
