//! `rw serve` command implementation.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_server::{bind_listener, run_server, server_config_from_rw_config};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the serve command.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
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

    /// Serve an embedded preview page (host-app shell). Hidden: a dev/testing
    /// aid, not a supported end-user feature, so it stays out of `--help`.
    #[arg(long = "embedded", hide = true)]
    embedded_preview: bool,

    /// Open the site in your default browser once the server is ready.
    #[arg(short = 'o', long)]
    open: bool,
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

        // Bind up front so we report the port the server actually listens on.
        // An explicit port (`-p` or `[server].port`) is a hard requirement; the
        // default port falls back to the next free one when it's busy.
        let requested_port = config.server.port;
        let allow_fallback = !config.server.port_explicit;
        let listener = bind_listener(&config.server.host, requested_port, allow_fallback).await?;
        let bound = listener.local_addr()?;
        if bound.port() != requested_port {
            output.warning(&format!(
                "Port {requested_port} is in use, using {} instead",
                bound.port()
            ));
        }

        // Print startup info
        output.info(&format!("Starting server on http://{bound}"));
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

        if self.embedded_preview {
            output.info("Embedded preview: enabled");
        }

        // Build server config
        let mut server_config =
            server_config_from_rw_config(&config, version.to_owned(), self.verbose);
        server_config.embedded_preview = self.embedded_preview;

        // Open the browser at the bound URL, once, before serving. The listener
        // is already bound, so the browser's connection is accepted into the
        // socket backlog and served once run_server starts. `open::that_detached`
        // spawns the OS launcher without waiting for it, so a launcher that would
        // block (an odd `xdg-open`, a WSL edge case) cannot stall startup. A
        // launch failure (headless box, no default browser) is a warning, not
        // fatal.
        if self.open {
            let url = browser_url(bound);
            output.info(&format!("Opening {url} in your browser"));
            let launched = tokio::task::spawn_blocking(move || open::that_detached(&url))
                .await
                .map_err(|err| err.to_string())
                .and_then(|res| res.map_err(|err| err.to_string()));
            if let Err(err) = launched {
                output.warning(&format!("Could not open browser: {err}"));
            }
        }

        run_server(server_config, listener).await?;

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
    std::fs::create_dir_all(project_dir)?;

    let gitignore_path = project_dir.join(".gitignore");
    if !gitignore_path.exists() {
        let _ = std::fs::write(&gitignore_path, "# Automatically created by rw\n*\n");
    }

    Ok(())
}

/// Build the URL to open in the browser for the address the server bound to.
///
/// When the server bound to an unspecified address (`0.0.0.0` or `::`, i.e. all
/// interfaces), that address is not something a browser can connect to, so the
/// URL targets the matching loopback address instead. Any concrete bound
/// address is used as-is.
fn browser_url(bound: SocketAddr) -> String {
    let addr = if bound.ip().is_unspecified() {
        let loopback = match bound.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        SocketAddr::new(loopback, bound.port())
    } else {
        bound
    };
    format!("http://{addr}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Minimal parser wrapper so `ServeArgs` can be parsed on its own in tests.
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: ServeArgs,
    }

    fn parse(argv: &[&str]) -> ServeArgs {
        TestCli::try_parse_from(argv)
            .expect("args should parse")
            .args
    }

    #[test]
    fn open_flag_defaults_to_false() {
        assert!(!parse(&["rw"]).open);
    }

    #[test]
    fn long_open_flag_sets_true() {
        assert!(parse(&["rw", "--open"]).open);
    }

    #[test]
    fn short_open_flag_sets_true() {
        assert!(parse(&["rw", "-o"]).open);
    }

    #[test]
    fn browser_url_keeps_concrete_lan_ipv4() {
        let addr = "192.168.1.5:8080".parse().unwrap();
        assert_eq!(browser_url(addr), "http://192.168.1.5:8080");
    }

    #[test]
    fn browser_url_rewrites_unspecified_ipv4_to_loopback() {
        let addr = "0.0.0.0:7991".parse().unwrap();
        assert_eq!(browser_url(addr), "http://127.0.0.1:7991");
    }

    #[test]
    fn browser_url_rewrites_unspecified_ipv6_to_loopback() {
        let addr = "[::]:7991".parse().unwrap();
        assert_eq!(browser_url(addr), "http://[::1]:7991");
    }
}
