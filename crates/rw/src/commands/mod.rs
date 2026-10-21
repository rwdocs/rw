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

use std::env;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Args;
use rw_cache::{Cache, NullCache};
use rw_config::{CliSettings, Config, ConfigError};
use rw_site::{PageRendererConfig, Site};
use rw_storage::Storage;
use rw_storage_fs::FsStorage;

use crate::error::CliError;

const README: &str = "README.md";

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
    /// Load the project's configuration, refusing a root with neither the docs
    /// directory nor a `README.md` homepage. Without `rw.toml` the root is the
    /// current directory, which from a subdirectory is the wrong one.
    pub(crate) fn load(&self, cli_settings: Option<&CliSettings>) -> Result<Config, CliError> {
        let config = self.load_any(cli_settings)?;
        let readme = config.project_dir.join("README.md");
        if !config.docs.source_dir.is_dir() && !readme.is_file() {
            return Err(CliError::Validation(format!(
                "project root {} has no docs: neither {} nor {} exists; \
                 run rw from the project root or pass --project-dir",
                config.project_dir.display(),
                config.docs.source_dir.display(),
                readme.display(),
            )));
        }
        Ok(config)
    }

    /// [`Self::load`] without the docs check.
    pub(crate) fn load_any(
        &self,
        cli_settings: Option<&CliSettings>,
    ) -> Result<Config, ConfigError> {
        match self.project_dir.as_deref() {
            Some(dir) => Config::load_from_dir(dir, cli_settings),
            None => Config::load(self.config.as_deref(), cli_settings),
        }
    }

    /// Whether the caller named the project root rather than leaving it to
    /// discovery.
    pub(crate) fn is_explicit(&self) -> bool {
        self.config.is_some() || self.project_dir.is_some()
    }
}

/// Build an [`FsStorage`] over the project's docs directory.
pub(crate) fn build_storage(config: &Config) -> FsStorage {
    FsStorage::new(config.project_dir.clone(), config.docs.source_dir.clone())
}

/// Build a read-only [`Site`] over the project's docs.
pub(crate) fn build_site(config: &Config) -> Site {
    let storage: Arc<dyn Storage> = Arc::new(build_storage(config));
    let cache: Arc<dyn Cache> = Arc::new(NullCache);
    let renderer_config = PageRendererConfig {
        kroki_url: config.diagrams.kroki_url.clone(),
        include_dirs: config.diagrams.include_dirs.clone(),
    };
    Site::new(storage, cache, renderer_config)
}

/// Whether the docs source directory lies outside the project, which places
/// no images.
pub(crate) fn source_dir_outside_project(config: &Config) -> bool {
    build_storage(config).source_dir_in_project().is_none()
}

/// The [`Document::source`](rw_storage::Document::source) paths `document`
/// may name, reading a relative one from the docs root and from the project
/// root.
pub(crate) fn source_paths(config: &Config, document: &Path) -> Vec<String> {
    // Only the parent is canonicalized, so a symlinked page keeps its name.
    let real = |path: &Path| {
        Some(
            fs::canonicalize(path.parent()?)
                .ok()?
                .join(path.file_name()?),
        )
    };
    // Missing in a README-only project.
    let source = fs::canonicalize(&config.docs.source_dir).ok();
    // Only while the source directory has no root page of its own, which
    // outside the project can be named `README.md` too.
    let readme = real(&config.project_dir.join(README)).filter(|_| {
        !fs::read_dir(&config.docs.source_dir).is_ok_and(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|entry| {
                    matches!(entry.file_name().to_str(), Some("index.md" | README))
                })
            })
        })
    });
    let source_in_project = build_storage(config).source_dir_in_project();
    [&config.docs.source_dir, &config.project_dir]
        .into_iter()
        .filter_map(|root| {
            let file = real(&root.join(document))?;
            let Some(path) = source
                .as_ref()
                .and_then(|source| file.strip_prefix(source).ok())
            else {
                return (Some(&file) == readme.as_ref()).then(|| README.to_owned());
            };
            let path = path.to_string_lossy().replace('\\', "/");
            Some(match source_in_project.as_deref() {
                None | Some("") => path,
                Some(dir) => format!("{dir}/{path}"),
            })
        })
        .collect()
}

/// `RW_DIAGRAMS_KROKI_URL`, for [`CliSettings::kroki_url_default`] in commands
/// that render diagrams. Empty counts as unset, which is how shells clear a
/// variable.
pub(crate) fn kroki_url_from_env() -> Option<String> {
    env::var("RW_DIAGRAMS_KROKI_URL")
        .ok()
        .filter(|url| !url.is_empty())
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
