//! HTTP server for RW documentation engine.
//!
//! This crate provides a native Rust HTTP server using axum, serving:
//! - API endpoints for page rendering and navigation
//! - Static files for the frontend SPA
//! - WebSocket endpoint for live reload during development
//!
//! # Static Asset Modes
//!
//! Static assets are served via `rw-assets`, which supports both embedded
//! and filesystem modes. See `rw-assets` crate for details.
//!
//! # Quick Start
//!
//! ```no_run
//! use std::path::PathBuf;
//! use rw_server::{ServerConfig, bind_listener, run_server};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig {
//!         host: "127.0.0.1".to_owned(),
//!         port: 7979,
//!         project_dir: PathBuf::from("."),
//!         source_dir: PathBuf::from("docs"),
//!         cache_dir: Some(PathBuf::from(".rw/cache")),
//!         kroki_url: Some("https://kroki.io".to_owned()),
//!         include_dirs: vec![PathBuf::from(".")],
//!         live_reload_enabled: true,
//!         verbose: false,
//!         version: "1.0.0".to_owned(),
//!         comments_db: PathBuf::from(".rw/comments/sqlite.db"),
//!         ..Default::default()
//!     };
//!
//!     // Bind the port (falling back to the next free one if 7979 is busy),
//!     // then serve on the bound listener.
//!     let listener = bind_listener(&config.host, config.port, true).await.unwrap();
//!     run_server(config, listener).await.unwrap();
//! }
//! ```
//!
//! # Architecture
//!
//! ```text
//! Browser ──HTTP──► Rust axum server (rw-server)
//!                        │
//!                        ├─► API routes (Rust handlers)
//!                        │       │
//!                        │       └─► Direct call ──► Site (render + structure)
//!                        │
//!                        ├─► WebSocket (Rust LiveReloadManager)
//!                        │       │
//!                        │       └─► notify (direct Rust crate)
//!                        │
//!                        └─► Static files (rw-assets)
//! ```

mod app;
mod error;
mod handlers;
mod live_reload;
mod middleware;
mod state;
mod static_files;
#[cfg(test)]
mod testing;

pub use error::ServerError;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use rw_comments::SqliteCommentStore;
use rw_server_info::ServerInfo;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;
use state::AppState;
use tokio::sync::broadcast;

/// Server configuration.
#[derive(Debug)]
pub struct ServerConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
    /// Project root — the directory containing `rw.toml`. The `README.md`
    /// homepage fallback is resolved from it.
    pub project_dir: PathBuf,
    /// Documentation source directory.
    pub source_dir: PathBuf,
    /// Cache directory (`None` disables caching).
    pub cache_dir: Option<PathBuf>,
    /// Kroki URL for diagrams (`None` disables diagrams).
    pub kroki_url: Option<String>,
    /// `PlantUML` include directories.
    pub include_dirs: Vec<PathBuf>,
    /// Enable live reload.
    pub live_reload_enabled: bool,
    /// Enable verbose output.
    pub verbose: bool,
    /// Application version (for cache invalidation).
    pub version: String,
    /// Metadata file name (default: "meta.yaml").
    pub meta_filename: String,
    /// Path to `SQLite` database for comments.
    pub comments_db: PathBuf,
    /// Enable embedded preview mode (serves Backstage-like shell at /).
    pub embedded_preview: bool,
    /// The `.rw` data directory (holds `server.json`, `comments/`, cache).
    pub data_dir: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        // Derive `comments_db` from `data_dir` rather than repeating `.rw`
        // here — the name is defined once in `rw_config::DATA_DIR_NAME`.
        let data_dir = PathBuf::from(rw_config::DATA_DIR_NAME);
        Self {
            host: "127.0.0.1".to_owned(),
            port: 7979,
            project_dir: PathBuf::from("."),
            source_dir: PathBuf::from("docs"),
            cache_dir: None,
            kroki_url: None,
            include_dirs: Vec::new(),
            live_reload_enabled: false,
            verbose: false,
            version: String::new(),
            meta_filename: "meta.yaml".to_owned(),
            comments_db: SqliteCommentStore::default_path(&data_dir),
            embedded_preview: false,
            data_dir,
        }
    }
}

