//! Configuration management for RW.
//!
//! Parses `rw.toml` configuration files with serde and provides
//! auto-discovery of config files in parent directories.
//!
//! CLI settings can be applied during load via [`CliSettings`].
//!
//! ## Environment Variable Expansion
//!
//! String configuration values support environment variable expansion:
//!
//! - `${VAR}` - expands to the value of VAR, errors if unset
//! - `${VAR:-default}` - expands to VAR if set, otherwise uses default
//!
//! Expanded fields:
//! - `server.host`
//! - `diagrams.kroki_url`
//!
//! ## Environment Variable Fallback
//!
//! Some fields fall back to a dedicated environment variable when no
//! value is supplied via `rw.toml` or CLI flags:
//!
//! - `diagrams.kroki_url` ← `RW_DIAGRAMS_KROKI_URL`
//!
//! The fallback is the lowest-priority source: an `rw.toml` value or a
//! CLI flag always wins. Empty env-var values are treated as unset.

mod expand;

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// CLI settings that override configuration file values.
///
/// All fields are optional. Only non-None values override the loaded config.
#[derive(Debug, Default)]
pub struct CliSettings {
    /// Override server host.
    pub host: Option<String>,
    /// Override server port.
    pub port: Option<u16>,
    /// Override cache enabled flag.
    pub cache_enabled: Option<bool>,
    /// Override Kroki URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Override live reload enabled flag.
    pub live_reload_enabled: Option<bool>,
}

/// Configuration filename to search for.
const CONFIG_FILENAME: &str = "rw.toml";

/// Name of the per-project data directory. Everything RW writes for a project
/// — the cache, the comments DB, and the server-info file — lives under it, so
/// the resolved `data_dir` is the single source for all of those paths.
pub const DATA_DIR_NAME: &str = ".rw";

/// Application configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Server configuration.
    pub server: ServerConfig,
    /// Documentation configuration (paths are relative strings from TOML).
    #[serde(default)]
    docs: DocsConfigRaw,
    /// Diagram rendering configuration (optional section).
    /// `kroki_url` is optional; when absent, diagram fences fall through.
    diagrams: Option<DiagramsConfigRaw>,
    /// Live reload configuration.
    pub live_reload: LiveReloadConfig,
    /// Metadata configuration.
    pub metadata: MetadataConfig,

    /// Resolved docs configuration (set after loading).
    #[serde(skip)]
    pub docs_resolved: DocsConfig,
    /// Resolved diagrams configuration (set after loading).
    #[serde(skip)]
    pub diagrams_resolved: DiagramsConfig,

    /// Directory the project is rooted at — the directory containing `rw.toml`,
    /// or, when no config file is involved, the directory the caller rooted the
    /// configuration at.
    ///
    /// This is the base [`Self::resolve_paths`] resolves against, so
    /// [`DocsConfig::source_dir`], [`DocsConfig::data_dir`], and
    /// [`DiagramsConfig::include_dirs`] are all derived from it. A CLI override
    /// breaks that relationship in one direction: `source_dir` can be replaced
    /// outright without moving the project root, so it is not safe to assume
    /// `source_dir` sits inside `project_dir`.
    ///
    /// Stored as supplied, not canonicalized: a relative path in yields a
    /// relative project dir out, matching the paths derived from it.
    // `#[serde(skip)]` is load-bearing: `Config` is `#[serde(default)]`, so
    // without it a stray `project_dir = "…"` key in an `rw.toml` would
    // deserialize straight into this field.
    #[serde(skip)]
    pub project_dir: PathBuf,
}

impl Default for Config {
    #[allow(clippy::derivable_impls)]
    fn default() -> Self {
        Self::default_with_base(Path::new("."))
    }
}

/// Server configuration.
#[derive(Debug)]
pub struct ServerConfig {
    /// Server host address.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Whether the port was set explicitly — via `[server].port` in `rw.toml` or
    /// the `-p`/`--port` CLI flag — rather than left at the built-in default.
    ///
    /// `rw serve` falls back to the next free port when the *default* port is
    /// busy, but treats an explicit port as a hard requirement (fail if busy).
    pub port_explicit: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 7979,
            port_explicit: false,
        }
    }
}

impl<'de> Deserialize<'de> for ServerConfig {
    /// Deserialize `[server]`, recording whether `port` was present so an
    /// explicitly-set port can be distinguished from the built-in default. A
    /// present `port` sets [`ServerConfig::port_explicit`].
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Raw {
            host: Option<String>,
            port: Option<u16>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let defaults = ServerConfig::default();
        Ok(ServerConfig {
            host: raw.host.unwrap_or(defaults.host),
            port_explicit: raw.port.is_some(),
            port: raw.port.unwrap_or(defaults.port),
        })
    }
}

