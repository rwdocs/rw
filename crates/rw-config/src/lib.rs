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
/// The default leaves the loaded config untouched.
#[derive(Debug, Default)]
pub struct CliSettings {
    /// Override server host.
    pub host: Option<String>,
    /// Override server port.
    pub port: Option<u16>,
    /// Disable caching.
    pub no_cache: bool,
    /// Override Kroki URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Disable live reload.
    pub no_live_reload: bool,
}

/// Configuration filename to search for.
const CONFIG_FILENAME: &str = "rw.toml";

/// Name of the per-project data directory. Everything RW writes for a project
/// — the cache, the comments DB, and the server-info file — lives under it, so
/// the resolved `data_dir` is the single source for all of those paths.
const DATA_DIR_NAME: &str = ".rw";

/// Application configuration.
#[derive(Debug)]
pub struct Config {
    /// Server configuration.
    pub server: ServerConfig,
    /// Documentation configuration.
    pub docs: DocsConfig,
    /// Diagram rendering configuration.
    pub diagrams: DiagramsConfig,
    /// Live reload configuration.
    pub live_reload: LiveReloadConfig,
    /// Metadata configuration.
    pub metadata: MetadataConfig,

    /// Directory the project is rooted at — the directory containing `rw.toml`,
    /// or, when no config file is involved, the directory the caller rooted the
    /// configuration at.
    ///
    /// This is the base the file's relative paths resolve against, so
    /// [`DocsConfig::source_dir`], [`DocsConfig::data_dir`], and
    /// [`DiagramsConfig::include_dirs`] are all derived from it. A CLI override
    /// breaks that relationship in one direction: `source_dir` can be replaced
    /// outright without moving the project root, so it is not safe to assume
    /// `source_dir` sits inside `project_dir`.
    ///
    /// Stored as supplied, not canonicalized: a relative path in yields a
    /// relative project dir out, matching the paths derived from it.
    pub project_dir: PathBuf,
}

/// `rw.toml` as written: relative paths, unexpanded variables.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    server: ServerConfig,
    docs: DocsConfigRaw,
    diagrams: DiagramsConfigRaw,
    live_reload: LiveReloadConfig,
    metadata: MetadataConfig,
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
#[derive(Deserialize, Default)]
#[serde(default)]
struct DocsConfigRaw {
    source_dir: Option<String>,
    cache_enabled: Option<bool>,
}

/// Resolved documentation configuration with absolute paths.
#[derive(Debug)]
pub struct DocsConfig {
    /// Source directory for markdown files.
    pub source_dir: PathBuf,
    /// The project's data directory, `.rw` under the project root.
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
#[derive(Deserialize, Default)]
#[serde(default)]
struct DiagramsConfigRaw {
    kroki_url: Option<String>,
    include_dirs: Vec<String>,
}

/// Resolved diagram rendering configuration with absolute paths.
#[derive(Debug)]
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
    /// Config file exists but cannot be read (e.g. it is a directory).
    #[error("cannot read {}", path.display())]
    Read {
        /// Config file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Config file is not valid TOML or does not match the schema.
    #[error("invalid {}", path.display())]
    Parse {
        /// Config file path.
        path: PathBuf,
        /// Underlying TOML error, carrying the line and column.
        #[source]
        source: toml::de::Error,
    },
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
        Self::load_with(config_path, cli_settings, expand::env_lookup)
    }

