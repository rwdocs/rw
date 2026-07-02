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
//! use rw_server::{ServerConfig, run_server};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig {
//!         host: "127.0.0.1".to_owned(),
//!         port: 7979,
//!         source_dir: PathBuf::from("docs"),
//!         cache_dir: Some(PathBuf::from(".rw/cache")),
//!         kroki_url: Some("https://kroki.io".to_owned()),
//!         include_dirs: vec![PathBuf::from(".")],
//!         dpi: 192,
//!         live_reload_enabled: true,
//!         verbose: false,
//!         version: "1.0.0".to_owned(),
//!         comments_db: PathBuf::from(".rw/comments/sqlite.db"),
//!         ..Default::default()
//!     };
//!
//!     run_server(config).await.unwrap();
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
    /// Documentation source directory.
    pub source_dir: PathBuf,
    /// Cache directory (`None` disables caching).
    pub cache_dir: Option<PathBuf>,
    /// Kroki URL for diagrams (`None` disables diagrams).
    pub kroki_url: Option<String>,
    /// `PlantUML` include directories.
    pub include_dirs: Vec<PathBuf>,
    /// Diagram DPI.
    pub dpi: u32,
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
    /// The `.rw` state directory (holds `server.json`, `comments/`, cache).
    pub project_dir: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        // The project state dir holds the comments DB (and cache, server-info
        // file), so derive `comments_db` from `project_dir` rather than
        // repeating the directory name. The name itself is defined once in
        // `rw_config::PROJECT_DIR_NAME`.
        let project_dir = PathBuf::from(rw_config::PROJECT_DIR_NAME);
        Self {
            host: "127.0.0.1".to_owned(),
            port: 7979,
            source_dir: PathBuf::from("docs"),
            cache_dir: None,
            kroki_url: None,
            include_dirs: Vec::new(),
            dpi: 192,
            live_reload_enabled: false,
            verbose: false,
            version: String::new(),
            meta_filename: "meta.yaml".to_owned(),
            comments_db: SqliteCommentStore::default_path(&project_dir),
            embedded_preview: false,
            project_dir,
        }
    }
}

/// Run the server.
///
/// # Arguments
///
/// * `config` - Server configuration
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn run_server(config: ServerConfig) -> Result<(), ServerError> {
    // Create shared storage backend
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(FsStorage::with_meta_filename(
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
        dpi: config.dpi,
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

    // Bind first so the server-info file (and the in-memory notify token that
    // guards the internal endpoint) reflect the actually-bound address.
    let addr = SocketAddr::from_str(&format!("{}:{}", config.host, config.port))?;
    tracing::info!(address = %addr, "Starting server");
    let listener = tokio::net::TcpListener::bind(addr).await?;

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
        version: config.version.clone(),
        comment_store,
        notify_token,
        embedded_preview: config.embedded_preview,
    });

    // Create router
    let app = app::create_router(state);

    // Write the runtime server-info file (non-fatal: an unwritable .rw should
    // not stop serving). The guard removes the file when `run_server` returns.
    let _info_guard = server_info.and_then(|info| match info.write(&config.project_dir) {
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
        source_dir: config.docs_resolved.source_dir.clone(),
        cache_dir: if config.docs_resolved.cache_enabled {
            Some(config.docs_resolved.cache_dir())
        } else {
            None
        },
        kroki_url: config.diagrams_resolved.kroki_url.clone(),
        include_dirs: config.diagrams_resolved.include_dirs.clone(),
        dpi: config.diagrams_resolved.dpi,
        live_reload_enabled: config.live_reload.enabled,
        verbose,
        version,
        meta_filename: config.metadata.name.clone(),
        comments_db: SqliteCommentStore::default_path(&config.docs_resolved.project_dir),
        project_dir: config.docs_resolved.project_dir.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_config_default_project_dir_is_dot_rw() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.project_dir, std::path::PathBuf::from(".rw"));
    }
}
