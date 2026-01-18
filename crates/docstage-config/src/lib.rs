//! Configuration management for Docstage.
//!
//! Parses `docstage.toml` configuration files with serde and provides
//! auto-discovery of config files in parent directories.
//!
//! CLI settings can be applied during load via [`CliSettings`].

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// CLI settings that override configuration file values.
///
/// All fields are optional. Only non-None values override the loaded config.
#[derive(Debug, Clone, Default)]
pub struct CliSettings {
    /// Override server host.
    pub host: Option<String>,
    /// Override server port.
    pub port: Option<u16>,
    /// Override docs source directory.
    pub source_dir: Option<PathBuf>,
    /// Override docs cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Override cache enabled flag.
    pub cache_enabled: Option<bool>,
    /// Override Kroki URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Override live reload enabled flag.
    pub live_reload_enabled: Option<bool>,
}

impl CliSettings {
    /// Check if all override fields are None (no overrides specified).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.host.is_none()
            && self.port.is_none()
            && self.source_dir.is_none()
            && self.cache_dir.is_none()
            && self.cache_enabled.is_none()
            && self.kroki_url.is_none()
            && self.live_reload_enabled.is_none()
    }
}

/// Configuration filename to search for.
pub const CONFIG_FILENAME: &str = "docstage.toml";

/// Application configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Server configuration.
    pub server: ServerConfig,
    /// Documentation configuration (paths are relative strings from TOML).
    #[serde(default)]
    docs: DocsConfigRaw,
    /// Diagram rendering configuration (paths are relative strings from TOML).
    #[serde(default)]
    diagrams: DiagramsConfigRaw,
    /// Live reload configuration.
    pub live_reload: LiveReloadConfig,
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
    fn default() -> Self {
        Self::default_with_base(Path::new("."))
    }
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
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
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

/// Raw docs configuration as parsed from TOML (paths as strings).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DocsConfigRaw {
    source_dir: Option<String>,
    cache_dir: Option<String>,
    cache_enabled: Option<bool>,
}

/// Resolved documentation configuration with absolute paths.
#[derive(Debug, Clone, Default)]
pub struct DocsConfig {
    /// Source directory for markdown files.
    pub source_dir: PathBuf,
    /// Cache directory for rendered pages.
    pub cache_dir: PathBuf,
    /// Whether caching is enabled.
    pub cache_enabled: bool,
}

/// Raw diagrams configuration as parsed from TOML (paths as strings).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DiagramsConfigRaw {
    kroki_url: Option<String>,
    include_dirs: Option<Vec<String>>,
    config_file: Option<String>,
    dpi: Option<u32>,
}

/// Resolved diagram rendering configuration with absolute paths.
#[derive(Debug, Clone, Default)]
pub struct DiagramsConfig {
    /// Kroki server URL for diagram rendering.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` directives.
    pub include_dirs: Vec<PathBuf>,
    /// `PlantUML` config file name.
    pub config_file: Option<String>,
    /// DPI for diagram rendering.
    pub dpi: u32,
}

/// Live reload configuration.
#[derive(Debug, Clone, Deserialize)]
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

/// Confluence configuration.
#[derive(Debug, Clone, Deserialize)]
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
    /// Test configuration.
    pub test: Option<ConfluenceTestConfig>,
}

fn default_consumer_key() -> String {
    "docstage".to_string()
}

/// Confluence test configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfluenceTestConfig {
    /// Space key for testing.
    pub space_key: String,
}

/// Configuration error.
#[derive(Debug)]
pub enum ConfigError {
    /// File not found.
    NotFound(PathBuf),
    /// IO error.
    Io(std::io::Error),
    /// TOML parsing error.
    Parse(toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Configuration file not found: {}", path.display()),
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Parse(err) => write!(f, "TOML parse error: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        Self::Parse(err)
    }
}

impl Config {
    /// Load configuration from file with optional CLI settings.
    ///
    /// If `config_path` is provided, loads from that file.
    /// Otherwise, searches for `docstage.toml` in current directory and parents.
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
        if let Some(cache_dir) = &settings.cache_dir {
            self.docs_resolved.cache_dir.clone_from(cache_dir);
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

    /// Search for config file in current directory and parents.
    #[must_use]
    pub fn discover_config() -> Option<PathBuf> {
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
    #[must_use]
    pub fn default_with_cwd() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Self::default_with_base(&cwd)
    }

    /// Create default config with paths relative to given base directory.
    #[must_use]
    pub fn default_with_base(base: &Path) -> Self {
        Self {
            server: ServerConfig::default(),
            docs: DocsConfigRaw::default(),
            diagrams: DiagramsConfigRaw::default(),
            live_reload: LiveReloadConfig::default(),
            confluence: None,
            docs_resolved: DocsConfig {
                source_dir: base.join("docs"),
                cache_dir: base.join(".cache"),
                cache_enabled: true,
            },
            diagrams_resolved: DiagramsConfig {
                kroki_url: None,
                include_dirs: Vec::new(),
                config_file: None,
                dpi: 192,
            },
            config_path: None,
        }
    }

    /// Load configuration from a specific file.
    fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&content)?;