    /// [`Self::load`], resolving environment variables through `lookup`.
    fn load_with(
        config_path: Option<&Path>,
        cli_settings: Option<&CliSettings>,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let config = if let Some(path) = config_path {
            if !path.exists() {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            Self::load_from_file(path, &lookup)?
        } else if let Some(discovered) = Self::discover_config() {
            Self::load_from_file(&discovered, &lookup)?
        } else {
            Self::default_with_cwd()
        };

        config.finish(cli_settings, lookup)
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
        Self::load_from_dir_with(dir, cli_settings, expand::env_lookup)
    }

    /// [`Self::load_from_dir`], resolving environment variables through
    /// `lookup`.
    fn load_from_dir_with(
        dir: &Path,
        cli_settings: Option<&CliSettings>,
        lookup: impl Fn(&str) -> Option<String>,
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
            Self::load_from_file(&config_path, &lookup)?
        } else {
            Self::default_with_base(dir)
        };

        config.finish(cli_settings, lookup)
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
        if settings.no_cache {
            self.docs.cache_enabled = false;
        }
        if let Some(kroki_url) = &settings.kroki_url {
            self.diagrams.kroki_url = Some(kroki_url.clone());
        }
        if settings.no_live_reload {
            self.live_reload.enabled = false;
        }
    }

