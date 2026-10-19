//! CLI command implementations.

pub(crate) mod backstage;
pub(crate) mod comment;
pub(crate) mod confluence;
pub(crate) mod serve;
pub(crate) mod update;
pub(crate) use backstage::BackstageCommand;
pub(crate) use comment::CommentCommand;
pub(crate) use confluence::ConfluenceCommand;
pub(crate) use serve::ServeArgs;
pub(crate) use update::UpdateArgs;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use clap::Args;
use rw_config::{CliSettings, Config, ConfigError};

use crate::error::CliError;

/// Shared arguments that locate the project: `--config` or `--project-dir`.
#[derive(Args, Debug)]
pub(crate) struct ProjectArgs {
    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Root the project at this directory instead of discovering `rw.toml`
    /// upward from the current directory.
    ///
    /// Reads `<dir>/rw.toml` if present and otherwise uses defaults rooted
    /// there. The directory is used as given and is never walked up from, so a
    /// project cannot silently inherit configuration from a parent it does not
    /// own.
    #[arg(long, conflicts_with = "config")]
    pub project_dir: Option<PathBuf>,
}

impl ProjectArgs {
    pub(crate) fn load(&self, cli_settings: Option<&CliSettings>) -> Result<Config, ConfigError> {
        match self.project_dir.as_deref() {
            Some(dir) => Config::load_from_dir(dir, cli_settings),
            None => Config::load(self.config.as_deref(), cli_settings),
        }
    }
}

/// Ensure the `.rw/` data directory exists, with a `.gitignore` that keeps
/// everything rw writes there out of version control.
pub(crate) fn ensure_data_dir(data_dir: &Path) -> Result<(), CliError> {
    std::fs::create_dir_all(data_dir)?;

    let gitignore_path = data_dir.join(".gitignore");
    if !gitignore_path.exists() {
        let _ = std::fs::write(&gitignore_path, "# Automatically created by rw\n*\n");
    }

    Ok(())
}

/// Map a wildcard bind (`0.0.0.0` or `::`) to the matching loopback address:
/// it is not a connectable destination everywhere (Windows refuses it).
pub(crate) fn connectable_addr(bound: SocketAddr) -> SocketAddr {
    let ip = match bound.ip() {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, bound.port())
}