        let config_dir = path.parent().unwrap_or(Path::new("."));
        config.resolve_paths(config_dir);
        config.config_path = Some(path.to_path_buf());

        Ok(config)
    }

    /// Resolve relative paths to absolute paths based on config directory.
    fn resolve_paths(&mut self, config_dir: &Path) {
        let resolve = |path: Option<&str>, default: &str| config_dir.join(path.unwrap_or(default));

        self.docs_resolved = DocsConfig {
            source_dir: resolve(self.docs.source_dir.as_deref(), "docs"),
            cache_dir: resolve(self.docs.cache_dir.as_deref(), ".cache"),
            cache_enabled: self.docs.cache_enabled.unwrap_or(true),
        };

        self.diagrams_resolved = DiagramsConfig {
            kroki_url: self.diagrams.kroki_url.clone(),
            include_dirs: self
                .diagrams
                .include_dirs
                .as_ref()
                .map(|dirs| dirs.iter().map(|d| config_dir.join(d)).collect())
                .unwrap_or_default(),
            config_file: self.diagrams.config_file.clone(),
            dpi: self.diagrams.dpi.unwrap_or(192),
        };
    }

    /// Get the confluence test config if present.
    #[must_use]
    pub fn confluence_test(&self) -> Option<&ConfluenceTestConfig> {
        self.confluence.as_ref()?.test.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_with_base(Path::new("/test"));
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.docs_resolved.source_dir, PathBuf::from("/test/docs"));
        assert_eq!(
            config.docs_resolved.cache_dir,
            PathBuf::from("/test/.cache")
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
        assert_eq!(config.server.port, 8080);
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

[confluence.test]
space_key = "DOCS"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let confluence = config.confluence.unwrap();
        assert_eq!(confluence.base_url, "https://confluence.example.com");
        assert_eq!(confluence.access_token, "token123");
        assert_eq!(confluence.access_secret, "secret456");
        assert_eq!(confluence.consumer_key, "myapp");
        assert_eq!(confluence.test.unwrap().space_key, "DOCS");
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
            Some(vec!["**/*.md".to_string(), "**/*.toml".to_string()])
        );
    }

    #[test]
    fn test_resolve_paths() {
        let toml = r#"
[docs]
source_dir = "documentation"
cache_dir = ".docstage-cache"

[diagrams]
include_dirs = ["diagrams", "shared/diagrams"]
"#;
        let mut config: Config = toml::from_str(toml).unwrap();
        config.resolve_paths(Path::new("/project"));

        assert_eq!(
            config.docs_resolved.source_dir,
            PathBuf::from("/project/documentation")
        );
        assert_eq!(
            config.docs_resolved.cache_dir,
            PathBuf::from("/project/.docstage-cache")
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
    fn test_apply_cli_settings_host() {
        let mut config = Config::default_with_base(Path::new("/test"));
        let overrides = CliSettings {
            host: Some("0.0.0.0".to_string()),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080); // Unchanged
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
        assert_eq!(
            config.docs_resolved.cache_dir,
            PathBuf::from("/test/.cache")
        ); // Unchanged
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
            kroki_url: Some("https://kroki.example.com".to_string()),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://kroki.example.com".to_string())
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
            host: Some("0.0.0.0".to_string()),
            port: Some(9000),
            kroki_url: Some("https://kroki.io".to_string()),
            live_reload_enabled: Some(false),
            ..Default::default()
        };

        config.apply_cli_settings(&overrides);

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert_eq!(
            config.diagrams_resolved.kroki_url,
            Some("https://kroki.io".to_string())
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
    fn test_cli_settings_is_empty() {
        assert!(CliSettings::default().is_empty());

        assert!(
            !CliSettings {
                host: Some("0.0.0.0".to_string()),
                ..Default::default()
            }
            .is_empty()
        );

        assert!(
            !CliSettings {
                port: Some(9000),
                ..Default::default()
            }
            .is_empty()
        );

        assert!(
            !CliSettings {
                live_reload_enabled: Some(false),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