    /// Lowest-priority source for `diagrams.kroki_url`. Empty
    /// `RW_DIAGRAMS_KROKI_URL` is treated as unset, matching how shells export
    /// cleared variables.
    fn apply_env_var_fallback(&mut self, lookup: impl Fn(&str) -> Option<String>) {
        const ENV_VAR: &str = "RW_DIAGRAMS_KROKI_URL";

        if self.diagrams.kroki_url.is_some() {
            return;
        }
        if let Some(value) = lookup(ENV_VAR)
            && !value.is_empty()
        {
            self.diagrams.kroki_url = Some(value);
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
        RawConfig::default().resolve(base)
    }

    /// Load configuration from a specific file.
    fn load_from_file(
        path: &Path,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut raw: RawConfig = toml::from_str(&content).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        raw.expand_env_vars(lookup)?;

        Ok(raw.resolve(path.parent().unwrap_or(Path::new("."))))
    }

    /// Validate configuration values.
    ///
    /// Checks that all required fields are properly set and contain valid values.
    /// Called automatically after loading from file.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::Validation` if any validation fails.
    fn validate(&self) -> Result<(), ConfigError> {
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
        if let Some(ref kroki_url) = self.diagrams.kroki_url {
            require_http_url(kroki_url, "diagrams.kroki_url")?;
        }

        Ok(())
    }

    /// Apply the caller's overrides and the env-var fallback, then validate.
    ///
    /// Shared by [`Self::load`] and [`Self::load_from_dir`], which differ only
    /// in how they choose a config source. Add new load-time steps here, not in
    /// the callers: a step added to one entry point alone is silently skipped
    /// by the other.
    fn finish(
        mut self,
        cli_settings: Option<&CliSettings>,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        if let Some(settings) = cli_settings {
            self.apply_cli_settings(settings);
        }
        self.apply_env_var_fallback(lookup);
        self.validate()?;
        Ok(self)
    }
}

impl RawConfig {
    /// Expand variable references in configuration strings, resolving names
    /// through `lookup`.
    fn expand_env_vars(
        &mut self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<(), ConfigError> {
        self.server.host = expand::expand_vars(&self.server.host, "server.host", &lookup)?;

        if let Some(ref url) = self.diagrams.kroki_url {
            self.diagrams.kroki_url =
                Some(expand::expand_vars(url, "diagrams.kroki_url", &lookup)?);
        }

        Ok(())
    }

    /// Resolve relative paths against `project_dir`. Does not validate:
    /// [`Config::validate`] runs only after CLI overrides are applied.
    fn resolve(self, project_dir: &Path) -> Config {
        Config {
            server: self.server,
            docs: DocsConfig {
                source_dir: project_dir.join(self.docs.source_dir.as_deref().unwrap_or("docs")),
                data_dir: project_dir.join(DATA_DIR_NAME),
                cache_enabled: self.docs.cache_enabled.unwrap_or(true),
            },
            diagrams: DiagramsConfig {
                kroki_url: self.diagrams.kroki_url,
                include_dirs: self
                    .diagrams
                    .include_dirs
                    .iter()
                    .map(|d| project_dir.join(d))
                    .collect(),
            },
            live_reload: self.live_reload,
            metadata: self.metadata,
            project_dir: project_dir.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::error::Error;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    /// An environment holding only `RW_DIAGRAMS_KROKI_URL`.
    fn kroki_env(value: &str) -> impl Fn(&str) -> Option<String> {
        let value = value.to_owned();
        move |name| (name == "RW_DIAGRAMS_KROKI_URL").then(|| value.clone())
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
        assert_eq!(config.docs.source_dir, PathBuf::from("/test/docs"));
        assert_eq!(config.docs.data_dir, PathBuf::from("/test/.rw"));
        assert_eq!(config.docs.cache_dir(), PathBuf::from("/test/.rw/cache"));
        assert!(config.docs.cache_enabled);
        assert!(config.live_reload.enabled);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = "";
        let config: RawConfig = toml::from_str(toml).unwrap();
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
        let config: RawConfig = toml::from_str(toml).unwrap();
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
        let from_empty: RawConfig = toml::from_str("").unwrap();
        assert!(!from_empty.server.port_explicit);
        // `[server]` present but without a `port` key is likewise not explicit.
        let host_only: RawConfig = toml::from_str("[server]\nhost = \"0.0.0.0\"\n").unwrap();
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
        let config: RawConfig = toml::from_str(toml).unwrap();
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
        let config = toml::from_str::<RawConfig>(toml)
            .unwrap()
            .resolve(Path::new("/project"));

        assert_eq!(
            config.docs.source_dir,
            PathBuf::from("/project/documentation")
        );
        assert_eq!(config.docs.data_dir, PathBuf::from("/project/.rw"));
        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://kroki.io".to_owned())
        );
        assert_eq!(
            config.diagrams.include_dirs,
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
        let config = toml::from_str::<RawConfig>(toml)
            .unwrap()
            .resolve(Path::new("/test"));
        assert!(config.diagrams.kroki_url.is_none());
        assert_eq!(config.diagrams.include_dirs.len(), 1);
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
        let config = toml::from_str::<RawConfig>(toml)
            .expect("stale [confluence] should be silently ignored")
            .resolve(Path::new("/test"));

        // The stale section must NOT leak into resolved state.
        assert!(config.diagrams.kroki_url.is_none());
        assert_eq!(config.diagrams.include_dirs, [] as [PathBuf; 0]);
        assert_eq!(config.docs.source_dir, PathBuf::from("/test/docs"));
    }

    #[test]
    fn test_no_diagrams_section_is_valid() {
        let toml = r#"
[docs]
source_dir = "documentation"
"#;
        let config = toml::from_str::<RawConfig>(toml)
            .unwrap()
            .resolve(Path::new("/project"));

        assert!(config.diagrams.kroki_url.is_none());
        assert_eq!(config.diagrams.include_dirs, [] as [PathBuf; 0]);
    }

    #[test]
    fn test_apply_cli_settings_overrides_only_given_fields() {
        #[derive(Debug, Clone, PartialEq)]
        struct Observed {
            host: String,
            port: u16,
            port_explicit: bool,
            cache_enabled: bool,
            kroki_url: Option<String>,
            live_reload_enabled: bool,
        }

        fn apply(settings: &CliSettings) -> Observed {
            let mut config = Config::default_with_base(Path::new("/test"));
            config.apply_cli_settings(settings);
            Observed {
                host: config.server.host,
                port: config.server.port,
                port_explicit: config.server.port_explicit,
                cache_enabled: config.docs.cache_enabled,
                kroki_url: config.diagrams.kroki_url,
                live_reload_enabled: config.live_reload.enabled,
            }
        }

        let defaults = Observed {
            host: "127.0.0.1".to_owned(),
            port: 7979,
            port_explicit: false,
            cache_enabled: true,
            kroki_url: None,
            live_reload_enabled: true,
        };
        let cases = [
            (CliSettings::default(), defaults.clone()),
            (
                CliSettings {
                    host: Some("0.0.0.0".to_owned()),
                    ..Default::default()
                },
                Observed {
                    host: "0.0.0.0".to_owned(),
                    ..defaults.clone()
                },
            ),
            (
                CliSettings {
                    port: Some(9000),
                    ..Default::default()
                },
                Observed {
                    port: 9000,
                    port_explicit: true,
                    ..defaults.clone()
                },
            ),
            (
                CliSettings {
                    no_cache: true,
                    ..Default::default()
                },
                Observed {
                    cache_enabled: false,
                    ..defaults.clone()
                },
            ),
            (
                CliSettings {
                    kroki_url: Some("https://kroki.example.com".to_owned()),
                    ..Default::default()
                },
                Observed {
                    kroki_url: Some("https://kroki.example.com".to_owned()),
                    ..defaults.clone()
                },
            ),
            (
                CliSettings {
                    no_live_reload: true,
                    ..Default::default()
                },
                Observed {
                    live_reload_enabled: false,
                    ..defaults
                },
            ),
        ];

        for (settings, expected) in cases {
            assert_eq!(apply(&settings), expected, "{settings:?}");
        }
    }

    #[test]
    fn test_expand_env_vars_server_host() {
        let toml = r#"
[server]
host = "${TEST_HOST}"
"#;
        let mut config: RawConfig = toml::from_str(toml).unwrap();
        config
            .expand_env_vars(|name| (name == "TEST_HOST").then(|| "0.0.0.0".to_owned()))
            .unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
    }

    #[test]
    fn test_expand_env_vars_diagrams_kroki_url() {
        let toml = r#"
[diagrams]
kroki_url = "${TEST_KROKI_URL}"
"#;
        let mut config: RawConfig = toml::from_str(toml).unwrap();
        config
            .expand_env_vars(|name| {
                (name == "TEST_KROKI_URL").then(|| "https://kroki.test.com".to_owned())
            })
            .unwrap();

        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://kroki.test.com".to_owned())
        );
    }

    #[test]
    fn test_expand_env_vars_missing_required_var() {
        let toml = r#"
[diagrams]
kroki_url = "${MISSING_VAR_CONFIG_TEST}"
"#;
        let mut config: RawConfig = toml::from_str(toml).unwrap();
        let result = config.expand_env_vars(|_| None);

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
        let mut config: RawConfig = toml::from_str(toml).unwrap();
        config.expand_env_vars(|_| None).unwrap();

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
        config.diagrams.kroki_url = Some(String::new());
        assert_validation_error(&config, &["kroki_url", "http"]);
    }

    #[test]
    fn test_validate_diagrams_kroki_url_invalid_scheme() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams.kroki_url = Some("ftp://kroki.io".to_owned());
        assert_validation_error(&config, &["kroki_url", "http"]);
    }

    #[test]
    fn test_env_var_fallback_no_config_no_flag() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback(kroki_env("https://env.example"));

        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://env.example".to_owned())
        );
    }

