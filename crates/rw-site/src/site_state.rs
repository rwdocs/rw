//! Site state for document hierarchy.
//!
//! Provides the core document site state with efficient path lookups
//! and traversal operations. This is the pure data representation of
//! the site structure, separate from the active [`SiteState`](crate::SiteState) type
//! which handles loading and rendering.
//!
//! # Architecture
//!
//! Pages are stored in a flat `Vec<Page>` with parent/children relationships
//! tracked by indices. This provides:
//! - O(1) URL path lookups via `path_index` `HashMap`
//! - O(1) source path lookups via `source_path_index` `HashMap`
//! - O(d) breadcrumb building where d is the page depth

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::metadata::PageMetadata;

/// Navigation item with children for UI tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NavItem {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
    /// Section type if this item is a section root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_type: Option<String>,
    /// Child navigation items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavItem>,
}

/// Document page data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Page title (from H1 heading, filename, or metadata override).
    pub title: String,
    /// URL path without leading slash (e.g., "guide", "domain/page", "" for root).
    pub path: String,
    /// Relative path to source file (e.g., "guide.md"). `None` for virtual pages.
    pub source_path: Option<PathBuf>,
    /// Page metadata from YAML sidecar file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PageMetadata>,
}

/// Section information for sub-sites or categorized content.
///
/// A section is created when a page has a `type` set in its metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionInfo {
    /// Section title (from page title).
    pub title: String,
    /// URL path to the section root (without leading slash).
    pub path: String,
    /// Section type (from metadata `type` field).
    pub section_type: String,
}

/// Breadcrumb navigation item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
}

/// Document site state with efficient path lookups.
///
/// Pure data structure storing pages in a flat list with parent/children
/// relationships tracked by indices. Provides O(1) URL path and source path
/// lookups, and O(d) breadcrumb building where d is the page depth.
///
/// This is the immutable state representation. For loading and rendering
/// functionality, see [`Site`](crate::Site).
#[derive(Clone)]
pub struct SiteState {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    path_index: HashMap<String, usize>,
    source_path_index: HashMap<PathBuf, usize>,
    sections: HashMap<String, SectionInfo>,
}

impl SiteState {
    /// Create a new site state from components.
    ///
    /// This constructor is primarily used by [`SiteStateBuilder::build`] and
    /// cache deserialization.
    #[must_use]
    pub(crate) fn new(
        pages: Vec<Page>,
        children: Vec<Vec<usize>>,
        parents: Vec<Option<usize>>,
        roots: Vec<usize>,
        sections: HashMap<String, SectionInfo>,
    ) -> Self {
        let path_index = pages
            .iter()
            .enumerate()
            .map(|(i, page)| (page.path.clone(), i))
            .collect();
        let source_path_index: HashMap<PathBuf, usize> = pages
            .iter()
            .enumerate()
            .filter_map(|(i, page)| page.source_path.clone().map(|sp| (sp, i)))
            .collect();

        Self {
            pages,
            children,
            parents,
            roots,
            path_index,
            source_path_index,
            sections,
        }
    }

    /// Get page by URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "domain/page", "" for root)
    ///
    /// # Returns
    ///
    /// Page reference if found, `None` otherwise.
    #[must_use]
    pub fn get_page(&self, path: &str) -> Option<&Page> {
        self.path_index.get(path).map(|&i| &self.pages[i])
    }