/// Raw docs configuration as parsed from TOML (paths as strings).
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DocsConfigRaw {
    source_dir: Option<String>,
    cache_enabled: Option<bool>,
}

/// Resolved documentation configuration with absolute paths.
#[derive(Debug, Default)]
pub struct DocsConfig {
    /// Source directory for markdown files.
    pub source_dir: PathBuf,
    /// Absolute path to the project's data directory — see [`DATA_DIR_NAME`].
    pub data_dir: PathBuf,
    /// Whether caching is enabled.
    pub cache_enabled: bool,
}

impl DocsConfig {
    /// Cache directory path (.rw/cache/).
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }
}

/// Raw diagrams configuration as parsed from TOML (paths as strings).
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DiagramsConfigRaw {
    kroki_url: Option<String>,
    include_dirs: Option<Vec<String>>,
}

/// Resolved diagram rendering configuration with absolute paths.
#[derive(Debug, Default)]
pub struct DiagramsConfig {
    /// Kroki server URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` directives.
    pub include_dirs: Vec<PathBuf>,
}

/// Live reload configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LiveReloadConfig {
    /// Whether live reload is enabled.
    pub enabled: bool,
}

impl Default for LiveReloadConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Metadata configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct MetadataConfig {
    /// Filename for metadata sidecar files.
    pub name: String,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            name: "meta.yaml".to_owned(),
        }
    }
}

/// Configuration error.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// File not found.
    #[error("Configuration file not found: {}", .0.display())]
    NotFound(PathBuf),
    /// Explicitly supplied project directory does not exist, or is not a
    /// directory.
    #[error("Project directory not found: {}", .0.display())]
    ProjectDirNotFound(PathBuf),
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    /// Validation error.
    #[error("Configuration error: {0}")]
    Validation(String),
    /// Environment variable error during expansion.
    #[error("Environment variable error in {field}: {message}")]
    EnvVar {
        /// Config field path (e.g., "`diagrams.kroki_url`").
        field: String,
        /// Error message (e.g., "${`KROKI_URL`} not set").
        message: String,
    },
}

/// Require a string field to be non-empty.
fn require_non_empty(value: &str, field: &str) -> Result<(), ConfigError> {
    if value.is_empty() {
        return Err(ConfigError::Validation(format!("{field} cannot be empty")));
    }
    Ok(())
}

/// Require a URL field to use http:// or https:// scheme.
fn require_http_url(url: &str, field: &str) -> Result<(), ConfigError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ConfigError::Validation(format!(
            "{field} must start with http:// or https://"
        )));
    }
    Ok(())
}

