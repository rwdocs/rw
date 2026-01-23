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
//! # Thread Safety
//!
//! `SiteLoader` is designed for concurrent access:
//! - `get()` returns `Arc<Site>` with minimal locking (just Arc clone)
//! - `reload_if_needed()` uses double-checked locking for efficient cache validation
//! - `invalidate()` is lock-free (atomic flag)
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use docstage_site::site_loader::{SiteLoader, SiteLoaderConfig};
//!
//! let config = SiteLoaderConfig {
//!     source_dir: PathBuf::from("docs"),
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let loader = Arc::new(SiteLoader::new(config));
//!
//! // Concurrent access from multiple threads
//! let site = loader.reload_if_needed();
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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
///
/// # Thread Safety
///
/// This struct is designed for concurrent access without external locking:
/// - Uses internal `RwLock<Arc<Site>>` for the current site snapshot
/// - Uses `Mutex<()>` for serializing reload operations
/// - Uses `AtomicBool` for cache validity tracking
pub struct SiteLoader {
    config: SiteLoaderConfig,
    file_cache: Box<dyn SiteCache>,
    /// Mutex for serializing reload operations.
    reload_lock: Mutex<()>,
    /// Current site snapshot (atomically swappable).
    current_site: RwLock<Arc<Site>>,
    /// Cache validity flag.
    cache_valid: AtomicBool,
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
        let file_cache: Box<dyn SiteCache> = match &config.cache_dir {
            Some(dir) => Box::new(FileSiteCache::new(dir.clone())),
            None => Box::new(NullSiteCache),
        };

        // Create initial empty site
        let initial_site = Arc::new(SiteBuilder::new(config.source_dir.clone()).build());

