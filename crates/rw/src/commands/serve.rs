//! `rw serve` command implementation.

use std::net::SocketAddr;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_error::error_chain;
use rw_server::{bind_listener, run_server};

use crate::commands::{ProjectArgs, connectable_addr, ensure_data_dir};
use crate::error::CliError;
use crate::output::Output;

/// Arguments for the serve command.
#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ServeArgs {
    #[command(flatten)]
    project: ProjectArgs,

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

    /// Disable live reload.
    #[arg(long)]
    no_live_reload: bool,

    /// Disable caching.
    #[arg(long)]
    no_cache: bool,

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

        let config = self.load_config()?;

        ensure_data_dir(&config.docs.data_dir)?;

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

        output.info(&format!("Starting server on http://{bound}"));
        output.info(&format!(
            "Source directory: {}",
            config.docs.source_dir.display()
        ));

        if config.docs.cache_enabled {
            output.info(&format!(
                "Cache directory: {}",
                config.docs.cache_dir().display()
            ));
        } else {
            output.info("Cache: disabled");
        }

        if let Some(kroki_url) = &config.diagrams.kroki_url {
            output.info(&format!("Kroki URL: {kroki_url}"));
        } else {
            output.info("Diagram rendering: disabled (no kroki_url in config)");
        }

        if config.live_reload.enabled {
            output.info("Live reload: enabled");
        } else {
            output.info("Live reload: disabled");
        }

        // The bound listener queues browser connections until run_server starts.
        // On macOS, `that_detached` waits for `/usr/bin/open` to hand off to
        // LaunchServices, not for the browser to exit. Awaiting it here means
        // a stalled launcher can delay serving despite using spawn_blocking.
        if self.open {
            let url = browser_url(bound);
            output.info(&format!("Opening {url} in your browser"));
            let launched = tokio::task::spawn_blocking(move || open::that_detached(&url))
                .await
                .map_err(|err| error_chain(&err))
                .and_then(|res| res.map_err(|err| error_chain(&err)));
            if let Err(err) = launched {
                output.warning(&format!("Could not open browser: {err}"));
            }
        }

        run_server(&config, version, listener).await?;

        Ok(())
    }

    fn load_config(&self) -> Result<Config, CliError> {
        let cli_settings = CliSettings {
            host: self.host.clone(),
            port: self.port,
            no_cache: self.no_cache,
            kroki_url: self.kroki_url.clone(),
            no_live_reload: self.no_live_reload,
        };
        self.project.load(Some(&cli_settings))
    }
}

fn browser_url(bound: SocketAddr) -> String {
    format!("http://{}", connectable_addr(bound))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use clap::Parser;
    use clap::error::ErrorKind;

    /// Minimal parser wrapper so `ServeArgs` can be parsed on its own in tests.
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: ServeArgs,
    }

    fn try_parse(argv: &[&str]) -> Result<ServeArgs, clap::Error> {
        TestCli::try_parse_from(argv).map(|cli| cli.args)
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

    #[test]
    fn project_dir_flag_is_accepted() {
        let args = try_parse(&["rw", "--project-dir", "/somewhere"]).expect("parse");
        assert_eq!(
            args.project.project_dir.as_deref(),
            Some(Path::new("/somewhere"))
        );
    }

    #[test]
    fn project_dir_has_no_short_flag() {
        // `-s` is deliberately not rebound: an old `-s docs` command line must
        // fail loudly rather than silently mean something new.
        assert!(try_parse(&["rw", "-s", "/somewhere"]).is_err());
    }

    #[test]
    fn source_dir_flag_is_rejected() {
        assert!(try_parse(&["rw", "--source-dir", "docs"]).is_err());
    }

    fn load_config_in(rw_toml: Option<&str>, flags: &[&str]) -> Config {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("docs")).unwrap();
        if let Some(contents) = rw_toml {
            std::fs::write(tmp.path().join("rw.toml"), contents).unwrap();
        }
        let dir = tmp.path().to_str().unwrap();
        let mut argv = vec!["rw", "--project-dir", dir];
        argv.extend_from_slice(flags);
        try_parse(&argv)
            .expect("parse")
            .load_config()
            .expect("load")
    }

    #[test]
    fn no_flags_keep_config_defaults() {
        let config = load_config_in(None, &[]);
        assert!(config.docs.cache_enabled);
        assert!(config.live_reload.enabled);
    }

    #[test]
    fn no_flags_keep_config_file_values() {
        let config = load_config_in(
            Some("[docs]\ncache_enabled = false\n\n[live_reload]\nenabled = false\n"),
            &[],
        );
        assert!(!config.docs.cache_enabled);
        assert!(!config.live_reload.enabled);
    }

    #[test]
    fn no_cache_disables_cache() {
        let config = load_config_in(None, &["--no-cache"]);
        assert!(!config.docs.cache_enabled);
        assert!(config.live_reload.enabled);
    }

    #[test]
    fn no_live_reload_disables_live_reload() {
        let config = load_config_in(None, &["--no-live-reload"]);
        assert!(!config.live_reload.enabled);
        assert!(config.docs.cache_enabled);
    }

    #[test]
    fn bool_valued_cache_and_live_reload_flags_are_rejected() {
        for argv in [
            &["rw", "--cache"][..],
            &["rw", "--cache", "true"],
            &["rw", "--cache=false"],
            &["rw", "--live-reload"],
            &["rw", "--live-reload", "true"],
            &["rw", "--live-reload=false"],
        ] {
            let err = try_parse(argv).expect_err("must reject");
            assert_eq!(err.kind(), ErrorKind::UnknownArgument, "{argv:?}");
        }
    }

    #[test]
    fn project_dir_conflicts_with_config() {
        let err = try_parse(&["rw", "--project-dir", "/a", "--config", "/b/rw.toml"])
            .expect_err("must conflict");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }
}
