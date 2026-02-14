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
//! - `confluence.base_url`
//! - `confluence.access_token`
//! - `confluence.access_secret`
//! - `confluence.consumer_key`
//! - `diagrams.kroki_url`

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
    /// Override docs source directory.
    pub source_dir: Option<PathBuf>,
    /// Override cache enabled flag.
    pub cache_enabled: Option<bool>,
    /// Override Kroki URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Override live reload enabled flag.
    pub live_reload_enabled: Option<bool>,
}

/// Configuration filename to search for.
const CONFIG_FILENAME: &str = "rw.toml";

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
    /// When present, `kroki_url` is required.
    diagrams: Option<DiagramsConfigRaw>,
    /// Live reload configuration.
    pub live_reload: LiveReloadConfig,
    /// Metadata configuration.
    pub metadata: MetadataConfig,
    /// Confluence configuration.
    pub confluence: Option<ConfluenceConfig>,

    /// Resolved docs configuration (set after loading).
    #[serde(skip)]
    pub docs_resolved: DocsConfig,
    /// Resolved diagrams configuration (set after loading).
    #[serde(skip)]
    pub diagrams_resolved: DiagramsConfig,
    /// Path to the config file (set after loading).
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
}

impl Default for Config {
    #[allow(clippy::derivable_impls)]
    fn default() -> Self {
        Self::default_with_base(Path::new("."))
    }
}

/// Server configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Server host address.
    pub host: String,
    /// Server port.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 7979,
        }
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
    /// Project directory for rw data (.rw/).
    pub project_dir: PathBuf,
    /// Whether caching is enabled.
    pub cache_enabled: bool,
}

impl DocsConfig {
    /// Cache directory path (.rw/cache/).
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.project_dir.join("cache")
    }
}

/// Raw diagrams configuration as parsed from TOML (paths as strings).
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DiagramsConfigRaw {
    kroki_url: Option<String>,
    include_dirs: Option<Vec<String>>,
    dpi: Option<u32>,
}

/// Resolved diagram rendering configuration with absolute paths.
#[derive(Debug)]
pub struct DiagramsConfig {
    /// Kroki server URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` directives.
    pub include_dirs: Vec<PathBuf>,
    /// DPI for diagram rendering.
    pub dpi: u32,
}

impl Default for DiagramsConfig {
    fn default() -> Self {
        Self {
            kroki_url: None,
            include_dirs: Vec::new(),
            dpi: 192,
        }
    }
}

/// Live reload configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LiveReloadConfig {
    /// Whether live reload is enabled.
    pub enabled: bool,
    /// File patterns to watch for changes.
    pub watch_patterns: Option<Vec<String>>,
}

impl Default for LiveReloadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            watch_patterns: None,
        }
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

/// Confluence configuration.
#[derive(Debug, Deserialize)]
pub struct ConfluenceConfig {
    /// Confluence server base URL.
    pub base_url: String,
    /// OAuth access token.
    pub access_token: String,
    /// OAuth access token secret.
    pub access_secret: String,
    /// OAuth consumer key.
    #[serde(default = "default_consumer_key")]
    pub consumer_key: String,
}

impl ConfluenceConfig {
    /// Validate that all required fields are properly set.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::Validation` if any field is empty or has invalid format.
    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty(&self.base_url, "confluence.base_url")?;
        require_http_url(&self.base_url, "confluence.base_url")?;
        require_non_empty(&self.access_token, "confluence.access_token")?;
        require_non_empty(&self.access_secret, "confluence.access_secret")?;
        require_non_empty(&self.consumer_key, "confluence.consumer_key")?;
        Ok(())
    }
}

fn default_consumer_key() -> String {
    "rw".to_owned()
}