impl Config {
    /// Load configuration from file with optional CLI settings.
    ///
    /// If `config_path` is provided, loads from that file.
    /// Otherwise, searches for `rw.toml` in current directory and parents.
    ///
    /// CLI settings are applied after loading and path resolution, allowing CLI
    /// arguments to take precedence over config file values.
    ///
    /// # Errors
    ///
    /// Returns error if explicit `config_path` doesn't exist or parsing fails.
    pub fn load(
        config_path: Option<&Path>,
        cli_settings: Option<&CliSettings>,
    ) -> Result<Self, ConfigError> {
        let config = if let Some(path) = config_path {
            if !path.exists() {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            Self::load_from_file(path)?
        } else if let Some(discovered) = Self::discover_config() {
            Self::load_from_file(&discovered)?
        } else {
            Self::default_with_cwd()
        };

        config.finish(cli_settings)
    }

    /// Load configuration rooted at `dir`.
    ///
    /// Loads `dir/rw.toml` if it exists; otherwise returns defaults rooted at
    /// `dir`. Either way [`Config::project_dir`] is `dir` — see that field for
    /// what the derived paths do and do not inherit from it.
    ///
    /// Unlike [`Self::load`]'s discovery branch, this does **not** walk up to a
    /// parent directory. The caller has said where the project is; an
    /// embedding host pointing at one entity's directory must not silently
    /// inherit an `rw.toml` from a directory it does not own. This matches
    /// `load`'s explicit-path branch, which does not walk either.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::ProjectDirNotFound`] if `dir` does not exist or
    /// is not a directory, an error if `dir/rw.toml` exists but cannot be read
    /// or parsed, or an error if the resulting configuration fails validation.
    pub fn load_from_dir(
        dir: &Path,
        cli_settings: Option<&CliSettings>,
    ) -> Result<Self, ConfigError> {
        // A missing directory must not resolve to defaults rooted at it, or the
        // caller materializes the mistake instead of reporting it. An explicit
        // config path already errors when the file is absent; this is the same
        // contract for an explicit root.
        if !dir.is_dir() {
            return Err(ConfigError::ProjectDirNotFound(dir.to_path_buf()));
        }

        let config_path = dir.join(CONFIG_FILENAME);
        let config = if config_path.exists() {
            Self::load_from_file(&config_path)?
        } else {
            Self::default_with_base(dir)
        };

        config.finish(cli_settings)
    }

    /// Apply CLI settings to the configuration.
    fn apply_cli_settings(&mut self, settings: &CliSettings) {
        if let Some(host) = &settings.host {
            self.server.host.clone_from(host);
        }
        if let Some(port) = settings.port {
            self.server.port = port;
            // An explicit `-p`/`--port` is a hard requirement — no port fallback.
            self.server.port_explicit = true;
        }
        if let Some(cache_enabled) = settings.cache_enabled {
            self.docs_resolved.cache_enabled = cache_enabled;
        }
        if let Some(kroki_url) = &settings.kroki_url {
            self.diagrams_resolved.kroki_url = Some(kroki_url.clone());
        }
        if let Some(live_reload_enabled) = settings.live_reload_enabled {
            self.live_reload.enabled = live_reload_enabled;
        }
    }

    /// Lowest-priority source for `diagrams.kroki_url`. Empty
    /// `RW_DIAGRAMS_KROKI_URL` is treated as unset, matching how shells export
    /// cleared variables.
    fn apply_env_var_fallback(&mut self) {
        const ENV_VAR: &str = "RW_DIAGRAMS_KROKI_URL";

        if self.diagrams_resolved.kroki_url.is_some() {
            return;
        }
        if let Ok(value) = std::env::var(ENV_VAR)
            && !value.is_empty()
        {
            self.diagrams_resolved.kroki_url = Some(value);
        }
    }

    /// Search for config file in current directory and parents.
    fn discover_config() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;
        loop {
            let candidate = current.join(CONFIG_FILENAME);
            if candidate.exists() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// Create default config with paths relative to current working directory.
    fn default_with_cwd() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Self::default_with_base(&cwd)
    }

    /// Create default config with paths relative to given base directory.
    fn default_with_base(base: &Path) -> Self {
        Self {
            server: ServerConfig::default(),
            docs: DocsConfigRaw::default(),
            diagrams: None,
            live_reload: LiveReloadConfig::default(),
            metadata: MetadataConfig::default(),
            docs_resolved: DocsConfig {
                source_dir: base.join("docs"),
                data_dir: base.join(DATA_DIR_NAME),
                cache_enabled: true,
            },
            diagrams_resolved: DiagramsConfig::default(),
            project_dir: base.to_path_buf(),
        }
    }

    /// Load configuration from a specific file.
    fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&content)?;

        // Expand environment variables before path resolution
        config.expand_env_vars()?;

        config.project_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        config.resolve_paths();

