//! Best-effort notification to a running `rw serve` after a comment mutation.
//!
//! Reads `.rw/server.json` to discover the server, then POSTs a comments event
//! to the internal events endpoint with the secret token. Every failure (no
//! file, parse error, unreachable, non-2xx) is silently ignored — the comment
//! is already persisted, so the CLI must stay fully decoupled from the server.

use std::path::Path;
use std::time::Duration;

use rw_server_info::ServerInfo;

/// Notify a running server (if any) that comments changed. Never fails.
pub(super) fn notify_server(project_dir: &Path) {
    let Ok(Some(info)) = ServerInfo::read(project_dir) else {
        return;
    };
    let url = format!("http://{}:{}/_api/_internal/events", info.host, info.port);
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
