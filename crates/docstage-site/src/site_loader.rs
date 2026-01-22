//! Site loading from filesystem.
//!
//! Provides [`SiteLoader`] for building [`Site`] structures by scanning
//! markdown source directories. Includes optional file-based caching.
//!
//! # Architecture
//!
//! The loader scans a source directory recursively:
//! - `index.md` files become section landing pages
//! - Other `.md` files become standalone pages
//! - Files starting with `.` or `_` are skipped
//! - Directories without `index.md` have their children promoted to parent level
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use docstage_site::site_loader::{SiteLoader, SiteLoaderConfig};
//!
//! let config = SiteLoaderConfig {
//!     source_dir: PathBuf::from("docs"),
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let mut loader = SiteLoader::new(config);
//! let site = loader.load(true);
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::site::{Site, SiteBuilder};
use crate::site_cache::{FileSiteCache, NullSiteCache, SiteCache};

/// Configuration for [`SiteLoader`].
#[derive(Clone, Debug)]
pub struct SiteLoaderConfig {
    /// Root directory containing markdown sources.
    pub source_dir: PathBuf,
    /// Cache directory for site structure (`None` disables caching).
    pub cache_dir: Option<PathBuf>,
}

/// Loads site structure from filesystem.
///
/// Scans a source directory for markdown files and builds a [`Site`] structure.
/// Uses `index.md` files as section landing pages. Extracts titles from the
/// first H1 heading in each document, falling back to filename-based titles.
pub struct SiteLoader {
    config: SiteLoaderConfig,
    cache: Box<dyn SiteCache>,
    cached_site: Option<Site>,
    h1_regex: Regex,
}

impl SiteLoader {
    /// Create a new site loader.
    ///
    /// # Arguments
    ///
    /// * `config` - Loader configuration
    ///
    /// # Panics
    ///
    /// Panics if the internal regex for H1 heading extraction fails to compile.
    /// This should never happen as the regex is a compile-time constant.
    #[must_use]
    pub fn new(config: SiteLoaderConfig) -> Self {
        let cache: Box<dyn SiteCache> = match &config.cache_dir {
            Some(dir) => Box::new(FileSiteCache::new(dir.clone())),
            None => Box::new(NullSiteCache),
        };

        Self {
            config,
            cache,
            cached_site: None,
            // Regex for extracting first H1 heading
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
        }
    }

    /// Load site structure from directory.
    ///
    /// # Arguments
    ///
    /// * `use_cache` - Whether to use cached data if available
    ///
    /// # Returns
    ///
    /// Reference to the loaded [`Site`].
    ///
    /// # Panics
    ///
    /// Panics if the internal cached site option is `None` after being set.
    /// This should never happen as this method always populates the cache before returning.
    pub fn load(&mut self, use_cache: bool) -> &Site {
        // Return in-memory cached Site if available
        if use_cache && self.cached_site.is_some() {
            return self.cached_site.as_ref().unwrap();
        }

        // Try file cache
        if use_cache && let Some(site) = self.cache.get() {
            self.cached_site = Some(site);
            return self.cached_site.as_ref().unwrap();
        }

        // Load from filesystem
        let site = self.load_from_filesystem();

        // Store in cache
        self.cache.set(&site);
        self.cached_site = Some(site);
        self.cached_site.as_ref().unwrap()
    }

    /// Invalidate cached site.
    pub fn invalidate(&mut self) {
        self.cached_site = None;
        self.cache.invalidate();
    }

    /// Get source directory.
    #[must_use]
    pub fn source_dir(&self) -> &Path {
        &self.config.source_dir
    }

    /// Scan filesystem and build site structure.
    fn load_from_filesystem(&self) -> Site {
        let mut builder = SiteBuilder::new(self.config.source_dir.clone());

        if !self.config.source_dir.exists() {
            return builder.build();
        }

        // Handle root index.md specially
        let root_index = self.config.source_dir.join("index.md");
        let root_idx = if root_index.exists() {
            let title = self
                .extract_title(&root_index)
                .unwrap_or_else(|| "Home".to_string());
            let source_path = PathBuf::from("index.md");
            Some(builder.add_page(title, "/".to_string(), source_path, None))
        } else {
            None
        };

        self.scan_directory(&self.config.source_dir, "", &mut builder, root_idx);

        builder.build()
    }