    #[test]
    fn test_env_var_fallback_unset_means_no_diagrams() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback(no_env);

        assert!(config.diagrams.kroki_url.is_none());
    }

    #[test]
    fn test_env_var_fallback_empty_treated_as_unset() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.apply_env_var_fallback(kroki_env(""));

        assert!(config.diagrams.kroki_url.is_none());
    }

    #[test]
    fn test_env_var_fallback_does_not_override_existing_value() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams.kroki_url = Some("https://from-config.example".to_owned());
        config.apply_env_var_fallback(kroki_env("https://env.example"));

        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://from-config.example".to_owned())
        );
    }

    #[test]
    fn test_load_uses_env_var_when_no_diagrams_in_toml() {
        let (_dir, toml_path) = rw_toml_tempdir("env-only", "");
        let config = Config::load_with(Some(&toml_path), None, kroki_env("https://env.example"))
            .expect("load should succeed with empty rw.toml");
        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://env.example".to_owned())
        );
    }

    #[test]
    fn test_load_cli_flag_beats_env_var() {
        let (_dir, toml_path) = rw_toml_tempdir("cli-wins", "");
        let cli = CliSettings {
            kroki_url: Some("https://cli.example".to_owned()),
            ..Default::default()
        };
        let config = Config::load_with(
            Some(&toml_path),
            Some(&cli),
            kroki_env("https://env.example"),
        )
        .expect("load should succeed");
        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://cli.example".to_owned())
        );
    }

    #[test]
    fn test_load_rejects_invalid_url_from_env() {
        let (_dir, toml_path) = rw_toml_tempdir("bad-env", "");
        let err = Config::load_with(Some(&toml_path), None, kroki_env("not-a-url"))
            .expect_err("invalid env URL should fail validation");
        assert_matches!(err, ConfigError::Validation(_));
        let msg = err.to_string();
        assert!(msg.contains("kroki_url"), "got error: {msg}");
    }

    #[test]
    fn test_load_diagrams_section_without_kroki_url_no_env_loads_ok() {
        let (_dir, toml_path) =
            rw_toml_tempdir("diag-no-kroki", "[diagrams]\ninclude_dirs = [\"shared\"]\n");
        let config = Config::load_with(Some(&toml_path), None, no_env)
            .expect("missing kroki_url should not error");
        assert!(config.diagrams.kroki_url.is_none());
        assert_eq!(config.diagrams.include_dirs.len(), 1);
    }

    #[test]
    fn test_load_diagrams_section_without_kroki_url_filled_by_env() {
        let (_dir, toml_path) =
            rw_toml_tempdir("diag-env", "[diagrams]\ninclude_dirs = [\"shared\"]\n");
        let config = Config::load_with(Some(&toml_path), None, kroki_env("https://env.example"))
            .expect("env should fill kroki_url");
        assert_eq!(
            config.diagrams.kroki_url,
            Some("https://env.example".to_owned())
        );
        assert_eq!(config.diagrams.include_dirs.len(), 1);
    }

    #[test]
    fn test_load_rejects_a_missing_config_path() {
        // Rejected before the environment is read, so the public entry point
        // is hermetic here.
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-missing-path-")
            .tempdir()
            .expect("create tempdir");

        let err = Config::load(Some(&dir.path().join("rw.toml")), None)
            .expect_err("a missing config path must be rejected");

        assert_matches!(err, ConfigError::NotFound(_));
    }

    #[test]
    fn an_unreadable_config_path_names_the_path() {
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-dir-as-config-")
            .tempdir()
            .expect("create tempdir");

        let err = Config::load_from_file(dir.path(), no_env)
            .expect_err("a directory must not read as a config file");

        assert_eq!(
            err.to_string(),
            format!("cannot read {}", dir.path().display())
        );
        assert_matches!(err, ConfigError::Read { ref path, .. } if path == dir.path());
        assert!(err.source().is_some(), "the I/O cause must stay reachable");
    }

    #[test]
    fn an_invalid_config_file_names_the_file_and_keeps_the_toml_detail() {
        let (dir, toml_path) = rw_toml_tempdir("invalid-toml", "[server]\nport = \"x\"\n");

        let err = Config::load_from_file(&toml_path, no_env)
            .expect_err("a mistyped value must be rejected");

        assert_eq!(err.to_string(), format!("invalid {}", toml_path.display()));
        let cause = err
            .source()
            .expect("the TOML error is the cause")
            .to_string();
        assert!(cause.contains("line 2, column 8"), "{cause}");
        drop(dir);
    }

    #[test]
    fn project_dir_is_the_config_file_parent() {
        let (dir, toml_path) =
            rw_toml_tempdir("project-dir-parent", "[docs]\nsource_dir = \"docs\"\n");
        let config = Config::load_from_file(&toml_path, no_env).expect("load config");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("docs"));
        assert_eq!(config.docs.data_dir, dir.path().join(DATA_DIR_NAME));
    }

    #[test]
    fn project_dir_is_the_base_for_a_nested_source_dir() {
        let (dir, toml_path) =
            rw_toml_tempdir("project-dir-nested", "[docs]\nsource_dir = \"docs/site\"\n");
        let config = Config::load_from_file(&toml_path, no_env).expect("load config");

        // The project dir is the toml's parent, NOT the source dir's parent.
        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("docs/site"));
    }

    #[test]
    fn load_from_dir_loads_the_toml_in_that_dir() {
        let (dir, _toml_path) =
            rw_toml_tempdir("from-dir-present", "[docs]\nsource_dir = \"content\"\n");

        let config = Config::load_from_dir_with(dir.path(), None, no_env).expect("load from dir");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("content"));
    }

    #[test]
    fn load_from_dir_uses_defaults_rooted_at_dir_when_no_toml() {
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-from-dir-absent-")
            .tempdir()
            .expect("create tempdir");

        let config = Config::load_from_dir_with(dir.path(), None, no_env).expect("load from dir");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("docs"));
        assert_eq!(config.docs.data_dir, dir.path().join(DATA_DIR_NAME));
    }

    #[test]
    fn load_from_dir_does_not_walk_up_to_a_parent_toml() {
        // A parent with an rw.toml, and a child without one. An embedding host
        // pointing at the child must not inherit config it does not own.
        let (parent, _toml_path) = rw_toml_tempdir(
            "from-dir-no-walk",
            "[docs]\nsource_dir = \"parent-content\"\n",
        );
        let child = parent.path().join("child");
        std::fs::create_dir(&child).expect("create child dir");

        let config = Config::load_from_dir_with(&child, None, no_env).expect("load from dir");

        assert_eq!(config.project_dir, child);
        assert_eq!(config.docs.source_dir, child.join("docs"));
    }

    #[test]
    fn load_from_dir_rejects_a_missing_directory() {
        // A mistyped `--project-dir` (or `projectDir`) must fail loudly. Without
        // the check it resolves to defaults rooted at a directory that is not
        // there, and the caller then materializes it: `rw serve` creates
        // `<typo>/.rw/`, and `rw backstage publish` can push an empty bundle
        // over good published docs.
        //
        // Rejected before the environment is read, so the public entry point
        // is hermetic here and in the not-a-directory case.
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
        let (dir, _toml_path) = rw_toml_tempdir("from-dir-cli", "[docs]\nsource_dir = \"docs\"\n");
        let settings = CliSettings {
            kroki_url: Some("https://kroki.example".to_owned()),
            ..CliSettings::default()
        };

        let config =
            Config::load_from_dir_with(dir.path(), Some(&settings), no_env).expect("load from dir");

        assert_eq!(
            config.diagrams.kroki_url.as_deref(),
            Some("https://kroki.example")
        );

        // The other half of `finish`: a config that fails validation must not
        // load. Without this, dropping `self.validate()?` from `finish` would
        // leave this test green.
        let (bad_dir, _bad_toml) = rw_toml_tempdir("from-dir-invalid", "[server]\nport = 0\n");

        assert_matches!(
            Config::load_from_dir_with(bad_dir.path(), None, no_env),
            Err(ConfigError::Validation(_))
        );
    }

    #[test]
    fn include_dirs_resolve_against_project_dir() {
        let (dir, toml_path) = rw_toml_tempdir(
            "project-dir-includes",
            "[diagrams]\nkroki_url = \"https://kroki.io\"\ninclude_dirs = [\"puml\"]\n",
        );
        let config = Config::load_from_file(&toml_path, no_env).expect("load config");

        assert_eq!(config.diagrams.include_dirs, vec![dir.path().join("puml")]);
    }
}
