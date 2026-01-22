//! Site structure for document hierarchy.
//!
//! Provides the core document site structure with efficient path lookups
//! and traversal operations. Includes [`SiteBuilder`] for constructing sites.
//!
//! # Architecture
//!
//! Pages are stored in a flat `Vec<Page>` with parent/children relationships
//! tracked by indices. This provides:
//! - O(1) URL path lookups via `path_index` `HashMap`
//! - O(1) source path lookups via `source_path_index` `HashMap`
//! - O(d) breadcrumb building where d is the page depth
//!
//! # Example
//!
//! ```
//! use std::path::PathBuf;
//! use docstage_site::site::{Site, SiteBuilder};
//!
//! let mut builder = SiteBuilder::new(PathBuf::from("/docs"));
//! let guide_idx = builder.add_page("Guide".to_string(), "/guide".to_string(), PathBuf::from("guide.md"), None);
//! builder.add_page("Setup".to_string(), "/guide/setup".to_string(), PathBuf::from("guide/setup.md"), Some(guide_idx));
//! let site = builder.build();
//!
//! assert!(site.get_page("/guide").is_some());
//!
//! // Build navigation tree
//! let nav = site.navigation();
//! assert_eq!(nav.len(), 1);
//! assert_eq!(nav[0].title, "Guide");
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Navigation item with children for UI tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NavItem {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
    /// Child navigation items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavItem>,
}

/// Document page data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Page title (from H1 heading or filename).
    pub title: String,
    /// URL path (e.g., "/guide").
    pub path: String,
    /// Relative path to source file (e.g., "guide.md").
    pub source_path: PathBuf,
}

/// Breadcrumb navigation item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
}

/// Document site structure with efficient path lookups.
///
/// Stores pages in a flat list with parent/children relationships
/// tracked by indices. Provides O(1) URL path and source path lookups,
/// and O(d) breadcrumb building where d is the page depth.
#[derive(Clone)]
pub struct Site {
    source_dir: PathBuf,
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    path_index: HashMap<String, usize>,
    source_path_index: HashMap<PathBuf, usize>,
}

impl Site {
    /// Create a new site from components.
    ///
    /// This constructor is primarily used by [`SiteBuilder::build`] and
    /// cache deserialization.
    #[must_use]
    pub fn new(
        source_dir: PathBuf,
        pages: Vec<Page>,
        children: Vec<Vec<usize>>,
        parents: Vec<Option<usize>>,
        roots: Vec<usize>,
    ) -> Self {
        let path_index = pages
            .iter()
            .enumerate()
            .map(|(i, page)| (page.path.clone(), i))
            .collect();
        let source_path_index = pages
            .iter()
            .enumerate()
            .map(|(i, page)| (page.source_path.clone(), i))
            .collect();

        Self {
            source_dir,
            pages,
            children,
            parents,
            roots,
            path_index,
            source_path_index,
        }
    }

    /// Get the source directory.
    #[must_use]
    pub fn source_dir(&self) -> &Path {
        &self.source_dir
    }

    /// Get page by URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "/guide" or "guide")
    ///
    /// # Returns
    ///
    /// Page reference if found, `None` otherwise.
    #[must_use]
    pub fn get_page(&self, path: &str) -> Option<&Page> {
        let normalized = Self::normalize_path(path);
        self.path_index.get(&normalized).map(|&i| &self.pages[i])
    }

    /// Get children of a page.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path of the parent page
    ///
    /// # Returns
    ///
    /// Vector of child page references, empty if page not found or has no children.
    #[must_use]
    pub fn get_children(&self, path: &str) -> Vec<&Page> {
        let normalized = Self::normalize_path(path);
        self.path_index
            .get(&normalized)
            .map(|&i| self.children[i].iter().map(|&j| &self.pages[j]).collect())
            .unwrap_or_default()
    }

    /// Build breadcrumbs for a given path.
    ///
    /// Returns breadcrumbs starting with "Home" for non-root pages,
    /// followed by ancestor pages. The current page is not included.
    ///
    /// # Note
    ///
    /// For unknown paths, returns `[Home]` to provide minimal navigation
    /// in UI even when the page doesn't exist in the site structure.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "/guide/setup")
    ///
    /// # Returns
    ///
    /// Vector of breadcrumb items for ancestor navigation.
    #[must_use]
    pub fn get_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        if path.is_empty() {
            return Vec::new();
        }