/// Number of sequential ports tried when the requested port is busy and port
/// fallback is enabled: the default port and the next 19 above it.
const PORT_FALLBACK_RANGE: u16 = 20;

/// Bind a TCP listener on `host:port`, optionally falling back to the next free
/// port.
///
/// When `allow_fallback` is `true` and `port` is already in use, the next
/// sequential ports are tried (up to [`PORT_FALLBACK_RANGE`] total) and the
/// first free one is used — this is how `rw serve` copes with a busy *default*
/// port. When `allow_fallback` is `false`, a busy port is a hard error
/// ([`ServerError::PortInUse`]): the caller asked for a specific port and must
/// get it or nothing.
///
/// `host` must be an IP literal (e.g. `127.0.0.1` or `0.0.0.0`), matching the
/// address form accepted elsewhere in the server.
///
/// # Errors
///
/// Returns [`ServerError::InvalidAddress`] if `host:port` is not a valid socket
/// address, [`ServerError::PortInUse`] if the sole requested port is taken (no
/// fallback), [`ServerError::NoFreePort`] if every port in the fallback range is
/// taken, or [`ServerError::Io`] for any other bind failure.
pub async fn bind_listener(
    host: &str,
    port: u16,
    allow_fallback: bool,
) -> Result<tokio::net::TcpListener, ServerError> {
    let attempts = if allow_fallback {
        PORT_FALLBACK_RANGE
    } else {
        1
    };

    let mut last_candidate = port;
    for offset in 0..attempts {
        // Stop early if incrementing would overflow past the last port. Only
        // reachable with a near-max explicit-but-fallback port, which we never
        // produce today, but keep it total rather than panicking.
        let Some(candidate) = port.checked_add(offset) else {
            break;
        };
        last_candidate = candidate;

        let addr = SocketAddr::from_str(&format!("{host}:{candidate}"))?;
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!(address = %addr, "Listening");
                return Ok(listener);
            }
            // Port taken — fall through to try the next one if fallback is
            // allowed, otherwise report it as a hard error below.
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(err) => return Err(err.into()),
        }
    }

    if allow_fallback {
        Err(ServerError::NoFreePort {
            start: port,
            end: last_candidate,
        })
    } else {
        Err(ServerError::PortInUse(port))
    }
}

