//! Runtime server-info file (`.rw/server.json`).
//!
//! `rw serve` writes this file (host, port, pid, version, and a reserved secret
//! token) so other tooling can discover a running server for the project. It is
//! written atomically with `0600` permissions and removed on graceful shutdown.

use std::fs;
use std::io::Write as _;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Contents of `.rw/server.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// OS process id of the running server.
    pub pid: u32,
    /// Bound host (e.g. `127.0.0.1`).
    pub host: String,
    /// Bound port.
    pub port: u16,
    /// Secret token (uuid-v4). Persisted for a future notify endpoint; never
    /// printed by CLI commands.
    pub token: String,
    /// Running `rw` version.
    pub version: String,
    /// Server start time, RFC 3339.
    pub started_at: String,
}

const FILE_NAME: &str = "server.json";

/// Error reading or writing the server-info file.
#[derive(Debug, thiserror::Error)]
pub enum ServerInfoError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl ServerInfo {
    /// Build the info for the current process from a bound address + version.
    #[must_use]
    pub fn new(addr: SocketAddr, version: impl Into<String>) -> Self {
        Self {
            pid: std::process::id(),
            host: addr.ip().to_string(),
            port: addr.port(),
            token: Uuid::new_v4().to_string(),
            version: version.into(),
            started_at: Utc::now().to_rfc3339(),
        }
    }

    /// Path to the server-info file inside the `.rw` state directory.
    #[must_use]
    pub fn path(rw_dir: &Path) -> PathBuf {
        rw_dir.join(FILE_NAME)
    }

    /// Atomically write the file into `rw_dir` (created if missing) with mode
    /// `0600` on Unix. Returns a guard that removes the file when dropped.
    ///
    /// # Errors
    /// Returns [`ServerInfoError`] if the directory cannot be created or the
    /// file cannot be written.
    pub fn write(&self, rw_dir: &Path) -> Result<ServerInfoGuard, ServerInfoError> {
        fs::create_dir_all(rw_dir)?;
        let final_path = Self::path(rw_dir);
        let tmp_path = rw_dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));

        let json = serde_json::to_vec_pretty(self)?;
        write_file_private(&tmp_path, &json)?;
        // Atomic on POSIX within the same directory. On failure, remove the
        // temp file so a failed write never leaves a stray `.tmp` behind.
        if let Err(err) = fs::rename(&tmp_path, &final_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(err.into());
        }

        Ok(ServerInfoGuard {
            path: final_path,
            token: self.token.clone(),
        })
    }

    /// Read and parse the file from `rw_dir`. `Ok(None)` if it does not exist.
    ///
    /// # Errors
    /// Returns [`ServerInfoError`] if the file exists but cannot be read or
    /// parsed.
    pub fn read(rw_dir: &Path) -> Result<Option<ServerInfo>, ServerInfoError> {
        let path = Self::path(rw_dir);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Probe whether `host:port` currently accepts TCP connections.
    ///
    /// A successful connect means a process is listening (likely this server).
    /// Connection refused / timeout means stale or not running. This does not
    /// verify the listener is actually an `rw` server — a future notify
    /// endpoint adds token-based identity.
    #[must_use]
    pub fn is_running(&self, timeout: Duration) -> bool {
        let Ok(mut addrs) = (self.host.as_str(), self.port).to_socket_addrs() else {
            return false;
        };
        addrs.any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
    }
}

/// Removes `.rw/server.json` when dropped — but only if the file still
/// belongs to this server.
pub struct ServerInfoGuard {
    path: PathBuf,
    token: String,
}

impl Drop for ServerInfoGuard {
    fn drop(&mut self) {
        // Only remove the file if its token still matches the one we wrote. If
        // a second server started against the same project dir and overwrote
        // it (last-writer-wins), the token differs and the file belongs to
        // that server — leave it. Any read/parse error also means "don't
        // delete" (the file is gone, or owned by something else).
        let owned = fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ServerInfo>(&bytes).ok())
            .is_some_and(|info| info.token == self.token);
        if owned {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Write `bytes` to `path`, restricting the file to owner-only (`0600`) on
/// Unix. On non-Unix platforms the file inherits default permissions, so the
/// token is not OS-protected there (best-effort, as documented).
fn write_file_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ServerInfo {
        ServerInfo {
            pid: 4321,
            host: "127.0.0.1".to_owned(),
            port: 7979,
            token: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            version: "0.1.24".to_owned(),
            started_at: "2026-06-18T15:00:00+00:00".to_owned(),
        }
    }

    #[test]
    fn serializes_camel_case_and_round_trips() {
        let info = sample();
        let json = serde_json::to_string(&info).unwrap();
        assert!(
            json.contains("\"startedAt\""),
            "expected camelCase key: {json}"
        );
        assert!(!json.contains("started_at"), "snake_case leaked: {json}");
        let back: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn new_populates_from_addr_and_process() {
        let addr = "127.0.0.1:8123".parse().unwrap();
        let info = ServerInfo::new(addr, "9.9.9");
        assert_eq!(info.host, "127.0.0.1");
        assert_eq!(info.port, 8123);
        assert_eq!(info.version, "9.9.9");
        assert_eq!(info.pid, std::process::id());
        // token is a parseable uuid
        uuid::Uuid::parse_str(&info.token).expect("token is a uuid");
        // started_at is RFC 3339
        chrono::DateTime::parse_from_rfc3339(&info.started_at).expect("rfc3339 timestamp");
    }

    #[test]
    fn path_joins_file_name() {
        let p = ServerInfo::path(std::path::Path::new("/proj/.rw"));
        assert_eq!(p, std::path::PathBuf::from("/proj/.rw/server.json"));
    }

    #[test]
    fn write_then_read_round_trips_and_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let rw_dir = tmp.path().join(".rw"); // does not exist yet
        let addr = "127.0.0.1:7979".parse().unwrap();
        let info = ServerInfo::new(addr, "0.0.1");

        let _guard = info.write(&rw_dir).unwrap();
        let read = ServerInfo::read(&rw_dir).unwrap().expect("file present");
        assert_eq!(read, info);
        // no leftover temp file
        let leftovers: Vec<_> = std::fs::read_dir(&rw_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                std::path::Path::new(n)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
            })
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn written_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let rw_dir = tmp.path().to_path_buf();
        let info = ServerInfo::new("127.0.0.1:1".parse().unwrap(), "v");
        let _guard = info.write(&rw_dir).unwrap();
        let mode = std::fs::metadata(ServerInfo::path(&rw_dir))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[test]
    fn guard_removes_file_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let rw_dir = tmp.path().to_path_buf();
        let info = ServerInfo::new("127.0.0.1:1".parse().unwrap(), "v");
        let path = ServerInfo::path(&rw_dir);
        {
            let _guard = info.write(&rw_dir).unwrap();
            assert!(path.exists());
        }
        assert!(!path.exists(), "guard should remove the file on drop");
    }

    #[test]
    fn guard_does_not_remove_file_owned_by_another_server() {
        let tmp = tempfile::tempdir().unwrap();
        let rw_dir = tmp.path().to_path_buf();
        let path = ServerInfo::path(&rw_dir);

        let info_a = ServerInfo::new("127.0.0.1:1".parse().unwrap(), "a");
        let guard_a = info_a.write(&rw_dir).unwrap();

        // A second server overwrites the file (last-writer-wins). Its token
        // differs (random uuid-v4 per `new`).
        let info_b = ServerInfo::new("127.0.0.1:2".parse().unwrap(), "b");
        let guard_b = info_b.write(&rw_dir).unwrap();
        assert_ne!(info_a.token, info_b.token);

        // Dropping A's guard must NOT delete B's file.
        drop(guard_a);
        assert!(path.exists(), "guard A wrongly deleted server B's file");
        assert_eq!(
            ServerInfo::read(&rw_dir).unwrap().unwrap().token,
            info_b.token
        );

        // Dropping B's guard (the current owner) removes it.
        drop(guard_b);
        assert!(!path.exists(), "owner guard B should remove the file");
    }

    #[test]
    fn read_missing_is_none_malformed_is_err() {
        let tmp = tempfile::tempdir().unwrap();
        let rw_dir = tmp.path().to_path_buf();
        assert!(ServerInfo::read(&rw_dir).unwrap().is_none());

        std::fs::write(ServerInfo::path(&rw_dir), b"not json").unwrap();
        assert!(ServerInfo::read(&rw_dir).is_err());
    }

    #[test]
    fn is_running_true_when_port_open() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let info = ServerInfo::new(format!("127.0.0.1:{port}").parse().unwrap(), "v");
        assert!(info.is_running(std::time::Duration::from_millis(500)));
    }

    #[test]
    fn is_running_false_when_port_closed() {
        // Bind then drop to free a port that is now almost certainly closed.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let info = ServerInfo::new(format!("127.0.0.1:{port}").parse().unwrap(), "v");
        assert!(!info.is_running(std::time::Duration::from_millis(200)));
    }
}