    /// Recursively scan directory and add pages to builder.
    ///
    /// Returns list of page indices added at this directory level.
    fn scan_directory(
        &self,
        dir_path: &Path,
        base_path: &str,
        builder: &mut SiteBuilder,
        parent_idx: Option<usize>,
    ) -> Vec<usize> {
        let Ok(entries) = fs::read_dir(dir_path) else {
            return Vec::new();
        };

        // Collect and sort entries: directories first, then alphabetical by name
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type().is_ok_and(|t| t.is_dir());
            let b_is_dir = b.file_type().is_ok_and(|t| t.is_dir());

            // Directories come before files
            b_is_dir.cmp(&a_is_dir).then_with(|| {
                a.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.file_name().to_string_lossy().to_lowercase())
            })
        });

        let mut indices = Vec::new();

        for entry in entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip hidden and underscore-prefixed files/dirs
            if name_str.starts_with('.') || name_str.starts_with('_') {
                continue;
            }

            let path = entry.path();

            if path.is_dir() {
                if let Some(result) = self.process_directory(&path, base_path, builder, parent_idx)
                {
                    indices.extend(result);
                }
            } else if path.extension().is_some_and(|e| e == "md") && name_str != "index.md" {
                let idx = self.process_file(&path, base_path, builder, parent_idx);
                indices.push(idx);
            }
        }

        indices
    }

    /// Process a directory into page(s).
    fn process_directory(
        &self,
        dir_path: &Path,
        base_path: &str,
        builder: &mut SiteBuilder,
        parent_idx: Option<usize>,
    ) -> Option<Vec<usize>> {
        let dir_name = dir_path.file_name()?.to_string_lossy();
        let item_path = if base_path.is_empty() {
            format!("/{dir_name}")
        } else {
            format!("{base_path}/{dir_name}")
        };

        let index_file = dir_path.join("index.md");

        if !index_file.exists() {
            // No index.md - promote children to parent level
            let child_indices = self.scan_directory(dir_path, &item_path, builder, parent_idx);
            return (!child_indices.is_empty()).then_some(child_indices);
        }

        // Create page for this directory
        let title = self
            .extract_title(&index_file)
            .unwrap_or_else(|| Self::title_from_name(&dir_name));
        let source_path = index_file
            .strip_prefix(&self.config.source_dir)
            .unwrap_or(&index_file)
            .to_path_buf();
        let page_idx = builder.add_page(title, item_path.clone(), source_path, parent_idx);

        // Scan children with this page as parent
        self.scan_directory(dir_path, &item_path, builder, Some(page_idx));

        Some(vec![page_idx])
    }

    /// Process a markdown file into a page.
    fn process_file(
        &self,
        file_path: &Path,
        base_path: &str,
        builder: &mut SiteBuilder,
        parent_idx: Option<usize>,
    ) -> usize {
        let file_name = file_path.file_stem().unwrap_or_default().to_string_lossy();
        let item_path = if base_path.is_empty() {
            format!("/{file_name}")
        } else {
            format!("{base_path}/{file_name}")
        };

        let title = self
            .extract_title(file_path)
            .unwrap_or_else(|| Self::title_from_name(&file_name));
        let source_path = file_path
            .strip_prefix(&self.config.source_dir)
            .unwrap_or(file_path)
            .to_path_buf();
        builder.add_page(title, item_path, source_path, parent_idx)
    }

    /// Extract title from first H1 heading in markdown file.
    fn extract_title(&self, file_path: &Path) -> Option<String> {
        let content = fs::read_to_string(file_path).ok()?;
        self.h1_regex
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().trim().to_string())
    }

    /// Generate title from file/directory name.
    fn title_from_name(name: &str) -> String {
        name.replace(['-', '_'], " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_load_missing_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let config = SiteLoaderConfig {
            source_dir: temp_dir.path().join("nonexistent"),
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_load_empty_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_load_flat_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(source_dir.join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert_eq!(site.get_root_pages().len(), 2);
        assert!(site.get_page("/guide").is_some());
        assert!(site.get_page("/api").is_some());
    }

    #[test]
    fn test_load_root_index_adds_home_page() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("index.md"),
            "# Welcome\n\nHome page content.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        let page = site.get_page("/");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Welcome");
        assert_eq!(page.path, "/");
        assert_eq!(page.source_path, PathBuf::from("index.md"));
        assert_eq!(
            site.resolve_source_path("/"),
            Some(source_dir.join("index.md"))
        );
    }

    #[test]
    fn test_load_nested_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let domain_dir = source_dir.join("domain-a");
        fs::create_dir_all(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain A\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Setup Guide\n\nSteps.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        let domain = site.get_page("/domain-a");
        assert!(domain.is_some());
        let domain = domain.unwrap();
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.source_path, PathBuf::from("domain-a/index.md"));

        let children = site.get_children("/domain-a");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].title, "Setup Guide");
        assert_eq!(children[0].source_path, PathBuf::from("domain-a/guide.md"));
    }

    #[test]
    fn test_load_extracts_title_from_h1() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# My Custom Title\n\nContent.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        let page = site.get_page("/guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_load_falls_back_to_filename() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("setup-guide.md"),
            "Content without heading.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        let page = site.get_page("/setup-guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Setup Guide");
    }

    #[test]
    fn test_load_cyrillic_filename() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("руководство.md"),
            "# Руководство\n\nСодержимое.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        let page = site.get_page("/руководство");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Руководство");
        assert_eq!(page.path, "/руководство");
        assert_eq!(page.source_path, PathBuf::from("руководство.md"));
    }

    #[test]
    fn test_load_skips_hidden_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join(".hidden.md"), "# Hidden").unwrap();
        fs::write(source_dir.join("visible.md"), "# Visible").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert!(site.get_page("/.hidden").is_none());
        assert!(site.get_page("/visible").is_some());
    }

    #[test]
    fn test_load_skips_underscore_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("_partial.md"), "# Partial").unwrap();
        fs::write(source_dir.join("main.md"), "# Main").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert!(site.get_page("/_partial").is_none());
        assert!(site.get_page("/main").is_some());
    }

    #[test]
    fn test_load_directory_without_index_promotes_children() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let no_index_dir = source_dir.join("no-index");
        fs::create_dir_all(&no_index_dir).unwrap();
        fs::write(no_index_dir.join("child.md"), "# Child Page").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        // Child should be at root level (promoted)
        let roots = site.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/no-index/child");
        assert_eq!(roots[0].source_path, PathBuf::from("no-index/child.md"));
    }

    #[test]
    fn test_load_caches_site_instance() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site1 = loader.load(true) as *const Site;
        let site2 = loader.load(true) as *const Site;

        assert_eq!(site1, site2);
    }

    #[test]
    fn test_invalidate_clears_cached_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        // First load - should NOT have /new
        let site1 = loader.load(true);
        assert!(site1.get_page("/new").is_none());

        // Add new file and invalidate
        fs::write(source_dir.join("new.md"), "# New").unwrap();
        loader.invalidate();

        // Second load - should have /new now
        let site2 = loader.load(true);
        assert!(site2.get_page("/new").is_some());
    }

    #[test]
    fn test_load_site_has_source_dir() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let mut loader = SiteLoader::new(config);

        let site = loader.load(true);

        assert_eq!(site.source_dir(), source_dir);
    }

    #[test]
    fn test_title_from_name() {
        assert_eq!(SiteLoader::title_from_name("setup-guide"), "Setup Guide");
        assert_eq!(SiteLoader::title_from_name("my_page"), "My Page");
        assert_eq!(
            SiteLoader::title_from_name("complex-name_here"),
            "Complex Name Here"
        );
        assert_eq!(SiteLoader::title_from_name("simple"), "Simple");
    }

    #[test]
    fn test_source_dir_getter() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        assert_eq!(loader.source_dir(), source_dir);
    }
}