        Self {
            config,
            file_cache,
            reload_lock: Mutex::new(()),
            current_site: RwLock::new(initial_site),
            cache_valid: AtomicBool::new(false),
            // Regex for extracting first H1 heading
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
        }
    }

    /// Get current site snapshot.
    ///
    /// Returns an `Arc<Site>` that can be used without holding any lock.
    /// The site is guaranteed to be internally consistent.
    ///
    /// Note: This returns the current snapshot without checking cache validity.
    /// For most use cases, prefer `reload_if_needed()` which ensures the site
    /// is up-to-date.
    #[must_use]
    pub fn get(&self) -> Arc<Site> {
        self.current_site.read().unwrap().clone()
    }

    /// Reload site from filesystem if cache is invalid.
    ///
    /// Uses double-checked locking pattern:
    /// 1. Fast path: return current site if cache valid
    /// 2. Slow path: acquire reload_lock, recheck, then reload
    ///
    /// # Returns
    ///
    /// `Arc<Site>` containing the current site snapshot.
    pub fn reload_if_needed(&self) -> Arc<Site> {
        // Fast path: cache valid
        if self.cache_valid.load(Ordering::Acquire) {
            return self.get();
        }

        // Slow path: acquire reload lock
        let _guard = self.reload_lock.lock().unwrap();

        // Double-check after acquiring lock
        if self.cache_valid.load(Ordering::Acquire) {
            return self.get();
        }

        // Try file cache first
        if let Some(site) = self.file_cache.get() {
            let site = Arc::new(site);
            *self.current_site.write().unwrap() = site.clone();
            self.cache_valid.store(true, Ordering::Release);
            return site;
        }

        // Load from filesystem
        let site = self.load_from_filesystem();
        let site = Arc::new(site);

        // Store in file cache
        self.file_cache.set(&site);

        // Update current site
        *self.current_site.write().unwrap() = site.clone();
        self.cache_valid.store(true, Ordering::Release);

        site
    }

    /// Invalidate cached site.
    ///
    /// Marks cache as invalid. Next `reload_if_needed()` will reload.
    /// Current readers continue using their existing `Arc<Site>`.
    pub fn invalidate(&self) {
        self.cache_valid.store(false, Ordering::Release);
        self.file_cache.invalidate();
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
    // Ensure SiteLoader is Send + Sync for use with Arc
    static_assertions::assert_impl_all!(super::SiteLoader: Send, Sync);
    use std::sync::Arc;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_reload_if_needed_missing_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let config = SiteLoaderConfig {
            source_dir: temp_dir.path().join("nonexistent"),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_empty_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(source_dir.join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert_eq!(site.get_root_pages().len(), 2);
        assert!(site.get_page("/guide").is_some());
        assert!(site.get_page("/api").is_some());
    }

    #[test]
    fn test_reload_if_needed_root_index_adds_home_page() {
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
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Welcome");
        assert_eq!(page.path, "/");
        assert_eq!(page.source_path, PathBuf::from("index.md"));
        // resolve_source_path returns canonicalized path
        let resolved = site.resolve_source_path("/");
        let expected = source_dir.join("index.md").canonicalize().unwrap();
        assert_eq!(resolved, Some(expected));
    }

    #[test]
    fn test_reload_if_needed_nested_structure_builds_site() {
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
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

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
    fn test_reload_if_needed_extracts_title_from_h1() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# My Custom Title\n\nContent.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_reload_if_needed_falls_back_to_filename() {
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
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/setup-guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Setup Guide");
    }

    #[test]
    fn test_reload_if_needed_cyrillic_filename() {
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
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/руководство");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Руководство");
        assert_eq!(page.path, "/руководство");
        assert_eq!(page.source_path, PathBuf::from("руководство.md"));
    }

    #[test]
    fn test_reload_if_needed_skips_hidden_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join(".hidden.md"), "# Hidden").unwrap();
        fs::write(source_dir.join("visible.md"), "# Visible").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_page("/.hidden").is_none());
        assert!(site.get_page("/visible").is_some());
    }

    #[test]
    fn test_reload_if_needed_skips_underscore_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("_partial.md"), "# Partial").unwrap();
        fs::write(source_dir.join("main.md"), "# Main").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_page("/_partial").is_none());
        assert!(site.get_page("/main").is_some());
    }

    #[test]
    fn test_reload_if_needed_directory_without_index_promotes_children() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let no_index_dir = source_dir.join("no-index");
        fs::create_dir_all(&no_index_dir).unwrap();
        fs::write(no_index_dir.join("child.md"), "# Child Page").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        // Child should be at root level (promoted)
        let roots = site.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/no-index/child");
        assert_eq!(roots[0].source_path, PathBuf::from("no-index/child.md"));
    }

    #[test]
    fn test_get_returns_same_arc() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        // First reload to populate
        let _ = loader.reload_if_needed();

        // Get should return the same Arc
        let site1 = loader.get();
        let site2 = loader.get();

        assert!(Arc::ptr_eq(&site1, &site2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site1 = loader.reload_if_needed();
        let site2 = loader.reload_if_needed();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&site1, &site2));
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
        let loader = SiteLoader::new(config);

        // First reload - should NOT have /new
        let site1 = loader.reload_if_needed();
        assert!(site1.get_page("/new").is_none());

        // Add new file and invalidate
        fs::write(source_dir.join("new.md"), "# New").unwrap();
        loader.invalidate();

        // Second reload - should have /new now
        let site2 = loader.reload_if_needed();
        assert!(site2.get_page("/new").is_some());

        // Should be a different Arc (reloaded)
        assert!(!Arc::ptr_eq(&site1, &site2));
    }

    #[test]
    fn test_reload_if_needed_site_has_source_dir() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

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

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = Arc::new(SiteLoader::new(config));

        // Spawn multiple threads accessing concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let loader = Arc::clone(&loader);
                thread::spawn(move || {
                    let site = loader.reload_if_needed();
                    assert!(site.get_page("/guide").is_some());
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_invalidate_and_reload() {
        use std::thread;

        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = Arc::new(SiteLoader::new(config));

        // Initial load
        let _ = loader.reload_if_needed();

        // Spawn threads that invalidate and reload concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let loader = Arc::clone(&loader);
                thread::spawn(move || {
                    if i % 2 == 0 {
                        loader.invalidate();
                    } else {
                        let site = loader.reload_if_needed();
                        // Site should always be valid
                        assert!(site.get_page("/guide").is_some());
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Final state should be valid
        let site = loader.reload_if_needed();
        assert!(site.get_page("/guide").is_some());
    }
}
