//! HTTP server for RW documentation engine.
//!
//! This crate provides a native Rust HTTP server using axum, serving:
//! - API endpoints for page rendering and navigation
//! - Static files for the frontend SPA
//! - WebSocket endpoint for live reload during development
//!
//! # Static Asset Modes
//!
//! This server supports two modes for serving static assets:
//!
//! - **Development** (default): Serves files from `frontend/dist` directory
//! - **Production** (`embed-assets` feature): Embeds assets in the binary
//!
//! # Quick Start
//!
//! ```ignore
//! use std::path::PathBuf;
//! use rw_server::{ServerConfig, run_server};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig {
//!         host: "127.0.0.1".to_string(),
//!         port: 8080,
//!         source_dir: PathBuf::from("docs"),
//!         cache_dir: Some(PathBuf::from(".rw/cache")),
//!         kroki_url: Some("https://kroki.io".to_string()),
//!         include_dirs: vec![PathBuf::from(".")],
//!         config_file: None,
//!         dpi: 192,
//!         live_reload_enabled: true,
//!         watch_patterns: None,
//!         verbose: false,
//!         version: "1.0.0".to_string(),
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
//!                        └─► Static files (embedded or tower-http)
//! ```

mod app;
mod error;
mod handlers;
mod live_reload;
mod middleware;
mod state;
mod static_files;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use rw_site::{Site, SiteConfig};
use rw_storage_fs::FsStorage;
use state::AppState;
use tokio::sync::broadcast;

/// Server configuration.
#[derive(Clone, Debug)]
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
    /// `PlantUML` config file.
    pub config_file: Option<String>,
    /// Diagram DPI.
    pub dpi: u32,
    /// Enable live reload.
    pub live_reload_enabled: bool,
    /// Watch patterns for live reload.
    pub watch_patterns: Option<Vec<String>>,
    /// Enable verbose output.
    pub verbose: bool,
    /// Application version (for cache invalidation).
    pub version: String,
    /// Metadata file name (default: "meta.yaml").
    pub meta_filename: String,
    /// README.md path to use as homepage fallback.
    pub readme_path: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            source_dir: PathBuf::from("docs"),
            cache_dir: None,
            kroki_url: None,
            include_dirs: Vec::new(),
            config_file: None,
            dpi: 192,
            live_reload_enabled: false,
            watch_patterns: None,
            verbose: false,
            version: String::new(),
            meta_filename: "meta.yaml".to_string(),
            readme_path: PathBuf::from("README.md"),
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
pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create shared storage backend
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(
        FsStorage::with_meta_filename(config.source_dir.clone(), &config.meta_filename)
            .with_readme(config.readme_path.clone()),
    );

    // Create unified Site with storage and configuration
    let site_config = SiteConfig {
        cache_dir: config.cache_dir.clone(),
        extract_title: true,
        kroki_url: config.kroki_url.clone(),
        include_dirs: config.include_dirs.clone(),
        config_file: config.config_file.clone(),
        dpi: config.dpi,
    };
    let site = Arc::new(Site::new(Arc::clone(&storage), site_config, &config.version));

    // Create live reload manager if enabled
    let live_reload = if config.live_reload_enabled {
        let (tx, _rx) = broadcast::channel::<live_reload::ReloadEvent>(100);
        let mut manager = live_reload::LiveReloadManager::new(Arc::clone(&site), tx);
        manager.start(storage.as_ref())?;
        Some(manager)
    } else {
        None
    };

    // Create app state
    let state = Arc::new(AppState {
        site,
        live_reload,
        verbose: config.verbose,
        version: config.version.clone(),
    });

    // Create router
    let app = app::create_router(state);

    // Bind and run server
    let addr = SocketAddr::from_str(&format!("{}:{}", config.host, config.port))?;
    tracing::info!(address = %addr, "Starting server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Wait for shutdown signal (Ctrl-C).
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
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
    // Auto-detect README.md as homepage fallback (FsStorage checks existence at runtime)
    let readme_path = config
        .config_path
        .as_ref()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
        .join("README.md");

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
        config_file: config.diagrams_resolved.config_file.clone(),
        dpi: config.diagrams_resolved.dpi,
        live_reload_enabled: config.live_reload.enabled,
        watch_patterns: config.live_reload.watch_patterns.clone(),
        verbose,
        version,
        meta_filename: config.metadata.name.clone(),
        readme_path,
    }
}