        let normalized = Self::normalize_path(path);
        let Some(&idx) = self.path_index.get(&normalized) else {
            // Unknown path - return minimal Home breadcrumb
            return vec![BreadcrumbItem {
                title: "Home".to_string(),
                path: "/".to_string(),
            }];
        };

        // Walk up parent chain
        let mut ancestors = Vec::new();
        let mut current = Some(idx);
        while let Some(i) = current {
            ancestors.push(&self.pages[i]);
            current = self.parents[i];
        }

        // Reverse to root-first, exclude current page and root index.md
        // (Home breadcrumb already represents "/" so root page would be duplicate)
        ancestors.reverse();

        let mut breadcrumbs = vec![BreadcrumbItem {
            title: "Home".to_string(),
            path: "/".to_string(),
        }];

        // Skip the last element (current page) and exclude root page (already represented by Home)
        breadcrumbs.extend(
            ancestors
                .iter()
                .take(ancestors.len().saturating_sub(1))
                .filter(|page| page.path != "/")
                .map(|page| BreadcrumbItem {
                    title: page.title.clone(),
                    path: page.path.clone(),
                }),
        );

        breadcrumbs
    }

    /// Get root-level pages.
    #[must_use]
    pub fn get_root_pages(&self) -> Vec<&Page> {
        self.roots.iter().map(|&i| &self.pages[i]).collect()
    }

    /// Resolve URL path to absolute source file path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "/guide")
    ///
    /// # Returns
    ///
    /// Absolute path to source markdown file, or `None` if page not found.
    #[must_use]
    pub fn resolve_source_path(&self, path: &str) -> Option<PathBuf> {
        self.get_page(path)
            .map(|page| self.source_dir.join(&page.source_path))
    }

    /// Get page by source file path.
    ///
    /// # Arguments
    ///
    /// * `source_path` - Relative path to source file (e.g., "guide.md")
    ///
    /// # Returns
    ///
    /// Page reference if found, `None` otherwise.
    #[must_use]
    pub fn get_page_by_source(&self, source_path: &Path) -> Option<&Page> {
        self.source_path_index
            .get(source_path)
            .map(|&i| &self.pages[i])
    }

    /// Get all pages (for serialization).
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Get children indices (for serialization).
    #[must_use]
    pub fn children_indices(&self) -> &[Vec<usize>] {
        &self.children
    }

    /// Get parent indices (for serialization).
    #[must_use]
    pub fn parent_indices(&self) -> &[Option<usize>] {
        &self.parents
    }

    /// Get root indices (for serialization).
    #[must_use]
    pub fn root_indices(&self) -> &[usize] {
        &self.roots
    }

    /// Build navigation tree from site structure.
    ///
    /// The root page (path="/") is excluded from navigation as it serves
    /// as the home page content. Navigation shows only top-level sections.
    ///
    /// # Returns
    ///
    /// List of [`NavItem`] trees for navigation UI.
    #[must_use]
    pub fn navigation(&self) -> Vec<NavItem> {
        if let Some(root_page) = self.get_page("/") {
            // Root page exists - navigation shows its children (top-level sections)
            self.get_children(&root_page.path)
                .into_iter()
                .map(|page| self.build_nav_item(page))
                .collect()
        } else {
            // No root page - navigation shows all root pages
            self.get_root_pages()
                .into_iter()
                .map(|page| self.build_nav_item(page))
                .collect()
        }
    }

    /// Recursively build [`NavItem`] from page.
    fn build_nav_item(&self, page: &Page) -> NavItem {
        let children = self
            .get_children(&page.path)
            .into_iter()
            .map(|child| self.build_nav_item(child))
            .collect();

        NavItem {
            title: page.title.clone(),
            path: page.path.clone(),
            children,
        }
    }

    /// Normalize path to have leading slash.
    fn normalize_path(path: &str) -> String {
        format!("/{}", path.trim_start_matches('/'))
    }
}

/// Builder for constructing [`Site`] instances.
pub struct SiteBuilder {
    source_dir: PathBuf,
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
}

impl SiteBuilder {
    /// Create a new site builder.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown sources
    #[must_use]
    pub fn new(source_dir: PathBuf) -> Self {
        Self {
            source_dir,
            pages: Vec::new(),
            children: Vec::new(),
            parents: Vec::new(),
            roots: Vec::new(),
        }
    }

