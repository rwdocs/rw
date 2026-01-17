//! Configuration management for Docstage.
//!
//! Parses `docstage.toml` configuration files with serde and provides
//! auto-discovery of config files in parent directories.

use serde::Deserialize;
use std::path::{Path, PathBuf};

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
    /// Directories to search for PlantUML includes.
    pub include_dirs: Vec<PathBuf>,
    /// PlantUML config filename.
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
    /// Load configuration from file.
    ///
    /// If `config_path` is provided, loads from that file.
    /// Otherwise, searches for `docstage.toml` in current directory and parents.
    ///
    /// # Errors
    ///
    /// Returns error if explicit `config_path` doesn't exist or parsing fails.
    pub fn load(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        if let Some(path) = config_path {
            if !path.exists() {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            return Self::load_from_file(path);
        }

        if let Some(discovered) = Self::discover_config() {
            return Self::load_from_file(&discovered);
        }

        Ok(Self::default_with_cwd())
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
        // Resolve docs paths
        self.docs_resolved = DocsConfig {
            source_dir: config_dir.join(self.docs.source_dir.as_deref().unwrap_or("docs")),
            cache_dir: config_dir.join(self.docs.cache_dir.as_deref().unwrap_or(".cache")),
            cache_enabled: self.docs.cache_enabled.unwrap_or(true),
        };

        // Resolve diagrams paths
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
}
