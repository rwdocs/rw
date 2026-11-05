//! `rw serve` command implementation.

use std::{io, net::SocketAddr, thread};

use clap::Args;
use rw_config::{CliSettings, Config, ServerConfig};
use rw_error::error_chain;
use rw_server::{bind_listener, run_server};

use crate::commands::{ProjectArgs, connectable_addr, ensure_data_dir, kroki_url_from_env};
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

    /// Also answer requests addressed to NAME (repeatable).
    ///
    /// Adds to `[server].allowed_hosts`; localhost and IP addresses are
    /// always allowed. A leading `.`, as in `.example.com`, also allows
    /// subdomains. `all` turns the host check off.
    #[arg(long = "allowed-host", value_name = "NAME")]
    allowed_hosts: Vec<String>,

    /// Kroki server URL for diagram rendering (overrides config).
    #[arg(long)]
    kroki_url: Option<String>,

    /// Also log info messages, such as scan timings.
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
        self.execute_with_browser(version, open::that_detached)
            .await
    }

    async fn execute_with_browser(
        self,
        version: &str,
        launch_browser: impl FnOnce(String) -> io::Result<()> + Send + 'static,
    ) -> Result<(), CliError> {
        let output = Output::new();

        let config = self.load_config()?;

        ensure_data_dir(&config.docs.data_dir)?;

        // Bind up front so we report the port the server actually listens on.
        // An explicit port (`-p` or `[server].port`) is a hard requirement; the
        // default port falls back to the next free one when it's busy.
        let requested_port = config.server.port.unwrap_or(ServerConfig::DEFAULT_PORT);
        let allow_fallback = config.server.port.is_none();
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

        if self.open {
            let url = browser_url(bound);
            output.info(&format!("Opening {url} in your browser"));
            // A stuck launcher must not delay serving or Tokio's blocking-pool shutdown.
            let launched = thread::Builder::new()
                .name("rw-open-browser".into())
                .spawn(move || {
                    if let Err(err) = launch_browser(url) {
                        Output::new()
                            .warning(&format!("Could not open browser: {}", error_chain(&err)));
                    }
                });
            if let Err(err) = launched {
                output.warning(&format!("Could not open browser: {}", error_chain(&err)));
            }
        }

        run_server(&config, version, listener).await?;

        Ok(())
    }

    fn load_config(&self) -> Result<Config, CliError> {
        let cli_settings = CliSettings {
            host: self.host.clone(),
            port: self.port,
            allowed_hosts: self.allowed_hosts.clone(),
            no_cache: self.no_cache,
            kroki_url: self.kroki_url.clone(),
            kroki_url_default: kroki_url_from_env(),
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
    use std::{fs, path::Path, sync::mpsc, time::Duration};

    use super::*;
    use clap::Parser;
    use clap::error::ErrorKind;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    /// Minimal parser wrapper so `ServeArgs` can be parsed on its own in tests.
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: ServeArgs,
    }

    fn try_parse(argv: &[&str]) -> Result<ServeArgs, clap::Error> {
        TestCli::try_parse_from(argv).map(|cli| cli.args)
    }

    async fn navigation_response(url: &str) -> String {
        let address = url.strip_prefix("http://").expect("HTTP browser URL");
        let mut stream = TcpStream::connect(address).await.expect("connect to rw");
        stream
            .write_all(
                b"GET /_api/navigation HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("request navigation");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("read navigation response");
        response
    }

    fn browser_project() -> (TempDir, ServeArgs) {
        let project = tempfile::tempdir().expect("project directory");
        fs::create_dir(project.path().join("docs")).expect("docs directory");
        fs::write(project.path().join("docs/index.md"), "# Home\n").expect("homepage");
        let args = try_parse(&[
            "rw",
            "--project-dir",
            project.path().to_str().expect("project path"),
            "--host",
            "127.0.0.1",
            "--open",
            "--no-cache",
            "--no-live-reload",
        ])
        .expect("serve arguments");
        (project, args)
    }

    #[tokio::test]
    async fn serves_requests_while_browser_launcher_is_blocked() {
        let (_project, args) = browser_project();
        let (started_tx, started_rx) = oneshot::channel();
        let (finished_tx, mut finished_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let server = tokio::spawn(args.execute_with_browser("test", move |url| {
            started_tx.send(url).expect("report browser URL");
            let _ = release_rx.recv();
            let _ = finished_tx.send(());
            Err(io::Error::other("test browser launcher failure"))
        }));
        let url = match timeout(Duration::from_secs(5), started_rx).await {
            Ok(Ok(url)) => url,
            Ok(Err(_)) => panic!("server exited before launching browser: {:?}", server.await),
            Err(err) => panic!("browser launcher did not start: {err}"),
        };
        let response = timeout(Duration::from_secs(5), navigation_response(&url))
            .await
            .expect("HTTP responds before the browser launcher returns");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert_eq!(
            finished_rx.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        );

        release_tx.send(()).expect("release browser launcher");
        finished_rx.await.expect("browser launcher returns");
        let response = timeout(Duration::from_secs(5), navigation_response(&url))
            .await
            .expect("HTTP responds after releasing the browser launcher");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        server.abort();
        assert!(
            server
                .await
                .expect_err("server task aborted")
                .is_cancelled()
        );
    }

    #[test]
    fn blocked_browser_launcher_does_not_hold_runtime_shutdown() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (_project, args) = browser_project();
            let runtime = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("server runtime");
            runtime.block_on(async {
                let (started_tx, started_rx) = oneshot::channel();
                let server = tokio::spawn(args.execute_with_browser("test", move |_| {
                    let _ = started_tx.send(());
                    let _ = release_rx.recv();
                    Ok(())
                }));
                timeout(Duration::from_secs(5), started_rx)
                    .await
                    .expect("browser launcher starts")
                    .expect("browser launcher running");
                server.abort();
                assert!(
                    server
                        .await
                        .expect_err("server task aborted")
                        .is_cancelled()
                );
            });
            drop(runtime);
            let _ = shutdown_tx.send(());
        });
        let shutdown = shutdown_rx.recv_timeout(Duration::from_secs(10));
        drop(release_tx);
        worker.join().expect("server runtime thread");
        shutdown.expect("runtime exits without waiting for the browser launcher");
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
    fn allowed_host_flags_add_to_config() {
        let config = load_config_in(
            Some("[server]\nallowed_hosts = [\".ts.net\"]\n"),
            &["--allowed-host", "docs.example", "--allowed-host", "all"],
        );
        assert_eq!(
            config.server.allowed_hosts,
            [".ts.net", "docs.example", "all"]
        );
    }

    #[test]
    fn project_dir_conflicts_with_config() {
        let err = try_parse(&["rw", "--project-dir", "/a", "--config", "/b/rw.toml"])
            .expect_err("must conflict");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }
}