    /// Get children of a page.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "" for root)
    ///
    /// # Returns
    ///
    /// Vector of child page references, empty if page not found or has no children.
    #[must_use]
    pub(crate) fn get_children(&self, path: &str) -> Vec<&Page> {
        self.path_index
            .get(path)
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
    /// * `path` - URL path without leading slash (e.g., "guide/setup", "" for root)
    ///
    /// # Returns
    ///
    /// Vector of breadcrumb items for ancestor navigation.
    /// Paths in breadcrumbs are also without leading slash (empty string for root).
    #[must_use]
    pub fn get_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        if path.is_empty() {
            return Vec::new();
        }

        let Some(&idx) = self.path_index.get(path) else {
            // Unknown path - return minimal Home breadcrumb
            return vec![BreadcrumbItem {
                title: "Home".to_string(),
                path: String::new(),
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
        // (Home breadcrumb already represents root so root page would be duplicate)
        ancestors.reverse();

        let mut breadcrumbs = vec![BreadcrumbItem {
            title: "Home".to_string(),
            path: String::new(),
        }];

        // Skip the last element (current page) and exclude root page (already represented by Home)
        breadcrumbs.extend(
            ancestors
                .iter()
                .take(ancestors.len().saturating_sub(1))
                .filter(|page| !page.path.is_empty())
                .map(|page| BreadcrumbItem {
                    title: page.title.clone(),
                    path: page.path.clone(),
                }),
        );

        breadcrumbs
    }

    /// Get root-level pages.
    #[must_use]
    pub(crate) fn get_root_pages(&self) -> Vec<&Page> {
        self.roots.iter().map(|&i| &self.pages[i]).collect()
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
    pub(crate) fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Get children indices (for serialization).
    #[must_use]
    pub(crate) fn children_indices(&self) -> &[Vec<usize>] {
        &self.children
    }

    /// Get parent indices (for serialization).
    #[must_use]
    pub(crate) fn parent_indices(&self) -> &[Option<usize>] {
        &self.parents
    }

    /// Get root indices (for serialization).
    #[must_use]
    pub(crate) fn root_indices(&self) -> &[usize] {
        &self.roots
    }

    /// Get all sections.
    #[must_use]
    pub fn sections(&self) -> &HashMap<String, SectionInfo> {
        &self.sections
    }

    /// Get a section by path.
    #[must_use]
    pub fn get_section(&self, path: &str) -> Option<&SectionInfo> {
        self.sections.get(path)
    }

    /// Build navigation tree from site structure.
    ///
    /// The root page (path="") is excluded from navigation as it serves
    /// as the home page content. Navigation shows only top-level sections.
    ///
    /// # Returns
    ///
    /// List of [`NavItem`] trees for navigation UI.
    /// Paths in navigation items are without leading slash.
    #[must_use]
    pub fn navigation(&self) -> Vec<NavItem> {
        if let Some(root_page) = self.get_page("") {
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

        let section_type = self
            .sections
            .get(&page.path)
            .map(|s| s.section_type.clone());

        NavItem {
            title: page.title.clone(),
            path: page.path.clone(),
            section_type,
            children,
        }
    }
}

/// Builder for constructing [`SiteState`] instances.
pub(crate) struct SiteStateBuilder {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    sections: HashMap<String, SectionInfo>,
}

impl SiteStateBuilder {
    /// Create a new site builder.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            pages: Vec::new(),
            children: Vec::new(),
            parents: Vec::new(),
            roots: Vec::new(),
            sections: HashMap::new(),
        }
    }

    /// Add a page to the site.
    ///
    /// # Arguments
    ///
    /// * `title` - Page title
    /// * `path` - URL path (e.g., "/guide")
    /// * `source_path` - Relative path to source file (e.g., "guide.md"), `None` for virtual pages
    /// * `parent_idx` - Index of parent page, `None` for root
    /// * `metadata` - Optional page metadata from YAML sidecar
    ///
    /// # Returns
    ///
    /// Index of the added page.
    pub(crate) fn add_page(
        &mut self,
        title: String,
        path: String,
        source_path: Option<PathBuf>,
        parent_idx: Option<usize>,
        metadata: Option<PageMetadata>,
    ) -> usize {
        let idx = self.pages.len();

        // Register section if page has a type
        if let Some(ref meta) = metadata
            && let Some(ref section_type) = meta.page_type
        {
            self.sections.insert(
                path.clone(),
                SectionInfo {
                    title: title.clone(),
                    path: path.clone(),
                    section_type: section_type.clone(),
                },
            );
        }

        self.pages.push(Page {
            title,
            path,
            source_path,
            metadata,
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

    /// Build the [`SiteState`] instance.
    #[must_use]
    pub(crate) fn build(self) -> SiteState {
        SiteState::new(
            self.pages,
            self.children,
            self.parents,
            self.roots,
            self.sections,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SiteState tests

    #[test]
    fn test_get_page_returns_page() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let page = site.get_page("guide");

        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "guide");
        assert_eq!(page.source_path, Some(PathBuf::from("guide.md")));
    }

    #[test]
    fn test_get_page_not_found_returns_none() {
        let site = SiteStateBuilder::new().build();

        let page = site.get_page("nonexistent");

        assert!(page.is_none());
    }

    #[test]
    fn test_get_children_returns_children() {
        let mut builder = SiteStateBuilder::new();
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "parent".to_string(),
            Some(PathBuf::from("parent/index.md")),
            None,
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "parent/child".to_string(),
            Some(PathBuf::from("parent/child.md")),
            Some(parent_idx),
            None,
        );
        let site = builder.build();

        let children = site.get_children("parent");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].title, "Child");
    }

    #[test]
    fn test_get_children_not_found_returns_empty() {
        let site = SiteStateBuilder::new().build();

        let children = site.get_children("nonexistent");

        assert!(children.is_empty());
    }

    #[test]
    fn test_get_children_no_children_returns_empty() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let children = site.get_children("guide");

        assert!(children.is_empty());
    }

    #[test]
    fn test_get_breadcrumbs_empty_path_returns_empty() {
        let site = SiteStateBuilder::new().build();

        let breadcrumbs = site.get_breadcrumbs("");

        assert!(breadcrumbs.is_empty());
    }

    #[test]
    fn test_get_breadcrumbs_root_page_returns_home() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("guide");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "");
    }

