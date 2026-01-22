//! HTTP server for Docstage documentation engine.
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
//! use docstage_server::{ServerConfig, run_server};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig {
//!         host: "127.0.0.1".to_string(),
//!         port: 8080,
//!         source_dir: PathBuf::from("docs"),
//!         cache_dir: Some(PathBuf::from(".cache")),
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
//! Browser ──HTTP──► Rust axum server (docstage-server)
//!                        │
//!                        ├─► API routes (Rust handlers)
//!                        │       │
//!                        │       └─► Direct call ──► PageRenderer
//!                        │       └─► Direct call ──► SiteLoader
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
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use docstage_site::{PageRenderer, PageRendererConfig, SiteLoader, SiteLoaderConfig};
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
    // Create PageRenderer
    let renderer_config = PageRendererConfig {
        cache_dir: config.cache_dir.clone(),
        version: config.version.clone(),
        extract_title: true,
        kroki_url: config.kroki_url.clone(),
        include_dirs: config.include_dirs.clone(),
        config_file: config.config_file.clone(),
        dpi: config.dpi,
    };
    let renderer = PageRenderer::new(renderer_config);

    // Create SiteLoader
    let site_loader_config = SiteLoaderConfig {
        source_dir: config.source_dir.clone(),
        cache_dir: config.cache_dir.clone(),
    };
    let site_loader = Arc::new(RwLock::new(SiteLoader::new(site_loader_config)));

    // Create live reload manager if enabled
    let live_reload = if config.live_reload_enabled {
        let (tx, _rx) = broadcast::channel::<live_reload::ReloadEvent>(100);
        let mut manager = live_reload::LiveReloadManager::new(
            config.source_dir.clone(),
            config.watch_patterns.clone(),
            Arc::clone(&site_loader),
            tx,
        );
        manager.start()?;
        Some(manager)
    } else {
        None
    };

    // Create app state
    let state = Arc::new(AppState {
        renderer,
        site_loader,
        live_reload,
        verbose: config.verbose,
        version: config.version.clone(),
    });

    // Create router
    let app = app::create_router(state);

    // Bind and run server
    let addr = SocketAddr::from_str(&format!("{}:{}", config.host, config.port))?;
    tracing::info!("Starting server at http://{}", addr);

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

/// Create server configuration from docstage config.
///
/// # Arguments
///
/// * `config` - Docstage configuration
/// * `version` - Application version
/// * `verbose` - Enable verbose output
#[must_use]
pub fn server_config_from_docstage_config(
    config: &docstage_config::Config,
    version: String,
    verbose: bool,
) -> ServerConfig {
    ServerConfig {
        host: config.server.host.clone(),
        port: config.server.port,
        source_dir: config.docs_resolved.source_dir.clone(),
        cache_dir: if config.docs_resolved.cache_enabled {
            Some(config.docs_resolved.cache_dir.clone())
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
    }
}
