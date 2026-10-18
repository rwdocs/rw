//! Best-effort notification to a running `rw serve` after a comment mutation.
//!
//! Reads `.rw/server.json` to discover the server, then POSTs a comments event
//! to the internal events endpoint with the secret token. Every failure (no
//! file, parse error, unreachable, non-2xx) is silently ignored — the comment
//! is already persisted, so the CLI must stay fully decoupled from the server.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use rw_server_info::ServerInfo;

/// Notify a running server (if any) that comments changed. Never fails.
pub(super) fn notify_server(data_dir: &Path) {
    let Ok(Some(info)) = ServerInfo::read(data_dir) else {
        return;
    };
    let Ok(ip) = info.host.parse::<IpAddr>() else {
        return;
    };
    // A wildcard bind is not a connectable destination everywhere (Windows
    // refuses it); mirrors `serve::browser_url`.
    let ip = match ip {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    // `SocketAddr`'s Display brackets IPv6, which a URL authority requires.
    let url = format!(
        "http://{}/_api/_internal/events",
        SocketAddr::new(ip, info.port)
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(1)))
        .build()
        .into();
    // Ignore the result entirely: any error means no reachable server.
    let _ = agent
        .post(&url)
        .header("X-RW-Token", &info.token)
        .header("Content-Type", "application/json")
        .send(br#"{"type":"comments"}"#.as_slice());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    const TOKEN: &str = "notify-test-secret";
    const WATCHDOG: Duration = Duration::from_secs(5);
    const NO_CONTENT: &[u8] =
        b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    type ServerFile = (
        tempfile::TempDir,
        rw_server_info::ServerInfoGuard,
        TcpListener,
    );

    fn server_file() -> ServerFile {
        server_file_on("127.0.0.1:0", None).unwrap()
    }

    /// `None` when `bind` is unavailable, e.g. a host without IPv6 loopback.
    /// `advertised_host` replaces the bound IP in `server.json`.
    fn server_file_on(bind: &str, advertised_host: Option<&str>) -> Option<ServerFile> {
        let tmp = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(bind).ok()?;
        listener.set_nonblocking(true).unwrap();
        let mut info = ServerInfo::new(listener.local_addr().unwrap(), "notify-test");
        info.token = TOKEN.to_owned();
        if let Some(host) = advertised_host {
            host.clone_into(&mut info.host);
        }
        let guard = info.write(tmp.path()).unwrap();
        Some((tmp, guard, listener))
    }

    // Keep the peer alive in the caller so a disconnect cannot masquerade as
    // the client's timeout.
    fn read_request(listener: &TcpListener) -> (TcpStream, String, serde_json::Value) {
        let deadline = Instant::now() + WATCHDOG;
        let mut peer = loop {
            match listener.accept() {
                Ok((peer, _)) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "no notification connection");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept notification: {error}"),
            }
        };
        peer.set_nonblocking(false).unwrap();
        let mut bytes = Vec::new();
        let mut header_end = None;
        let mut content_length = 0;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("request read deadline");
            peer.set_read_timeout(Some(remaining)).unwrap();
            let mut buffer = [0; 1024];
            let count = peer.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "client closed before sending a complete request");
            bytes.extend_from_slice(&buffer[..count]);
            assert!(bytes.len() <= 16 * 1024, "unexpectedly large notification");
            if header_end.is_none()
                && let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n")
            {
                let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                content_length = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .expect("slice body has Content-Length")
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                assert!(content_length <= 1024, "unexpectedly large body");
                header_end = Some(end + 4);
            }
            if let Some(end) = header_end
                && bytes.len() >= end + content_length
            {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let body = serde_json::from_slice(&bytes[end..end + content_length]).unwrap();
                return (peer, headers, body);
            }
        }
    }

    fn assert_comments_request(headers: &str, body: &serde_json::Value) {
        assert_eq!(
            headers.lines().next(),
            Some("POST /_api/_internal/events HTTP/1.1")
        );
        let header = |wanted: &str| {
            headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .filter(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .map(|(_, value)| value.trim())
                .collect::<Vec<_>>()
        };
        assert_eq!(header("X-RW-Token"), [TOKEN]);
        assert_eq!(header("Content-Type"), ["application/json"]);
        assert_eq!(*body, serde_json::json!({"type": "comments"}));
    }

    fn record_notification(response: &[u8]) -> (String, serde_json::Value) {
        record_notification_to(server_file(), response)
    }

    fn record_notification_to(
        (tmp, _guard, listener): ServerFile,
        response: &[u8],
    ) -> (String, serde_json::Value) {
        thread::scope(|scope| {
            let server = scope.spawn(|| {
                let (mut peer, headers, body) = read_request(&listener);
                peer.set_write_timeout(Some(WATCHDOG)).unwrap();
                peer.write_all(response).unwrap();
                (headers, body)
            });
            notify_server(tmp.path());
            server.join().unwrap()
        })
    }

    #[test]
    fn notify_posts_authenticated_comments_event() {
        let (headers, body) = record_notification(NO_CONTENT);
        assert_comments_request(&headers, &body);
    }

    #[test]
    fn notify_reaches_ipv6_server() {
        let Some(server) = server_file_on("[::1]:0", None) else {
            return;
        };
        let (headers, body) = record_notification_to(server, NO_CONTENT);
        assert_comments_request(&headers, &body);
    }

    #[test]
    fn notify_reaches_server_bound_to_unspecified_ipv6() {
        let Some(server) = server_file_on("[::1]:0", Some("::")) else {
            return;
        };
        let (headers, body) = record_notification_to(server, NO_CONTENT);
        assert_comments_request(&headers, &body);
    }

    #[test]
    fn notify_ignores_service_unavailable_after_sending_request() {
        let (headers, body) = record_notification(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert_comments_request(&headers, &body);
    }

    #[test]
    fn notify_times_out_while_server_keeps_connection_open() {
        let (tmp, _guard, listener) = server_file();
        let (release, released) = mpsc::channel();
        let ((headers, body, caller_released_peer), elapsed) = thread::scope(|scope| {
            let server = scope.spawn(move || {
                let (peer, headers, body) = read_request(&listener);
                // Only the caller may release the peer for this test to pass;
                // the watchdog keeps a missing client timeout from hanging it.
                let caller_released_peer = released.recv_timeout(WATCHDOG).is_ok();
                drop(peer);
                (headers, body, caller_released_peer)
            });
            let start = Instant::now();
            notify_server(tmp.path());
            let elapsed = start.elapsed();
            let _ = release.send(());
            (server.join().unwrap(), elapsed)
        });
        assert_comments_request(&headers, &body);
        assert!(
            caller_released_peer,
            "server watchdog, not client timeout, closed the peer ({elapsed:?})"
        );
        assert!(
            elapsed >= Duration::from_millis(750),
            "returned before the one-second budget: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "exceeded the one-second budget: {elapsed:?}"
        );
    }

    #[test]
    fn notify_is_a_noop_when_no_server_file() {
        let tmp = tempfile::tempdir().unwrap();
        // No server.json in this dir → returns without error or panic.
        notify_server(tmp.path());
    }

    #[test]
    fn notify_is_a_noop_when_server_file_is_malformed() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("server.json"), b"not valid json").unwrap();
        // ServerInfo::read returns Err -> the let-else returns early, no panic, no network.
        notify_server(tmp.path());
    }
}