    /// Add a page to the site.
    ///
    /// # Arguments
    ///
    /// * `title` - Page title
    /// * `path` - URL path (e.g., "/guide")
    /// * `source_path` - Relative path to source file (e.g., "guide.md")
    /// * `parent_idx` - Index of parent page, `None` for root
    ///
    /// # Returns
    ///
    /// Index of the added page.
    pub fn add_page(
        &mut self,
        title: String,
        path: String,
        source_path: PathBuf,
        parent_idx: Option<usize>,
    ) -> usize {
        let idx = self.pages.len();
        self.pages.push(Page {
            title,
            path,
            source_path,
        });
        self.children.push(Vec::new());
        self.parents.push(parent_idx);

        if let Some(parent) = parent_idx {
            self.children[parent].push(idx);
        } else {
            self.roots.push(idx);
        }

        idx
    }

    /// Build the [`Site`] instance.
    #[must_use]
    pub fn build(self) -> Site {
        Site::new(
            self.source_dir,
            self.pages,
            self.children,
            self.parents,
            self.roots,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_dir() -> PathBuf {
        PathBuf::from("/docs")
    }

    // Site tests

    #[test]
    fn test_get_page_returns_page() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let page = site.get_page("/guide");

        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "/guide");
        assert_eq!(page.source_path, PathBuf::from("guide.md"));
    }

    #[test]
    fn test_get_page_not_found_returns_none() {
        let site = SiteBuilder::new(source_dir()).build();

        let page = site.get_page("/nonexistent");

        assert!(page.is_none());
    }