    #[test]
    fn test_get_breadcrumbs_nested_page_returns_ancestors() {
        let mut builder = SiteStateBuilder::new();
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "parent".to_string(),
            Some(PathBuf::from("parent/index.md")),
            None,
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "parent/child".to_string(),
            Some(PathBuf::from("parent/child.md")),
            Some(parent_idx),
            None,
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("parent/child");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[1].title, "Parent");
        assert_eq!(breadcrumbs[1].path, "parent");
    }

    #[test]
    fn test_get_breadcrumbs_not_found_returns_home() {
        let site = SiteStateBuilder::new().build();

        let breadcrumbs = site.get_breadcrumbs("nonexistent");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
    }

    #[test]
    fn test_get_breadcrumbs_with_root_index_excludes_root() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Welcome".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        let domain_idx = builder.add_page(
            "Domain".to_string(),
            "domain".to_string(),
            Some(PathBuf::from("domain/index.md")),
            Some(root_idx),
            None,
        );
        builder.add_page(
            "Page".to_string(),
            "domain/page".to_string(),
            Some(PathBuf::from("domain/page.md")),
            Some(domain_idx),
            None,
        );
        let site = builder.build();

        let breadcrumbs = site.get_breadcrumbs("domain/page");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "");
        assert_eq!(breadcrumbs[1].title, "Domain");
        assert_eq!(breadcrumbs[1].path, "domain");
    }

    #[test]
    fn test_get_root_pages_returns_roots() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "A".to_string(),
            "a".to_string(),
            Some(PathBuf::from("a.md")),
            None,
            None,
        );
        builder.add_page(
            "B".to_string(),
            "b".to_string(),
            Some(PathBuf::from("b.md")),
            None,
            None,
        );
        let site = builder.build();

        let roots = site.get_root_pages();

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].title, "A");
        assert_eq!(roots[1].title, "B");
    }

    #[test]
    fn test_get_page_by_source_returns_page() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("guide.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Guide");
        assert_eq!(page.unwrap().path, "guide");
    }

    #[test]
    fn test_get_page_by_source_nested_path() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Deep".to_string(),
            "domain/page".to_string(),
            Some(PathBuf::from("domain/page.md")),
            None,
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("domain/page.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().path, "domain/page");
    }

    #[test]
    fn test_get_page_by_source_index_file() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Section".to_string(),
            "section".to_string(),
            Some(PathBuf::from("section/index.md")),
            None,
            None,
        );
        let site = builder.build();

        let page = site.get_page_by_source(Path::new("section/index.md"));

        assert!(page.is_some());
        assert_eq!(page.unwrap().path, "section");
    }

    #[test]
    fn test_get_page_by_source_not_found_returns_none() {
        let site = SiteStateBuilder::new().build();

        let page = site.get_page_by_source(Path::new("nonexistent.md"));

        assert!(page.is_none());
    }

    // SiteStateBuilder tests

    #[test]
    fn test_add_page_returns_index() {
        let mut builder = SiteStateBuilder::new();

        let idx = builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );

        assert_eq!(idx, 0);
    }

    #[test]
    fn test_add_page_increments_index() {
        let mut builder = SiteStateBuilder::new();

        let idx1 = builder.add_page(
            "A".to_string(),
            "a".to_string(),
            Some(PathBuf::from("a.md")),
            None,
            None,
        );
        let idx2 = builder.add_page(
            "B".to_string(),
            "b".to_string(),
            Some(PathBuf::from("b.md")),
            None,
            None,
        );

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
    }

    #[test]
    fn test_add_page_with_parent_links_child() {
        let mut builder = SiteStateBuilder::new();
        let parent_idx = builder.add_page(
            "Parent".to_string(),
            "parent".to_string(),
            Some(PathBuf::from("parent/index.md")),
            None,
            None,
        );
        builder.add_page(
            "Child".to_string(),
            "parent/child".to_string(),
            Some(PathBuf::from("parent/child.md")),
            Some(parent_idx),
            None,
        );
        let site = builder.build();

        let children = site.get_children("parent");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "parent/child");
    }

    #[test]
    fn test_build_creates_site() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );

        let site = builder.build();

        assert!(site.get_page("guide").is_some());
    }

    // Page tests

    #[test]
    fn test_page_creation_stores_values() {
        let page = Page {
            title: "Guide".to_string(),
            path: "guide".to_string(),
            source_path: Some(PathBuf::from("guide.md")),
            metadata: None,
        };

        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "guide");
        assert_eq!(page.source_path, Some(PathBuf::from("guide.md")));
    }

    // BreadcrumbItem tests

    #[test]
    fn test_breadcrumb_item_creation_stores_values() {
        let item = BreadcrumbItem {
            title: "Home".to_string(),
            path: String::new(),
        };

        assert_eq!(item.title, "Home");
        assert_eq!(item.path, "");
    }

    // Navigation tests

    #[test]
    fn test_navigation_empty_site_returns_empty_list() {
        let site = SiteStateBuilder::new().build();

        let nav = site.navigation();

        assert!(nav.is_empty());
    }

    #[test]
    fn test_navigation_flat_site() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        builder.add_page(
            "API".to_string(),
            "api".to_string(),
            Some(PathBuf::from("api.md")),
            None,
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
        let mut builder = SiteStateBuilder::new();
        let parent_idx = builder.add_page(
            "Domain A".to_string(),
            "domain-a".to_string(),
            Some(PathBuf::from("domain-a/index.md")),
            None,
            None,
        );
        builder.add_page(
            "Setup Guide".to_string(),
            "domain-a/guide".to_string(),
            Some(PathBuf::from("domain-a/guide.md")),
            Some(parent_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav.len(), 1);
        let domain = &nav[0];
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.path, "domain-a");
        assert_eq!(domain.children.len(), 1);
        assert_eq!(domain.children[0].title, "Setup Guide");
        assert_eq!(domain.children[0].path, "domain-a/guide");
    }

    #[test]
    fn test_navigation_deeply_nested() {
        let mut builder = SiteStateBuilder::new();
        let idx_a = builder.add_page(
            "A".to_string(),
            "a".to_string(),
            Some(PathBuf::from("a/index.md")),
            None,
            None,
        );
        let idx_b = builder.add_page(
            "B".to_string(),
            "a/b".to_string(),
            Some(PathBuf::from("a/b/index.md")),
            Some(idx_a),
            None,
        );
        builder.add_page(
            "C".to_string(),
            "a/b/c".to_string(),
            Some(PathBuf::from("a/b/c/index.md")),
            Some(idx_b),
            None,
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav[0].title, "A");
        assert_eq!(nav[0].children[0].title, "B");
        assert_eq!(nav[0].children[0].children[0].title, "C");
    }

    #[test]
    fn test_navigation_root_page_excluded() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        builder.add_page(
            "Domains".to_string(),
            "domains".to_string(),
            Some(PathBuf::from("domains/index.md")),
            Some(root_idx),
            None,
        );
        builder.add_page(
            "Usage".to_string(),
            "usage".to_string(),
            Some(PathBuf::from("usage/index.md")),
            Some(root_idx),
            None,
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

    #[test]
    fn test_navigation_includes_section_type() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        builder.add_page(
            "Billing".to_string(),
            "billing".to_string(),
            Some(PathBuf::from("billing/index.md")),
            Some(root_idx),
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("domain".to_string()),
                vars: HashMap::new(),
            }),
        );
        builder.add_page(
            "Payments".to_string(),
            "payments".to_string(),
            Some(PathBuf::from("payments/index.md")),
            Some(root_idx),
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("system".to_string()),
                vars: HashMap::new(),
            }),
        );
        builder.add_page(
            "Getting Started".to_string(),
            "getting-started".to_string(),
            Some(PathBuf::from("getting-started.md")),
            Some(root_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation();

        assert_eq!(nav.len(), 3);

        // Find items by title
        let billing = nav.iter().find(|item| item.title == "Billing").unwrap();
        let payments = nav.iter().find(|item| item.title == "Payments").unwrap();
        let getting_started = nav
            .iter()
            .find(|item| item.title == "Getting Started")
            .unwrap();

        assert_eq!(billing.section_type, Some("domain".to_string()));
        assert_eq!(payments.section_type, Some("system".to_string()));
        assert_eq!(getting_started.section_type, None);
    }

    // NavItem tests

    #[test]
    fn test_nav_item_creation() {
        let item = NavItem {
            title: "Guide".to_string(),
            path: "guide".to_string(),
            section_type: None,
            children: Vec::new(),
        };

        assert_eq!(item.title, "Guide");
        assert_eq!(item.path, "guide");
        assert!(item.children.is_empty());
    }

    #[test]
    fn test_nav_item_with_children() {
        let child = NavItem {
            title: "Child".to_string(),
            path: "parent/child".to_string(),
            section_type: None,
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_string(),
            path: "parent".to_string(),
            section_type: None,
            children: vec![child],
        };

        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].title, "Child");
    }

    #[test]
    fn test_nav_item_serialization_without_children() {
        let item = NavItem {
            title: "Guide".to_string(),
            path: "guide".to_string(),
            section_type: None,
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "guide");
        assert!(json.get("children").is_none()); // Skipped when empty
    }

    #[test]
    fn test_nav_item_serialization_with_children() {
        let child = NavItem {
            title: "Child".to_string(),
            path: "parent/child".to_string(),
            section_type: None,
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_string(),
            path: "parent".to_string(),
            section_type: None,
            children: vec![child],
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Parent");
        assert_eq!(json["path"], "parent");
        assert!(json["children"].is_array());
        assert_eq!(json["children"][0]["title"], "Child");
        assert_eq!(json["children"][0]["path"], "parent/child");
    }

    #[test]
    fn test_nav_item_serialization_with_section_type() {
        let item = NavItem {
            title: "Billing".to_string(),
            path: "domains/billing".to_string(),
            section_type: Some("domain".to_string()),
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Billing");
        assert_eq!(json["path"], "domains/billing");
        assert_eq!(json["section_type"], "domain");
    }

    #[test]
    fn test_nav_item_serialization_skips_none_section_type() {
        let item = NavItem {
            title: "Guide".to_string(),
            path: "guide".to_string(),
            section_type: None,
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert!(json.get("section_type").is_none()); // Skipped when None
    }
}