        Ok(config)
    }

    /// Validate configuration values.
    ///
    /// Checks that all required fields are properly set and contain valid values.
    /// Called automatically after loading from file.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::Validation` if any validation fails.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_server()?;
        self.validate_diagrams()?;
        Ok(())
    }

    /// Validate server configuration.
    fn validate_server(&self) -> Result<(), ConfigError> {
        require_non_empty(&self.server.host, "server.host")?;

        // Port 0 is technically valid (OS assigns a random port), but it's
        // unlikely to be intentional in a config file
        if self.server.port == 0 {
            return Err(ConfigError::Validation(
                "server.port cannot be 0".to_owned(),
            ));
        }

        Ok(())
    }

    /// Validate diagrams configuration.
    fn validate_diagrams(&self) -> Result<(), ConfigError> {
        // `kroki_url` is optional — when absent, diagram fences render as
        // syntax-highlighted code (matching `rw serve`'s default).
        if let Some(ref kroki_url) = self.diagrams_resolved.kroki_url {
            require_non_empty(kroki_url, "diagrams.kroki_url")?;
            require_http_url(kroki_url, "diagrams.kroki_url")?;
        }

        Ok(())
    }

    /// Expand environment variable references in configuration strings.
    fn expand_env_vars(&mut self) -> Result<(), ConfigError> {
        // Server config
        self.server.host = expand::expand_env(&self.server.host, "server.host")?;

        // Diagrams config (if present)
        if let Some(ref mut diagrams) = self.diagrams
            && let Some(ref url) = diagrams.kroki_url
        {
            diagrams.kroki_url = Some(expand::expand_env(url, "diagrams.kroki_url")?);
        }

        Ok(())
    }

    /// Resolve relative paths in `docs` and `diagrams` against
    /// [`Config::project_dir`], which must already be set.
    ///
    /// Do **not** reintroduce a base parameter: reading the field keeps exactly
    /// one value in play, so the field and the paths derived from it cannot
    /// disagree.
    ///
    /// `[diagrams].kroki_url` is optional; downstream consumers decide how to
    /// react when absent. Does not validate field presence — callers must run
    /// [`Self::validate`] afterwards.
    fn resolve_paths(&mut self) {
        let project_dir = self.project_dir.clone();
        let resolve = |path: Option<&str>, default: &str| project_dir.join(path.unwrap_or(default));

        self.docs_resolved = DocsConfig {
            source_dir: resolve(self.docs.source_dir.as_deref(), "docs"),
            data_dir: project_dir.join(DATA_DIR_NAME),
            cache_enabled: self.docs.cache_enabled.unwrap_or(true),
        };

        self.diagrams_resolved = match &self.diagrams {
            Some(diagrams) => {
                let include_dirs = diagrams
                    .include_dirs
                    .iter()
                    .flatten()
                    .map(|d| project_dir.join(d))
                    .collect();
                DiagramsConfig {
                    kroki_url: diagrams.kroki_url.clone(),
                    include_dirs,
                }
            }
            None => DiagramsConfig::default(),
        };
    }

    /// Apply the caller's overrides and the env-var fallback, then validate.
    ///
    /// Shared by [`Self::load`] and [`Self::load_from_dir`], which differ only
    /// in how they choose a config source. Add new load-time steps here, not in
    /// the callers: a step added to one entry point alone is silently skipped
    /// by the other.
    fn finish(mut self, cli_settings: Option<&CliSettings>) -> Result<Self, ConfigError> {
        if let Some(settings) = cli_settings {
            self.apply_cli_settings(settings);
        }
        self.apply_env_var_fallback();
        self.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    /// Serializes tests that mutate `RW_DIAGRAMS_KROKI_URL`. The process env is
    /// shared across cargo's parallel tests, so any test that touches this
    /// variable must hold the guard for its full duration.
    static RW_DIAGRAMS_KROKI_URL_GUARD: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    /// RAII helper: acquire the guard and clear `RW_DIAGRAMS_KROKI_URL` for the
    /// duration of the test. Set the var inside the test (with `unsafe`)
    /// after constructing this; both the guard and the unset happen on
    /// drop, even on panic.
    struct EnvGuard {
        _lock: parking_lot::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let lock = RW_DIAGRAMS_KROKI_URL_GUARD.lock();
            // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD; no other thread reads
            // or writes RW_DIAGRAMS_KROKI_URL for the duration of this guard.
            unsafe { std::env::remove_var("RW_DIAGRAMS_KROKI_URL") };
            Self { _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still holding the mutex guard.
            unsafe { std::env::remove_var("RW_DIAGRAMS_KROKI_URL") };
        }
    }

    /// Make a tempdir + `rw.toml` inside it, returning the `TempDir` (drops the
    /// directory on scope exit, including panics) plus the path to the toml.
    fn rw_toml_tempdir(label: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("rw-config-test-{label}-"))
            .tempdir()
            .expect("create tempdir");
        let toml_path = dir.path().join("rw.toml");
        std::fs::write(&toml_path, contents).expect("write rw.toml");
        (dir, toml_path)
    }

    #[test]
    fn test_default_config() {
        let config = Config::default_with_base(Path::new("/test"));
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 7979);
        assert_eq!(config.docs_resolved.source_dir, PathBuf::from("/test/docs"));
        assert_eq!(config.docs_resolved.data_dir, PathBuf::from("/test/.rw"));
        assert_eq!(
            config.docs_resolved.cache_dir(),
            PathBuf::from("/test/.rw/cache")
        );
        assert!(config.docs_resolved.cache_enabled);
        assert!(config.live_reload.enabled);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = "";
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 7979);
    }

    #[test]
    fn test_parse_server_config() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 9000
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        // A port set in `rw.toml` is explicit — not eligible for port fallback.
        assert!(config.server.port_explicit);
    }

    #[test]
    fn test_default_port_not_explicit() {
        // No `[server].port` anywhere → the default 7979 stays eligible for
        // fallback to the next free port.
        assert!(
            !Config::default_with_base(Path::new("/test"))
                .server
                .port_explicit
        );
        let from_empty: Config = toml::from_str("").unwrap();
        assert!(!from_empty.server.port_explicit);
        // `[server]` present but without a `port` key is likewise not explicit.
        let host_only: Config = toml::from_str("[server]\nhost = \"0.0.0.0\"\n").unwrap();
        assert!(!host_only.server.port_explicit);
        assert_eq!(host_only.server.port, 7979);
    }

    #[test]
    fn test_cli_port_marks_explicit() {
        let mut config = Config::default_with_base(Path::new("/test"));
        assert!(!config.server.port_explicit);
        config.apply_cli_settings(&CliSettings {
            port: Some(9000),
            ..Default::default()
        });
        assert_eq!(config.server.port, 9000);
        assert!(config.server.port_explicit);
    }

    #[test]
    fn test_parse_live_reload_config() {
        let toml = r"
[live_reload]
enabled = false
";
        let config: Config = toml::from_str(toml).unwrap();
        assert!(!config.live_reload.enabled);
    }

    #[test]
    fn test_resolve_paths() {
        let toml = r#"
[docs]
source_dir = "documentation"

[diagrams]
kroki_url = "https://kroki.io"
include_dirs = ["diagrams", "shared/diagrams"]
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.project_dir = PathBuf::from("/project");
        config.resolve_paths();

        assert_eq!(
            config.docs_resolved.source_dir,
            PathBuf::from("/project/documentation")
        );
        assert_eq!(config.docs_resolved.data_dir, PathBuf::from("/project/.rw"));
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://kroki.io".to_owned())
        );
        assert_eq!(
            config.diagrams_resolved.include_dirs,
            vec![
                PathBuf::from("/project/diagrams"),
                PathBuf::from("/project/shared/diagrams")
            ]
        );
    }

    #[test]
    fn test_diagrams_section_without_kroki_url_is_valid() {
        let toml = r#"
[diagrams]
include_dirs = ["plantuml-includes"]
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.project_dir = PathBuf::from("/test");
        config.resolve_paths();
        assert!(config.diagrams_resolved.kroki_url.is_none());
        assert_eq!(config.diagrams_resolved.include_dirs.len(), 1);
    }

    #[test]
    fn test_stale_confluence_section_is_silently_ignored() {
        let toml = r#"
[confluence]
base_url = "https://example.com"
access_token = "x"
access_secret = "y"
"#;
        // serde silently ignores unknown top-level fields. This guards against
        // a future hardening (e.g. `#[serde(deny_unknown_fields)]`) breaking
        // upgrades from configs that still contain the deleted [confluence] section.
        let mut config: Config =
            toml::from_str(toml).expect("stale [confluence] should be silently ignored");
        config.project_dir = PathBuf::from("/test");
        config.resolve_paths();

        // The stale section must NOT leak into resolved state.
        assert!(config.diagrams_resolved.kroki_url.is_none());
        assert!(config.diagrams_resolved.include_dirs.is_empty());
        assert_eq!(config.docs_resolved.source_dir, PathBuf::from("/test/docs"));
    }

    #[test]
    fn test_no_diagrams_section_is_valid() {
        let toml = r#"
[docs]
source_dir = "documentation"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.project_dir = PathBuf::from("/project");
        config.resolve_paths();

        assert!(config.diagrams_resolved.kroki_url.is_none());
        assert!(config.diagrams_resolved.include_dirs.is_empty());
    }

    #[test]
    fn test_apply_cli_settings_host() {
        let mut config = Config::default_with_base(Path::new("/test"));
        let overrides = CliSettings {
            host: Some("0.0.0.0".to_owned()),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 7979); // Unchanged
    }

    #[test]
    fn test_apply_cli_settings_port() {
        let mut config = Config::default_with_base(Path::new("/test"));
        let overrides = CliSettings {
            port: Some(9000),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1"); // Unchanged
    }

    #[test]
    fn test_apply_cli_settings_cache_enabled() {
        let mut config = Config::default_with_base(Path::new("/test"));
        assert!(config.docs_resolved.cache_enabled);

        let overrides = CliSettings {
            cache_enabled: Some(false),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert!(!config.docs_resolved.cache_enabled);
    }

    #[test]
    fn test_apply_cli_settings_kroki_url() {
        let mut config = Config::default_with_base(Path::new("/test"));
        assert!(config.diagrams_resolved.kroki_url.is_none());

        let overrides = CliSettings {
            kroki_url: Some("https://kroki.example.com".to_owned()),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://kroki.example.com".to_owned())
        );
    }

    #[test]
    fn test_apply_cli_settings_live_reload() {
        let mut config = Config::default_with_base(Path::new("/test"));
        assert!(config.live_reload.enabled);

        let overrides = CliSettings {
            live_reload_enabled: Some(false),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert!(!config.live_reload.enabled);
    }

    #[test]
    fn test_apply_cli_settings_multiple() {
        let mut config = Config::default_with_base(Path::new("/test"));

        let overrides = CliSettings {
            host: Some("0.0.0.0".to_owned()),
            port: Some(9000),
            kroki_url: Some("https://kroki.io".to_owned()),
            live_reload_enabled: Some(false),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://kroki.io".to_owned())
        );
        assert!(!config.live_reload.enabled);
    }

    #[test]
    fn test_apply_cli_settings_empty() {
        let config_before = Config::default_with_base(Path::new("/test"));
        let mut config = Config::default_with_base(Path::new("/test"));

        config.apply_cli_settings(&CliSettings::default());

        assert_eq!(config.server.host, config_before.server.host);
        assert_eq!(config.server.port, config_before.server.port);
        assert_eq!(
            config.docs_resolved.source_dir,
            config_before.docs_resolved.source_dir
        );
    }

    #[test]
    fn test_expand_env_vars_server_host() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("TEST_HOST", "0.0.0.0");
        }

        let toml = r#"
[server]
host = "${TEST_HOST}"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.expand_env_vars().unwrap();

        assert_eq!(config.server.host, "0.0.0.0");

        unsafe {
            std::env::remove_var("TEST_HOST");
        }
    }

    #[test]
    fn test_expand_env_vars_diagrams_kroki_url() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("TEST_KROKI_URL", "https://kroki.test.com");
        }

        let toml = r#"