/// Configuration error.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// File not found.
    #[error("Configuration file not found: {}", .0.display())]
    NotFound(PathBuf),
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
        /// Config field path (e.g., "`confluence.access_token`").
        field: String,
        /// Error message (e.g., "${`CONFLUENCE_TOKEN`} not set").
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
        let mut config = if let Some(path) = config_path {
            if !path.exists() {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            Self::load_from_file(path)?
        } else if let Some(discovered) = Self::discover_config() {
            Self::load_from_file(&discovered)?
        } else {
            Self::default_with_cwd()
        };

        if let Some(settings) = cli_settings {
            config.apply_cli_settings(settings);
        }

        Ok(config)
    }

    /// Apply CLI settings to the configuration.
    fn apply_cli_settings(&mut self, settings: &CliSettings) {
        if let Some(host) = &settings.host {
            self.server.host.clone_from(host);
        }
        if let Some(port) = settings.port {
            self.server.port = port;
        }
        if let Some(source_dir) = &settings.source_dir {
            self.docs_resolved.source_dir.clone_from(source_dir);
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

    /// Get validated Confluence configuration.
    ///
    /// Returns the Confluence config if the `[confluence]` section is present
    /// and all fields are valid. Use this instead of accessing the `confluence`
    /// field directly when the command requires Confluence.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::Validation` if the section is missing or invalid.
    pub fn require_confluence(&self) -> Result<&ConfluenceConfig, ConfigError> {
        let conf = self.confluence.as_ref().ok_or_else(|| {
            ConfigError::Validation("[confluence] section required in config".into())
        })?;
        conf.validate()?;
        Ok(conf)
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
            confluence: None,
            docs_resolved: DocsConfig {
                source_dir: base.join("docs"),
                project_dir: base.join(".rw"),
                cache_enabled: true,
            },
            diagrams_resolved: DiagramsConfig::default(),
            config_path: None,
        }
    }

    /// Load configuration from a specific file.
    fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&content)?;

        // Expand environment variables before path resolution
        config.expand_env_vars()?;

        let config_dir = path.parent().unwrap_or(Path::new("."));
        config.resolve_paths(config_dir)?;
        config.config_path = Some(path.to_path_buf());

        // Validate configuration after loading and resolution
        config.validate()?;

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
        const MAX_DPI: u32 = 1000;

        // Only validate kroki_url if set (diagram rendering enabled)
        if let Some(ref kroki_url) = self.diagrams_resolved.kroki_url {
            require_non_empty(kroki_url, "diagrams.kroki_url")?;
            require_http_url(kroki_url, "diagrams.kroki_url")?;
        }

        // DPI validation: must be positive and reasonable
        let dpi = self.diagrams_resolved.dpi;
        if dpi == 0 {
            return Err(ConfigError::Validation(
                "diagrams.dpi must be greater than 0".to_owned(),
            ));
        }
        if dpi > MAX_DPI {
            return Err(ConfigError::Validation(format!(
                "diagrams.dpi cannot exceed {MAX_DPI}"
            )));
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

        // Confluence config (if present)
        if let Some(ref mut confluence) = self.confluence {
            confluence.base_url = expand::expand_env(&confluence.base_url, "confluence.base_url")?;
            confluence.access_token =
                expand::expand_env(&confluence.access_token, "confluence.access_token")?;
            confluence.access_secret =
                expand::expand_env(&confluence.access_secret, "confluence.access_secret")?;
            confluence.consumer_key =
                expand::expand_env(&confluence.consumer_key, "confluence.consumer_key")?;
        }

        Ok(())
    }

    /// Resolve relative paths to absolute paths based on config directory.
    ///
    /// Validates that `kroki_url` is provided when `[diagrams]` section exists.
    fn resolve_paths(&mut self, config_dir: &Path) -> Result<(), ConfigError> {
        let resolve = |path: Option<&str>, default: &str| config_dir.join(path.unwrap_or(default));

        self.docs_resolved = DocsConfig {
            source_dir: resolve(self.docs.source_dir.as_deref(), "docs"),
            project_dir: config_dir.join(".rw"),
            cache_enabled: self.docs.cache_enabled.unwrap_or(true),
        };

        self.diagrams_resolved = match &self.diagrams {
            Some(diagrams) => {
                let kroki_url = diagrams.kroki_url.clone().ok_or_else(|| {
                    ConfigError::Validation(
                        "[diagrams] section requires kroki_url to be set".to_owned(),
                    )
                })?;
                let include_dirs = diagrams
                    .include_dirs
                    .iter()
                    .flatten()
                    .map(|d| config_dir.join(d))
                    .collect();
                DiagramsConfig {
                    kroki_url: Some(kroki_url),
                    include_dirs,
                    dpi: diagrams.dpi.unwrap_or(192),
                }
            }
            None => DiagramsConfig::default(),
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_with_base(Path::new("/test"));
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 7979);
        assert_eq!(config.docs_resolved.source_dir, PathBuf::from("/test/docs"));
        assert_eq!(config.docs_resolved.project_dir, PathBuf::from("/test/.rw"));
        assert_eq!(
            config.docs_resolved.cache_dir(),
            PathBuf::from("/test/.rw/cache")
        );
        assert!(config.docs_resolved.cache_enabled);
        assert_eq!(config.diagrams_resolved.dpi, 192);
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
    }

    #[test]
    fn test_parse_confluence_config() {
        let toml = r#"
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
consumer_key = "myapp"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let confluence = config.confluence.unwrap();
        assert_eq!(confluence.base_url, "https://confluence.example.com");
        assert_eq!(confluence.access_token, "token123");
        assert_eq!(confluence.access_secret, "secret456");
        assert_eq!(confluence.consumer_key, "myapp");
    }

    #[test]
    fn test_parse_live_reload_config() {
        let toml = r#"
[live_reload]
enabled = false
watch_patterns = ["**/*.md", "**/*.toml"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(!config.live_reload.enabled);
        assert_eq!(
            config.live_reload.watch_patterns,
            Some(vec!["**/*.md".to_owned(), "**/*.toml".to_owned()])
        );
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
        config.resolve_paths(Path::new("/project")).unwrap();

        assert_eq!(
            config.docs_resolved.source_dir,
            PathBuf::from("/project/documentation")
        );
        assert_eq!(
            config.docs_resolved.project_dir,
            PathBuf::from("/project/.rw")
        );
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
    fn test_diagrams_section_requires_kroki_url() {
        let toml = r#"
[diagrams]
include_dirs = ["diagrams"]
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        let result = config.resolve_paths(Path::new("/project"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Validation(_)),
            "Expected ConfigError::Validation, got {err:?}"
        );
        assert!(err.to_string().contains("kroki_url"));
    }

    #[test]
    fn test_no_diagrams_section_is_valid() {
        let toml = r#"
[docs]
source_dir = "documentation"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.resolve_paths(Path::new("/project")).unwrap();

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
    fn test_apply_cli_settings_source_dir() {
        let mut config = Config::default_with_base(Path::new("/test"));
        let overrides = CliSettings {
            source_dir: Some(PathBuf::from("/custom/docs")),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(
            config.docs_resolved.source_dir,
            PathBuf::from("/custom/docs")
        );
        assert_eq!(config.docs_resolved.project_dir, PathBuf::from("/test/.rw")); // Unchanged
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
    fn test_expand_env_vars_confluence() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("TEST_CONFLUENCE_URL", "https://confluence.test.com");
            std::env::set_var("TEST_TOKEN", "my-token");
            std::env::set_var("TEST_SECRET", "my-secret");
        }

        let toml = r#"
[confluence]
base_url = "${TEST_CONFLUENCE_URL}"
access_token = "${TEST_TOKEN}"
access_secret = "${TEST_SECRET}"
consumer_key = "${TEST_CONSUMER_KEY:-rw}"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.expand_env_vars().unwrap();

        let confluence = config.confluence.unwrap();
        assert_eq!(confluence.base_url, "https://confluence.test.com");
        assert_eq!(confluence.access_token, "my-token");
        assert_eq!(confluence.access_secret, "my-secret");
        assert_eq!(confluence.consumer_key, "rw");

        unsafe {
            std::env::remove_var("TEST_CONFLUENCE_URL");
            std::env::remove_var("TEST_TOKEN");
            std::env::remove_var("TEST_SECRET");
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
[confluence]
base_url = "${MISSING_VAR_CONFIG_TEST}"
access_token = "token"
access_secret = "secret"
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        let result = config.expand_env_vars();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::EnvVar { .. }));
        assert!(err.to_string().contains("MISSING_VAR_CONFIG_TEST"));
        assert!(err.to_string().contains("confluence.base_url"));
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

    fn assert_validation_error_on_confluence(
        config: &ConfluenceConfig,
        expected_substrings: &[&str],
    ) {
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

    /// Create a valid Confluence config for testing.
    fn valid_confluence_config() -> ConfluenceConfig {
        ConfluenceConfig {
            base_url: "https://confluence.example.com".to_owned(),
            access_token: "token".to_owned(),
            access_secret: "secret".to_owned(),
            consumer_key: "rw".to_owned(),
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
    fn test_validate_diagrams_dpi_zero() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.dpi = 0;
        assert_validation_error(&config, &["dpi", "greater than 0"]);
    }

    #[test]
    fn test_validate_diagrams_dpi_too_high() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.diagrams_resolved.dpi = 2000;
        assert_validation_error(&config, &["dpi", "1000"]);
    }

    #[test]
    fn test_confluence_config_validate_valid() {
        let config = valid_confluence_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_confluence_config_validate_empty_token() {
        let config = ConfluenceConfig {
            access_token: String::new(),
            ..valid_confluence_config()
        };
        assert_validation_error_on_confluence(&config, &["access_token", "empty"]);
    }

    #[test]
    fn test_confluence_config_validate_invalid_url() {
        let config = ConfluenceConfig {
            base_url: "not-a-url".to_owned(),
            ..valid_confluence_config()
        };
        assert_validation_error_on_confluence(&config, &["base_url", "http"]);
    }

    #[test]
    fn test_config_require_confluence_returns_validated() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.confluence = Some(valid_confluence_config());
        assert!(config.require_confluence().is_ok());
    }

    #[test]
    fn test_config_require_confluence_missing_section() {
        let config = Config::default_with_base(Path::new("/test"));
        let err = config.require_confluence().unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("[confluence]"));
    }

    #[test]
    fn test_config_require_confluence_invalid_config() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.confluence = Some(ConfluenceConfig {
            access_token: String::new(),
            ..valid_confluence_config()
        });
        let err = config.require_confluence().unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("access_token"));
    }

    #[test]
    fn test_validate_passes_with_confluence_section_present_but_empty_creds() {
        let mut config = Config::default_with_base(Path::new("/test"));
        config.confluence = Some(ConfluenceConfig {
            base_url: String::new(),
            access_token: String::new(),
            access_secret: String::new(),
            consumer_key: String::new(),
        });
        // Config::validate() should pass — confluence is not eagerly validated
        assert!(config.validate().is_ok());
    }
}