/// Run the server on an already-bound listener.
///
/// The caller binds the socket (see [`bind_listener`]) so it can report the
/// actually-bound address — which may differ from the requested port when port
/// fallback kicked in — before the server takes over the terminal.
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `listener` - A bound TCP listener to serve on
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn run_server(
    config: ServerConfig,
    listener: tokio::net::TcpListener,
) -> Result<(), ServerError> {
    // Create shared storage backend
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(FsStorage::with_meta_filename(
        config.project_dir.clone(),
        config.source_dir.clone(),
        &config.meta_filename,
    ));

    // Construct cache
    let cache: Arc<dyn rw_cache::Cache> = match &config.cache_dir {
        Some(dir) => Arc::new(rw_cache::FileCache::new(dir.clone(), &config.version)),
        None => Arc::new(rw_cache::NullCache),
    };

    // Create unified Site with storage and configuration
    let site_config = PageRendererConfig {
        extract_title: true,
        kroki_url: config.kroki_url.clone(),
        include_dirs: config.include_dirs.clone(),
    };
    let site = Arc::new(Site::new(Arc::clone(&storage), cache, site_config));

    // Create live reload manager if enabled
    let live_reload = if config.live_reload_enabled {
        let (tx, _rx) = broadcast::channel::<live_reload::ReloadEvent>(100);
        let mut manager = live_reload::LiveReloadManager::new(Arc::clone(&site), tx);
        manager.start(storage.as_ref())?;
        Some(manager)
    } else {
        None
    };

    let comment_store = Arc::new(SqliteCommentStore::open(&config.comments_db).await?);

    // The listener is already bound by the caller (via `bind_listener`), so the
    // server-info file and notify token below reflect the actually-bound address.
    // Build the server-info struct once from the bound address. Its token is
    // both written to `.rw/server.json` and held in `AppState` so the internal
    // notify endpoint can authenticate the CLI. If the bound address can't be
    // read, there is no token and the endpoint stays disabled (404).
    let server_info = match listener.local_addr() {
        Ok(bound) => Some(ServerInfo::new(bound, config.version.clone())),
        Err(err) => {
            tracing::warn!(error = %err, "failed to read bound address for server info file");
            None
        }
    };
    let notify_token = server_info.as_ref().map(|info| info.token.clone());

    let state = Arc::new(AppState {
        site,
        live_reload,
        verbose: config.verbose,
        comment_store,
        notify_token,
        embedded_preview: config.embedded_preview,
    });

    // Create router
    let app = app::create_router(state);

    // Write the runtime server-info file (non-fatal: an unwritable .rw should
    // not stop serving). The guard removes the file when `run_server` returns.
    let _info_guard = server_info.and_then(|info| match info.write(&config.data_dir) {
        Ok(guard) => Some(guard),
        Err(err) => {
            tracing::warn!(error = %err, "failed to write server info file");
            None
        }
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Wait for a shutdown signal: Ctrl-C (SIGINT) on all platforms, plus SIGTERM
/// on Unix (the default `kill` / `docker stop` / systemd-stop signal). Handling
/// SIGTERM lets graceful shutdown run so the server-info file guard cleans up
/// `.rw/server.json` on a normal stop, not just on Ctrl-C.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("Shutdown signal received, stopping server...");
}

/// Create server configuration from RW config.
///
/// # Arguments
///
/// * `config` - RW configuration
/// * `version` - Application version
/// * `verbose` - Enable verbose output
#[must_use]
pub fn server_config_from_rw_config(
    config: &rw_config::Config,
    version: String,
    verbose: bool,
) -> ServerConfig {
    #[allow(clippy::needless_update)]
    ServerConfig {
        host: config.server.host.clone(),
        port: config.server.port,
        project_dir: config.project_dir.clone(),
        source_dir: config.docs_resolved.source_dir.clone(),
        cache_dir: if config.docs_resolved.cache_enabled {
            Some(config.docs_resolved.cache_dir())
        } else {
            None
        },
        kroki_url: config.diagrams_resolved.kroki_url.clone(),
        include_dirs: config.diagrams_resolved.include_dirs.clone(),
        live_reload_enabled: config.live_reload.enabled,
        verbose,
        version,
        meta_filename: config.metadata.name.clone(),
        comments_db: SqliteCommentStore::default_path(&config.docs_resolved.data_dir),
        data_dir: config.docs_resolved.data_dir.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_config_default_data_dir_is_dot_rw() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.data_dir, std::path::PathBuf::from(".rw"));
    }

    #[tokio::test]
    async fn bind_listener_uses_requested_free_port() {
        // Port 0 asks the OS for any free port — always succeeds.
        let listener = bind_listener("127.0.0.1", 0, false).await.unwrap();
        assert!(listener.local_addr().unwrap().port() > 0);
    }

    #[tokio::test]
    async fn bind_listener_explicit_busy_port_errors() {
        // Occupy a port, then request it explicitly (no fallback).
        let occupied = bind_listener("127.0.0.1", 0, false).await.unwrap();
        let port = occupied.local_addr().unwrap().port();

        let err = bind_listener("127.0.0.1", port, false).await.unwrap_err();
        match err {
            ServerError::PortInUse(p) => assert_eq!(p, port),
            other => panic!("expected PortInUse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bind_listener_falls_back_to_next_free_port() {
        // Occupy a port, then request it with fallback allowed: it should land
        // on a different, free port within the fallback range.
        let occupied = bind_listener("127.0.0.1", 0, false).await.unwrap();
        let port = occupied.local_addr().unwrap().port();

        let listener = bind_listener("127.0.0.1", port, true).await.unwrap();
        let bound = listener.local_addr().unwrap().port();
        assert_ne!(bound, port);
        assert!((port..port.saturating_add(PORT_FALLBACK_RANGE)).contains(&bound));
    }
}