[diagrams]
kroki_url = "${TEST_KROKI_URL}"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.expand_env_vars().unwrap();

        assert_eq!(
            config.diagrams.as_ref().unwrap().kroki_url,
            Some("https://kroki.test.com".to_owned())
        );

        unsafe {
            std::env::remove_var("TEST_KROKI_URL");
        }
    }

    #[test]
    fn test_expand_env_vars_missing_required_var() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::remove_var("MISSING_VAR_CONFIG_TEST");
        }

        let toml = r#"
[diagrams]
kroki_url = "${MISSING_VAR_CONFIG_TEST}"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        let result = config.expand_env_vars();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_matches!(err, ConfigError::EnvVar { .. });
        assert!(err.to_string().contains("MISSING_VAR_CONFIG_TEST"));
        assert!(err.to_string().contains("diagrams.kroki_url"));
    }

    #[test]
    fn test_expand_env_vars_literal_unchanged() {
        let toml = r#"
[server]
host = "127.0.0.1"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.expand_env_vars().unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
    }

    // Validation tests

    /// Assert that validation fails with expected substrings in the error message.
    fn assert_validation_error(config: &Config, expected_substrings: &[&str]) {
        let result = config.validate();
        assert!(result.is_err(), "Expected validation to fail");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Validation(_)),
            "Expected ConfigError::Validation, got {err:?}"
        );
        let msg = err.to_string();
        for s in expected_substrings {
            assert!(
                msg.contains(s),
                "Expected error to contain '{s}', got: {msg}"
            );
        }
    }

    #[test]
    fn test_validate_default_config_passes() {
        let config = Config::default_with_base(Path::new("/test"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_server_host_empty() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.server.host = String::new();
        assert_validation_error(&config, &["server.host", "empty"]);
    }

    #[test]
    fn test_validate_server_port_zero() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.server.port = 0;
        assert_validation_error(&config, &["server.port"]);
    }

    #[test]
    fn test_validate_diagrams_kroki_url_empty() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.kroki_url = Some(String::new());
        assert_validation_error(&config, &["kroki_url", "empty"]);
    }

    #[test]
    fn test_validate_diagrams_kroki_url_invalid_scheme() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.kroki_url = Some("ftp://kroki.io".to_owned());
        assert_validation_error(&config, &["kroki_url", "http"]);
    }

    #[test]
    fn test_validate_diagrams_kroki_url_valid_http() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.kroki_url = Some("http://localhost:8000".to_owned());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_diagrams_kroki_url_valid_https() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.kroki_url = Some("https://kroki.io".to_owned());
        assert!(config.validate().is_ok());
    }
    #[test]
    fn test_env_var_fallback_no_config_no_flag() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "https://env.example") };

        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback();

        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://env.example".to_owned())
        );
    }

    #[test]
    fn test_env_var_fallback_unset_means_no_diagrams() {
        let _guard = EnvGuard::new();

        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback();

        assert!(config.diagrams_resolved.kroki_url.is_none());
    }

    #[test]
    fn test_env_var_fallback_empty_treated_as_unset() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "") };

        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback();

        assert!(config.diagrams_resolved.kroki_url.is_none());
    }

    #[test]
    fn test_env_var_fallback_does_not_override_existing_value() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "https://env.example") };

        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.kroki_url = Some("https://from-config.example".to_owned());
        config.apply_env_var_fallback();

        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://from-config.example".to_owned())
        );
    }

    #[test]
    fn test_load_uses_env_var_when_no_diagrams_in_toml() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "https://env.example") };

        let (_dir, toml_path) = rw_toml_tempdir("env-only", "");
        let config =
            Config::load(Some(&toml_path), None).expect("load should succeed with empty rw.toml");
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://env.example".to_owned())
        );
    }

    #[test]
    fn test_load_cli_flag_beats_env_var() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "https://env.example") };

        let (_dir, toml_path) = rw_toml_tempdir("cli-wins", "");
        let cli = CliSettings {
            kroki_url: Some("https://cli.example".to_owned()),
            ..Default::default()
        };
        let config = Config::load(Some(&toml_path), Some(&cli)).expect("load should succeed");
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://cli.example".to_owned())
        );
    }

    #[test]
    fn test_load_rejects_invalid_url_from_env() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "not-a-url") };

        let (_dir, toml_path) = rw_toml_tempdir("bad-env", "");
        let err = Config::load(Some(&toml_path), None)
            .expect_err("invalid env URL should fail validation");
        assert_matches!(err, ConfigError::Validation(_));
        let msg = err.to_string();
        assert!(msg.contains("kroki_url"), "got error: {msg}");
    }

    #[test]
    fn test_load_diagrams_section_without_kroki_url_no_env_loads_ok() {
        let _guard = EnvGuard::new();

        let (_dir, toml_path) =
            rw_toml_tempdir("diag-no-kroki", "[diagrams]\ninclude_dirs = [\"shared\"]\n");
        let config =
            Config::load(Some(&toml_path), None).expect("missing kroki_url should not error");
        assert!(config.diagrams_resolved.kroki_url.is_none());
        assert_eq!(config.diagrams_resolved.include_dirs.len(), 1);
    }

    #[test]
    fn test_load_diagrams_section_without_kroki_url_filled_by_env() {
        let _guard = EnvGuard::new();
        // SAFETY: serialized via RW_DIAGRAMS_KROKI_URL_GUARD.
        unsafe { std::env::set_var("RW_DIAGRAMS_KROKI_URL", "https://env.example") };

        let (_dir, toml_path) =
            rw_toml_tempdir("diag-env", "[diagrams]\ninclude_dirs = [\"shared\"]\n");
        let config = Config::load(Some(&toml_path), None).expect("env should fill kroki_url");
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://env.example".to_owned())
        );
        assert_eq!(config.diagrams_resolved.include_dirs.len(), 1);
    }

    #[test]
    fn project_dir_is_the_config_file_parent() {
        let (dir, toml_path) =
            rw_toml_tempdir("project-dir-parent", "[docs]\nsource_dir = \"docs\"\n");
        let config = Config::load_from_file(&toml_path).expect("load config");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs_resolved.source_dir, dir.path().join("docs"));
        assert_eq!(
            config.docs_resolved.data_dir,
            dir.path().join(DATA_DIR_NAME)
        );
    }

    #[test]
    fn project_dir_is_the_base_for_a_nested_source_dir() {
        let (dir, toml_path) =
            rw_toml_tempdir("project-dir-nested", "[docs]\nsource_dir = \"docs/site\"\n");
        let config = Config::load_from_file(&toml_path).expect("load config");

        // The project dir is the toml's parent, NOT the source dir's parent.
        assert_eq!(config.project_dir, dir.path());
        assert_eq!(
            config.docs_resolved.source_dir,
            dir.path().join("docs/site")
        );
    }

    #[test]
    fn project_dir_is_not_read_from_the_toml() {
        // Pins `#[serde(skip)]` on `project_dir` in isolation: parse with a bare
        // `toml::from_str::<Config>()`, NOT `Config::load_from_file`, which would
        // unconditionally overwrite `project_dir` from the path's parent
        // afterwards and pass regardless of the attribute.
        let toml = "project_dir = \"/definitely/elsewhere\"\n";
        let config: Config = toml::from_str(toml).unwrap();

        assert_eq!(config.project_dir, PathBuf::from("."));
    }

    #[test]
    fn load_from_dir_loads_the_toml_in_that_dir() {
        let _env = EnvGuard::new();
        let (dir, _toml_path) =
            rw_toml_tempdir("from-dir-present", "[docs]\nsource_dir = \"content\"\n");

        let config = Config::load_from_dir(dir.path(), None).expect("load from dir");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs_resolved.source_dir, dir.path().join("content"));
    }

    #[test]
    fn load_from_dir_uses_defaults_rooted_at_dir_when_no_toml() {
        let _env = EnvGuard::new();
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-from-dir-absent-")
            .tempdir()
            .expect("create tempdir");

        let config = Config::load_from_dir(dir.path(), None).expect("load from dir");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs_resolved.source_dir, dir.path().join("docs"));
        assert_eq!(
            config.docs_resolved.data_dir,
            dir.path().join(DATA_DIR_NAME)
        );
    }

    #[test]
    fn load_from_dir_does_not_walk_up_to_a_parent_toml() {
        let _env = EnvGuard::new();
        // A parent with an rw.toml, and a child without one. An embedding host
        // pointing at the child must not inherit config it does not own.
        let (parent, _toml_path) = rw_toml_tempdir(
            "from-dir-no-walk",
            "[docs]\nsource_dir = \"parent-content\"\n",
        );
        let child = parent.path().join("child");
        std::fs::create_dir(&child).expect("create child dir");

        let config = Config::load_from_dir(&child, None).expect("load from dir");

        assert_eq!(config.project_dir, child);
        assert_eq!(config.docs_resolved.source_dir, child.join("docs"));
    }

    #[test]
    fn load_from_dir_rejects_a_missing_directory() {
        let _env = EnvGuard::new();
        // A mistyped `--project-dir` (or `projectDir`) must fail loudly. Without
        // the check it resolves to defaults rooted at a directory that is not
        // there, and the caller then materializes it: `rw serve` creates
        // `<typo>/.rw/`, and `rw backstage publish` can push an empty bundle
        // over good published docs.
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-from-dir-missing-")
            .tempdir()
            .expect("create tempdir");

        let err = Config::load_from_dir(&dir.path().join("not-here"), None)
            .expect_err("a missing project directory must be rejected");

        assert_matches!(err, ConfigError::ProjectDirNotFound(_));
    }

    #[test]
    fn load_from_dir_rejects_a_path_that_is_not_a_directory() {
        let _env = EnvGuard::new();
        // Same class as the missing case: `dir.join("rw.toml")` under a *file*
        // is a path that cannot exist, so without the check this silently
        // succeeds with a nonsense root.
        let (dir, toml_path) = rw_toml_tempdir("from-dir-not-a-dir", "");

        let err = Config::load_from_dir(&toml_path, None)
            .expect_err("a file must be rejected as a project directory");

        assert_matches!(err, ConfigError::ProjectDirNotFound(_));
        drop(dir);
    }

    #[test]
    fn load_from_dir_applies_cli_settings_and_validates() {
        let _env = EnvGuard::new();
        let (dir, _toml_path) = rw_toml_tempdir("from-dir-cli", "[docs]\nsource_dir = \"docs\"\n");
        let settings = CliSettings {
            kroki_url: Some("https://kroki.example".to_owned()),
            ..CliSettings::default()
        };

        let config = Config::load_from_dir(dir.path(), Some(&settings)).expect("load from dir");

        assert_eq!(
            config.diagrams_resolved.kroki_url.as_deref(),
            Some("https://kroki.example")
        );

        // The other half of `finish`: a config that fails validation must not
        // load. Without this, dropping `self.validate()?` from `finish` would
        // leave this test green.
        let (bad_dir, _bad_toml) = rw_toml_tempdir("from-dir-invalid", "[server]\nport = 0\n");

        assert_matches!(
            Config::load_from_dir(bad_dir.path(), None),
            Err(ConfigError::Validation(_))
        );
    }

    #[test]
    fn include_dirs_resolve_against_project_dir() {
        let (dir, toml_path) = rw_toml_tempdir(
            "project-dir-includes",
            "[diagrams]\nkroki_url = \"https://kroki.io\"\ninclude_dirs = [\"puml\"]\n",
        );
        let config = Config::load_from_file(&toml_path).expect("load config");

        assert_eq!(
            config.diagrams_resolved.include_dirs,
            vec![dir.path().join("puml")]
        );
    }
}
