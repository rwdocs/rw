//! `rw serve` command implementation.

use std::path::{Path, PathBuf};

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_server::{run_server, server_config_from_rw_config};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the serve command.
#[derive(Args)]
pub(crate) struct ServeArgs {
    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Documentation source directory (overrides config).
    #[arg(short, long)]
    source_dir: Option<PathBuf>,

    /// Host to bind to (overrides config).
    #[arg(long)]
    host: Option<String>,

    /// Port to bind to (overrides config).
    #[arg(short, long)]
    port: Option<u16>,

    /// Kroki server URL for diagram rendering (overrides config).
    #[arg(long)]
    kroki_url: Option<String>,

    /// Enable verbose output (show diagram warnings and timing logs).
    #[arg(short, long)]
    pub verbose: bool,

    /// Enable live reload (default: enabled).
    #[arg(long)]
    live_reload: Option<bool>,

    /// Disable live reload.
    #[arg(long, conflicts_with = "live_reload")]
    no_live_reload: bool,

    /// Enable caching (default: enabled).
    #[arg(long)]
    cache: Option<bool>,

    /// Disable caching.
    #[arg(long, conflicts_with = "cache")]
    no_cache: bool,
}

impl ServeArgs {
    /// Execute the serve command.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration fails or the server fails to start.
    pub(crate) async fn execute(self, version: &str) -> Result<(), CliError> {
        let output = Output::new();

        // Resolve flags before moving into CliSettings
        let cache_enabled = self.resolve_cache_enabled();
        let live_reload_enabled = self.resolve_live_reload_enabled();

        // Build CLI settings from args
        let cli_settings = CliSettings {
            host: self.host,
            port: self.port,
            source_dir: self.source_dir,
            cache_enabled,
            kroki_url: self.kroki_url,
            live_reload_enabled,
        };

        // Load config
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        // Ensure project directory exists with .gitignore
        ensure_project_dir(&config.docs_resolved.project_dir)?;

        // Print startup info
        output.info(&format!(
            "Starting server on {}:{}",
            config.server.host, config.server.port
        ));
        output.info(&format!(
            "Source directory: {}",
            config.docs_resolved.source_dir.display()
        ));

        if config.docs_resolved.cache_enabled {
            output.info(&format!(
                "Cache directory: {}",
                config.docs_resolved.cache_dir().display()
            ));
        } else {
            output.info("Cache: disabled");
        }

        if let Some(kroki_url) = &config.diagrams_resolved.kroki_url {
            output.info(&format!("Kroki URL: {kroki_url}"));
        } else {
            output.info("Diagram rendering: disabled (no kroki_url in config)");
        }

        if config.live_reload.enabled {
            output.info("Live reload: enabled");
        } else {
            output.info("Live reload: disabled");
        }

        // Build server config and run
        let server_config =
            server_config_from_rw_config(&config, version.to_string(), self.verbose);
        run_server(server_config)
            .await
            .map_err(|e| CliError::Server(e.to_string()))?;

        Ok(())
    }

    /// Resolve `cache_enabled` from --cache/--no-cache flags.
    fn resolve_cache_enabled(&self) -> Option<bool> {
        self.no_cache.then_some(false).or(self.cache)
    }

    /// Resolve `live_reload_enabled` from --live-reload/--no-live-reload flags.
    fn resolve_live_reload_enabled(&self) -> Option<bool> {
        self.no_live_reload.then_some(false).or(self.live_reload)
    }
}

/// Ensure the `.rw/` project directory exists with a `.gitignore`.
fn ensure_project_dir(project_dir: &Path) -> Result<(), CliError> {
    std::fs::create_dir_all(project_dir)
        .map_err(|e| CliError::Server(format!("Failed to create project directory: {e}")))?;

    let gitignore_path = project_dir.join(".gitignore");
    if !gitignore_path.exists() {
        // Auto-create .gitignore like mypy does for .mypy_cache
        let _ = std::fs::write(&gitignore_path, "# Automatically created by rw\n*\n");
    }

    Ok(())
}