    #[test]
    fn test_get_page_normalizes_path() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let page = site.get_page("guide");

        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Guide");
    }

    #[test]
    fn test_get_children_returns_children() {
        let mut builder = SiteBuilder::new(source_dir());
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "/parent".to_string(),
            PathBuf::from("parent/index.md"),
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "/parent/child".to_string(),
            PathBuf::from("parent/child.md"),
            Some(parent_idx),
        );
        let site = builder.build();

        let children = site.get_children("/parent");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].title, "Child");
    }

    #[test]
    fn test_get_children_not_found_returns_empty() {
        let site = SiteBuilder::new(source_dir()).build();

        let children = site.get_children("/nonexistent");

        assert!(children.is_empty());
    }

    #[test]
    fn test_get_children_no_children_returns_empty() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let children = site.get_children("/guide");

        assert!(children.is_empty());
    }

    #[test]
    fn test_get_breadcrumbs_empty_path_returns_empty() {
        let site = SiteBuilder::new(source_dir()).build();

        let breadcrumbs = site.get_breadcrumbs("");

        assert!(breadcrumbs.is_empty());
    }

    #[test]
    fn test_get_breadcrumbs_root_page_returns_home() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("/guide");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "/");
    }

    #[test]
    fn test_get_breadcrumbs_nested_page_returns_ancestors() {
        let mut builder = SiteBuilder::new(source_dir());
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "/parent".to_string(),
            PathBuf::from("parent/index.md"),
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "/parent/child".to_string(),
            PathBuf::from("parent/child.md"),
            Some(parent_idx),
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("/parent/child");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[1].title, "Parent");
        assert_eq!(breadcrumbs[1].path, "/parent");
    }

    #[test]
    fn test_get_breadcrumbs_not_found_returns_home() {
        let site = SiteBuilder::new(source_dir()).build();

        let breadcrumbs = site.get_breadcrumbs("/nonexistent");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
    }

    #[test]
    fn test_get_breadcrumbs_with_root_index_excludes_root() {
        let mut builder = SiteBuilder::new(source_dir());
        let root_idx = builder.add_page(
            "Welcome".to_string(),
            "/".to_string(),
            PathBuf::from("index.md"),
            None,
        );
        let domain_idx = builder.add_page(
            "Domain".to_string(),
            "/domain".to_string(),
            PathBuf::from("domain/index.md"),
            Some(root_idx),
        );
        builder.add_page(
            "Page".to_string(),
            "/domain/page".to_string(),
            PathBuf::from("domain/page.md"),
            Some(domain_idx),
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("/domain/page");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "/");
        assert_eq!(breadcrumbs[1].title, "Domain");
        assert_eq!(breadcrumbs[1].path, "/domain");
    }

    #[test]
    fn test_get_root_pages_returns_roots() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "A".to_string(),
            "/a".to_string(),
            PathBuf::from("a.md"),
            None,
        );
        builder.add_page(
            "B".to_string(),
            "/b".to_string(),
            PathBuf::from("b.md"),
            None,
        );
        let site = builder.build();

        let roots = site.get_root_pages();

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].title, "A");
        assert_eq!(roots[1].title, "B");
    }

    #[test]
    fn test_source_dir_returns_path() {
        let site = SiteBuilder::new(source_dir()).build();

        assert_eq!(site.source_dir(), Path::new("/docs"));
    }

    #[test]
    fn test_resolve_source_path_returns_absolute_path() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let result = site.resolve_source_path("/guide");

        assert_eq!(result, Some(PathBuf::from("/docs/guide.md")));
    }

    #[test]
    fn test_resolve_source_path_nested_page() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Deep".to_string(),
            "/domain/subdomain/page".to_string(),
            PathBuf::from("domain/subdomain/page.md"),
            None,
        );
        let site = builder.build();

        let result = site.resolve_source_path("/domain/subdomain/page");

        assert_eq!(
            result,
            Some(PathBuf::from("/docs/domain/subdomain/page.md"))
        );
    }

    #[test]
    fn test_resolve_source_path_not_found_returns_none() {
        let site = SiteBuilder::new(source_dir()).build();

        let result = site.resolve_source_path("/nonexistent");

        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_source_path_normalizes_path() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let result = site.resolve_source_path("guide");

        assert_eq!(result, Some(PathBuf::from("/docs/guide.md")));
    }

    #[test]
    fn test_get_page_by_source_returns_page() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("guide.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Guide");
        assert_eq!(page.unwrap().path, "/guide");
    }

    #[test]
    fn test_get_page_by_source_nested_path() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Deep".to_string(),
            "/domain/page".to_string(),
            PathBuf::from("domain/page.md"),
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("domain/page.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().path, "/domain/page");
    }

    #[test]
    fn test_get_page_by_source_index_file() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Section".to_string(),
            "/section".to_string(),
            PathBuf::from("section/index.md"),
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("section/index.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().path, "/section");
    }

    #[test]
    fn test_get_page_by_source_not_found_returns_none() {
        let site = SiteBuilder::new(source_dir()).build();

        let page = site.get_page_by_source(Path::new("nonexistent.md"));

        assert!(page.is_none());
    }

    // SiteBuilder tests

    #[test]
    fn test_add_page_returns_index() {
        let mut builder = SiteBuilder::new(source_dir());

        let idx = builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );

        assert_eq!(idx, 0);
    }

    #[test]
    fn test_add_page_increments_index() {
        let mut builder = SiteBuilder::new(source_dir());

        let idx1 = builder.add_page(
            "A".to_string(),
            "/a".to_string(),
            PathBuf::from("a.md"),
            None,
        );
        let idx2 = builder.add_page(
            "B".to_string(),
            "/b".to_string(),
            PathBuf::from("b.md"),
            None,
        );

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
    }

    #[test]
    fn test_add_page_with_parent_links_child() {
        let mut builder = SiteBuilder::new(source_dir());
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "/parent".to_string(),
            PathBuf::from("parent/index.md"),
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "/parent/child".to_string(),
            PathBuf::from("parent/child.md"),
            Some(parent_idx),
        );
        let site = builder.build();

        let children = site.get_children("/parent");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "/parent/child");
    }

    #[test]
    fn test_build_creates_site() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );

        let site = builder.build();

        assert!(site.get_page("/guide").is_some());
        assert_eq!(site.source_dir(), Path::new("/docs"));
    }

    // Page tests

    #[test]
    fn test_page_creation_stores_values() {
        let page = Page {
            title: "Guide".to_string(),
            path: "/guide".to_string(),
            source_path: PathBuf::from("guide.md"),
        };

        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "/guide");
        assert_eq!(page.source_path, PathBuf::from("guide.md"));
    }

    // BreadcrumbItem tests

    #[test]
    fn test_breadcrumb_item_creation_stores_values() {
        let item = BreadcrumbItem {
            title: "Home".to_string(),
            path: "/".to_string(),
        };

        assert_eq!(item.title, "Home");
        assert_eq!(item.path, "/");
    }

    // Navigation tests

    #[test]
    fn test_navigation_empty_site_returns_empty_list() {
        let site = SiteBuilder::new(source_dir()).build();

        let nav = site.navigation();

        assert!(nav.is_empty());
    }

    #[test]
    fn test_navigation_flat_site() {
        let mut builder = SiteBuilder::new(source_dir());
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            None,
        );
        builder.add_page(
            "API".to_string(),
            "/api".to_string(),
            PathBuf::from("api.md"),
            None,
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav.len(), 2);
        let titles: Vec<_> = nav.iter().map(|item| item.title.as_str()).collect();
        assert!(titles.contains(&"Guide"));
        assert!(titles.contains(&"API"));
    }

    #[test]
    fn test_navigation_nested_site() {
        let mut builder = SiteBuilder::new(source_dir());
        let parent_idx = builder.add_page(
            "Domain A".to_string(),
            "/domain-a".to_string(),
            PathBuf::from("domain-a/index.md"),
            None,
        );
        builder.add_page(
            "Setup Guide".to_string(),
            "/domain-a/guide".to_string(),
            PathBuf::from("domain-a/guide.md"),
            Some(parent_idx),
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav.len(), 1);
        let domain = &nav[0];
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.path, "/domain-a");
        assert_eq!(domain.children.len(), 1);
        assert_eq!(domain.children[0].title, "Setup Guide");
        assert_eq!(domain.children[0].path, "/domain-a/guide");
    }

    #[test]
    fn test_navigation_deeply_nested() {
        let mut builder = SiteBuilder::new(source_dir());
        let idx_a = builder.add_page(
            "A".to_string(),
            "/a".to_string(),
            PathBuf::from("a/index.md"),
            None,
        );
        let idx_b = builder.add_page(
            "B".to_string(),
            "/a/b".to_string(),
            PathBuf::from("a/b/index.md"),
            Some(idx_a),
        );
        builder.add_page(
            "C".to_string(),
            "/a/b/c".to_string(),
            PathBuf::from("a/b/c/index.md"),
            Some(idx_b),
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav[0].title, "A");
        assert_eq!(nav[0].children[0].title, "B");
        assert_eq!(nav[0].children[0].children[0].title, "C");
    }

    #[test]
    fn test_navigation_root_page_excluded() {
        let mut builder = SiteBuilder::new(source_dir());
        let root_idx = builder.add_page(
            "Home".to_string(),
            "/".to_string(),
            PathBuf::from("index.md"),
            None,
        );
        builder.add_page(
            "Domains".to_string(),
            "/domains".to_string(),
            PathBuf::from("domains/index.md"),
            Some(root_idx),
        );
        builder.add_page(
            "Usage".to_string(),
            "/usage".to_string(),
            PathBuf::from("usage/index.md"),
            Some(root_idx),
        );
        let site = builder.build();

        let nav = site.navigation();

        // Navigation should show children of root, not root itself
        assert_eq!(nav.len(), 2);
        let titles: Vec<_> = nav.iter().map(|item| item.title.as_str()).collect();
        assert!(titles.contains(&"Domains"));
        assert!(titles.contains(&"Usage"));
        assert!(!titles.contains(&"Home"));
    }

    // NavItem tests

    #[test]
    fn test_nav_item_creation() {
        let item = NavItem {
            title: "Guide".to_string(),
            path: "/guide".to_string(),
            children: Vec::new(),
        };

        assert_eq!(item.title, "Guide");
        assert_eq!(item.path, "/guide");
        assert!(item.children.is_empty());
    }

    #[test]
    fn test_nav_item_with_children() {
        let child = NavItem {
            title: "Child".to_string(),
            path: "/parent/child".to_string(),
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_string(),
            path: "/parent".to_string(),
            children: vec![child],
        };

        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].title, "Child");
    }

    #[test]
    fn test_nav_item_serialization_without_children() {
        let item = NavItem {
            title: "Guide".to_string(),
            path: "/guide".to_string(),
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "/guide");
        assert!(json.get("children").is_none()); // Skipped when empty
    }

    #[test]
    fn test_nav_item_serialization_with_children() {
        let child = NavItem {
            title: "Child".to_string(),
            path: "/parent/child".to_string(),
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_string(),
            path: "/parent".to_string(),
            children: vec![child],
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Parent");
        assert_eq!(json["path"], "/parent");
        assert!(json["children"].is_array());
        assert_eq!(json["children"][0]["title"], "Child");
        assert_eq!(json["children"][0]["path"], "/parent/child");
    }
}
