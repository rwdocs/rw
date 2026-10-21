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
    /// Kroki URL used when neither `kroki_url` nor `rw.toml` sets one.
    pub kroki_url_default: Option<String>,
    /// Disable live reload.
    pub no_live_reload: bool,
}

/// Configuration filename to search for.
const CONFIG_FILENAME: &str = "rw.toml";

/// Name of the per-project data directory. Everything RW writes for a project
/// — the cache and the comments DB — lives under it, so the resolved
/// `data_dir` is the single source for all of those paths.
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

    /// Directory the project is rooted at — the directory containing `rw.toml`,
    /// or, when no config file is involved, the directory the caller rooted the
    /// configuration at.
    ///
    /// This is the base the file's relative paths resolve against, so
    /// [`DocsConfig::source_dir`], [`DocsConfig::data_dir`], and
    /// [`DiagramsConfig::include_dirs`] are all derived from it. `source_dir`
    /// may still be absolute or climb out with `..`, so do not assume it sits
    /// inside `project_dir`.
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
}

/// Server configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Server host address.
    pub host: String,
    /// Server port, from `[server].port` or `-p`/`--port`.
    ///
    /// `rw serve` treats a set port as a hard requirement (fail if busy), but
    /// falls back from a busy [`Self::DEFAULT_PORT`] to the next free one.
    pub port: Option<u16>,
}

impl ServerConfig {
    /// Port `rw serve` listens on when none is set.
    pub const DEFAULT_PORT: u16 = 7979;
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: None,
        }
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
        self.server.port = settings.port.or(self.server.port);
        if settings.no_cache {
            self.docs.cache_enabled = false;
        }
        if let Some(kroki_url) = &settings.kroki_url {
            self.diagrams.kroki_url = Some(kroki_url.clone());
        }
        if self.diagrams.kroki_url.is_none() {
            self.diagrams
                .kroki_url
                .clone_from(&settings.kroki_url_default);
        }
        if settings.no_live_reload {
            self.live_reload.enabled = false;
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
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::default_with_base(&cwd)
    }

    /// Create default config with paths relative to given base directory.
    fn default_with_base(base: &Path) -> Self {
        RawConfig::default().resolve(base)
    }

    /// Load configuration from a specific file.
    fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut raw: RawConfig = toml::from_str(&content).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        raw.expand_env_vars(expand::env_lookup)?;

        Ok(raw.resolve(config_file_dir(path)))
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
        if self.server.port == Some(0) {
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

    /// Apply the caller's overrides, then validate.
    ///
    /// Shared by [`Self::load`] and [`Self::load_from_dir`], which differ only
    /// in how they choose a config source. Add new load-time steps here, not in
    /// the callers: a step added to one entry point alone is silently skipped
    /// by the other.
    fn finish(mut self, cli_settings: Option<&CliSettings>) -> Result<Self, ConfigError> {
        if let Some(settings) = cli_settings {
            self.apply_cli_settings(settings);
        }
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
            project_dir: project_dir.to_path_buf(),
        }
    }
}

/// The directory holding the config file at `path`. A bare file name's
/// parent is `""`, which `canonicalize` rejects.
fn config_file_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::error::Error;

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
        assert_eq!(config.server.port, None);
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
        assert_eq!(config.server.port, None);
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
        assert_eq!(config.server.port, Some(9000));
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
    fn test_apply_cli_settings_overrides_only_given_fields() {
        #[derive(Debug, Clone, PartialEq)]
        struct Observed {
            host: String,
            port: Option<u16>,
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
                cache_enabled: config.docs.cache_enabled,
                kroki_url: config.diagrams.kroki_url,
                live_reload_enabled: config.live_reload.enabled,
            }
        }

        let defaults = Observed {
            host: "127.0.0.1".to_owned(),
            port: None,
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
                    port: Some(9000),
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
        config.server.port = Some(0);
        assert_validation_error(&config, &["server.port"]);
    }

    #[test]
    fn test_validate_diagrams_kroki_url_invalid_scheme() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams.kroki_url = Some("ftp://kroki.io".to_owned());
        assert_validation_error(&config, &["kroki_url", "http"]);
    }

    #[test]
    fn test_kroki_url_default_is_the_lowest_priority_source() {
        fn kroki_url(from_toml: Option<&str>, from_flag: Option<&str>) -> Option<String> {
            let toml = from_toml.map_or(String::new(), |url| {
                format!("[diagrams]\nkroki_url = \"{url}\"\n")
            });
            let (_dir, toml_path) = rw_toml_tempdir("kroki-default", &toml);
            let settings = CliSettings {
                kroki_url: from_flag.map(str::to_owned),
                kroki_url_default: Some("https://default.example".to_owned()),
                ..Default::default()
            };
            Config::load(Some(&toml_path), Some(&settings))
                .expect("load")
                .diagrams
                .kroki_url
        }

        let toml = Some("https://toml.example");
        let flag = Some("https://flag.example");
        assert_eq!(
            kroki_url(None, None).as_deref(),
            Some("https://default.example")
        );
        assert_eq!(kroki_url(toml, None).as_deref(), toml);
        assert_eq!(kroki_url(toml, flag).as_deref(), flag);
    }

    #[test]
    fn test_load_rejects_a_missing_config_path() {
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

        let err = Config::load_from_file(dir.path())
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

        let err =
            Config::load_from_file(&toml_path).expect_err("a mistyped value must be rejected");

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
        let config = Config::load_from_file(&toml_path).expect("load config");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("docs"));
        assert_eq!(config.docs.data_dir, dir.path().join(DATA_DIR_NAME));
    }

    #[test]
    fn a_bare_config_file_name_roots_the_project_at_the_working_directory() {
        assert_eq!(config_file_dir(Path::new("rw.toml")), Path::new("."));
        assert_eq!(config_file_dir(Path::new("sub/rw.toml")), Path::new("sub"));
    }

    #[test]
    fn project_dir_is_the_base_for_a_nested_source_dir() {
        let (dir, toml_path) =
            rw_toml_tempdir("project-dir-nested", "[docs]\nsource_dir = \"docs/site\"\n");
        let config = Config::load_from_file(&toml_path).expect("load config");

        // The project dir is the toml's parent, NOT the source dir's parent.
        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("docs/site"));
    }

    #[test]
    fn load_from_dir_loads_the_toml_in_that_dir() {
        let (dir, _toml_path) =
            rw_toml_tempdir("from-dir-present", "[docs]\nsource_dir = \"content\"\n");

        let config = Config::load_from_dir(dir.path(), None).expect("load from dir");

        assert_eq!(config.project_dir, dir.path());
        assert_eq!(config.docs.source_dir, dir.path().join("content"));
    }

    #[test]
    fn load_from_dir_uses_defaults_rooted_at_dir_when_no_toml() {
        let dir = tempfile::Builder::new()
            .prefix("rw-config-test-from-dir-absent-")
            .tempdir()
            .expect("create tempdir");

        let config = Config::load_from_dir(dir.path(), None).expect("load from dir");

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

        let config = Config::load_from_dir(&child, None).expect("load from dir");

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

        let config = Config::load_from_dir(dir.path(), Some(&settings)).expect("load from dir");

        assert_eq!(
            config.diagrams.kroki_url.as_deref(),
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

        assert_eq!(config.diagrams.include_dirs, vec![dir.path().join("puml")]);
    }
}
