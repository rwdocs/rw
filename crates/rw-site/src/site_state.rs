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

/// Information about a navigation scope for the frontend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScopeInfo {
    /// URL path to the scope (with leading slash for frontend).
    pub path: String,
    /// Display title.
    pub title: String,
    /// Section type.
    #[serde(rename = "type")]
    pub section_type: String,
}

/// Result of scoped navigation query.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Navigation {
    /// Navigation items for this scope.
    pub items: Vec<NavItem>,
    /// Current scope info (None at root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopeInfo>,
    /// Parent scope for back navigation (None at root or if no parent section).
    #[serde(rename = "parentScope", skip_serializing_if = "Option::is_none")]
    pub parent_scope: Option<ScopeInfo>,
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
    has_content: Vec<bool>,
}

/// Compute which pages have markdown content in their subtree.
///
/// Uses post-order DFS to compute the values efficiently in O(N) time.
fn compute_has_content(pages: &[Page], children: &[Vec<usize>], roots: &[usize]) -> Vec<bool> {
    fn dfs(idx: usize, pages: &[Page], children: &[Vec<usize>], result: &mut [bool]) {
        // Process children first (post-order)
        for &child in &children[idx] {
            dfs(child, pages, children, result);
        }
        // Page has content if it has source_path OR any child has content
        result[idx] = pages[idx].source_path.is_some() || children[idx].iter().any(|&c| result[c]);
    }

    let mut has_content = vec![false; pages.len()];

    // Traverse from roots to ensure all pages are visited
    for &root in roots {
        dfs(root, pages, children, &mut has_content);
    }

    has_content
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
        let has_content = compute_has_content(&pages, &children, &roots);

        Self {
            pages,
            children,
            parents,
            roots,
            path_index,
            source_path_index,
            sections,
            has_content,
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

    /// Get children of a page that have markdown content in their subtree.
    ///
    /// When `path` is empty and no root page exists, returns root-level pages
    /// with content as a fallback. This handles sites without an `index.md`.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "" for root)
    ///
    /// # Returns
    ///
    /// Vector of child page references that have content, empty if page not found or has no children with content.
    #[must_use]
    fn get_children_with_content(&self, path: &str) -> Vec<&Page> {
        if let Some(&idx) = self.path_index.get(path) {
            self.children[idx]
                .iter()
                .filter(|&&j| self.has_content[j])
                .map(|&j| &self.pages[j])
                .collect()
        } else if path.is_empty() {
            // No root page exists, return root pages as fallback
            self.roots
                .iter()
                .filter(|&&i| self.has_content[i])
                .map(|&i| &self.pages[i])
                .collect()
        } else {
            Vec::new()
        }
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
    #[cfg(test)]
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

    /// Build navigation scoped to a section.
    ///
    /// If `scope_path` is empty, returns root navigation with sections as leaves.
    /// If `scope_path` points to a section, returns that section's children.
    ///
    /// # Arguments
    ///
    /// * `scope_path` - Path to scope (without leading slash), empty for root scope.
    #[must_use]
    pub fn navigation(&self, scope_path: &str) -> Navigation {
        if scope_path.is_empty() {
            // Root scope: show children of root page (or root pages if no index.md)
            let items = self
                .get_children_with_content("")
                .into_iter()
                .map(|page| self.build_nav_item_with_section_cutoff(page))
                .collect();

            Navigation {
                items,
                scope: None,
                parent_scope: None,
            }
        } else {
            // Section scope: show section's children
            let Some(section) = self.sections.get(scope_path) else {
                // Not a valid section, return empty navigation
                return Navigation::default();
            };

            // Get children of this section
            let items = self
                .get_children_with_content(scope_path)
                .into_iter()
                .map(|page| self.build_nav_item_with_section_cutoff(page))
                .collect();

            // Build scope info
            let scope = Some(ScopeInfo {
                path: format!("/{scope_path}"),
                title: section.title.clone(),
                section_type: section.section_type.clone(),
            });

            // Find parent section for back navigation
            let parent_scope = self.find_parent_section(scope_path);

            Navigation {
                items,
                scope,
                parent_scope,
            }
        }
    }

    /// Determine the navigation scope for a page.
    ///
    /// Returns the path of the section this page belongs to (empty for root).
    ///
    /// # Arguments
    ///
    /// * `page_path` - URL path without leading slash.
    #[must_use]
    pub fn get_navigation_scope(&self, page_path: &str) -> String {
        // If the page itself is a section, that's the scope
        if self.sections.contains_key(page_path) {
            return page_path.to_string();
        }

        // Walk up the path to find the nearest section ancestor
        let mut current = page_path.to_string();
        while let Some((parent, _)) = current.rsplit_once('/') {
            if self.sections.contains_key(parent) {
                return parent.to_string();
            }
            current = parent.to_string();
        }

        // No section ancestor found, return root scope
        String::new()
    }

    /// Build [`NavItem`] but stop recursion at section boundaries.
    ///
    /// Sections become leaf nodes - they don't include their children.
    /// Only includes children that have markdown content in their subtree.
    fn build_nav_item_with_section_cutoff(&self, page: &Page) -> NavItem {
        let is_section = self.sections.contains_key(&page.path);

        // Sections become leaf nodes - don't include children
        let children = if is_section {
            Vec::new()
        } else {
            self.get_children_with_content(&page.path)
                .into_iter()
                .map(|child| self.build_nav_item_with_section_cutoff(child))
                .collect()
        };

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

    /// Find the nearest ancestor section for back navigation.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash.
    ///
    /// # Returns
    ///
    /// `ScopeInfo` for the parent section, or `None` if at root level.
    fn find_parent_section(&self, path: &str) -> Option<ScopeInfo> {
        let mut current = path.to_string();
        while let Some((parent, _)) = current.rsplit_once('/') {
            if let Some(section) = self.sections.get(parent) {
                return Some(ScopeInfo {
                    path: format!("/{parent}"),
                    title: section.title.clone(),
                    section_type: section.section_type.clone(),
                });
            }
            current = parent.to_string();
        }
        None
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
            Some(PageMetadata {
                page_type: Some("section".to_string()),
                ..Default::default()
            }),
        );
        builder.add_page(
            "Child".to_string(),
            "parent/child".to_string(),
            Some(PathBuf::from("parent/child.md")),
            Some(parent_idx),
            None,
        );
        let site = builder.build();

        // Verify child is linked via scoped navigation
        let nav = site.navigation("parent");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "parent/child");
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

        let nav = site.navigation("");

        assert!(nav.items.is_empty());
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

        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 2);
        let titles: Vec<_> = nav.items.iter().map(|item| item.title.as_str()).collect();
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

        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 1);
        let domain = &nav.items[0];
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.path, "domain-a");
        // Non-section pages expand children
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

        let nav = site.navigation("");

        // Non-section pages expand children recursively
        assert_eq!(nav.items[0].title, "A");
        assert_eq!(nav.items[0].children[0].title, "B");
        assert_eq!(nav.items[0].children[0].children[0].title, "C");
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

        let nav = site.navigation("");

        // Navigation should show children of root, not root itself
        assert_eq!(nav.items.len(), 2);
        let titles: Vec<_> = nav.items.iter().map(|item| item.title.as_str()).collect();
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

        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 3);

        // Find items by title
        let billing = nav
            .items
            .iter()
            .find(|item| item.title == "Billing")
            .unwrap();
        let payments = nav
            .items
            .iter()
            .find(|item| item.title == "Payments")
            .unwrap();
        let getting_started = nav
            .items
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

    // Scoped navigation tests

    #[test]
    fn test_navigation_root_scope() {
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
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            Some(root_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Root scope should have no scope info
        assert!(nav.scope.is_none());
        assert!(nav.parent_scope.is_none());

        // Should show both items
        assert_eq!(nav.items.len(), 2);

        // Billing (a section) should have no children in root scope
        let billing = nav.items.iter().find(|i| i.title == "Billing").unwrap();
        assert!(billing.children.is_empty());
        assert_eq!(billing.section_type, Some("domain".to_string()));
    }

    #[test]
    fn test_navigation_sections_are_leaves_in_root() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        let billing_idx = builder.add_page(
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
        // Add child under section
        builder.add_page(
            "Payments".to_string(),
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments.md")),
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Billing is a section, so it should have no children in root scope
        let billing = nav.items.iter().find(|i| i.title == "Billing").unwrap();
        assert!(billing.children.is_empty());
    }

    #[test]
    fn test_navigation_section_scope() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        let billing_idx = builder.add_page(
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
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments.md")),
            Some(billing_idx),
            None,
        );
        builder.add_page(
            "Invoicing".to_string(),
            "billing/invoicing".to_string(),
            Some(PathBuf::from("billing/invoicing.md")),
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("billing");

        // Should have scope info
        assert!(nav.scope.is_some());
        let scope = nav.scope.unwrap();
        assert_eq!(scope.path, "/billing");
        assert_eq!(scope.title, "Billing");
        assert_eq!(scope.section_type, "domain");

        // No parent section
        assert!(nav.parent_scope.is_none());

        // Should show billing's children
        assert_eq!(nav.items.len(), 2);
        let titles: Vec<_> = nav.items.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Payments"));
        assert!(titles.contains(&"Invoicing"));
    }

    #[test]
    fn test_navigation_nested_sections() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        let billing_idx = builder.add_page(
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
        let payments_idx = builder.add_page(
            "Payments".to_string(),
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments/index.md")),
            Some(billing_idx),
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("system".to_string()),
                vars: HashMap::new(),
            }),
        );
        builder.add_page(
            "API".to_string(),
            "billing/payments/api".to_string(),
            Some(PathBuf::from("billing/payments/api.md")),
            Some(payments_idx),
            None,
        );
        let site = builder.build();

        // Request navigation for nested section
        let nav = site.navigation("billing/payments");

        // Should have scope info
        let scope = nav.scope.as_ref().unwrap();
        assert_eq!(scope.path, "/billing/payments");
        assert_eq!(scope.title, "Payments");
        assert_eq!(scope.section_type, "system");

        // Should have parent scope pointing to billing
        let parent = nav.parent_scope.as_ref().unwrap();
        assert_eq!(parent.path, "/billing");
        assert_eq!(parent.title, "Billing");
        assert_eq!(parent.section_type, "domain");

        // Should show payments' children
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "API");
    }

    #[test]
    fn test_get_navigation_scope_page_is_section() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Billing".to_string(),
            "billing".to_string(),
            Some(PathBuf::from("billing/index.md")),
            None,
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("domain".to_string()),
                vars: HashMap::new(),
            }),
        );
        let site = builder.build();

        let scope = site.get_navigation_scope("billing");

        assert_eq!(scope, "billing");
    }

    #[test]
    fn test_get_navigation_scope_page_inside_section() {
        let mut builder = SiteStateBuilder::new();
        let billing_idx = builder.add_page(
            "Billing".to_string(),
            "billing".to_string(),
            Some(PathBuf::from("billing/index.md")),
            None,
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("domain".to_string()),
                vars: HashMap::new(),
            }),
        );
        builder.add_page(
            "Payments".to_string(),
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments.md")),
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let scope = site.get_navigation_scope("billing/payments");

        assert_eq!(scope, "billing");
    }

    #[test]
    fn test_get_navigation_scope_page_deeply_nested() {
        let mut builder = SiteStateBuilder::new();
        let billing_idx = builder.add_page(
            "Billing".to_string(),
            "billing".to_string(),
            Some(PathBuf::from("billing/index.md")),
            None,
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("domain".to_string()),
                vars: HashMap::new(),
            }),
        );
        let payments_idx = builder.add_page(
            "Payments".to_string(),
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments/index.md")),
            Some(billing_idx),
            Some(PageMetadata {
                title: None,
                description: None,
                page_type: Some("system".to_string()),
                vars: HashMap::new(),
            }),
        );
        builder.add_page(
            "API".to_string(),
            "billing/payments/api".to_string(),
            Some(PathBuf::from("billing/payments/api.md")),
            Some(payments_idx),
            None,
        );
        let site = builder.build();

        // API page should belong to the payments section (nearest ancestor)
        let scope = site.get_navigation_scope("billing/payments/api");
        assert_eq!(scope, "billing/payments");
    }

    #[test]
    fn test_get_navigation_scope_page_not_in_section() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let scope = site.get_navigation_scope("guide");

        assert_eq!(scope, ""); // Root scope
    }

    #[test]
    fn test_navigation_invalid_scope_returns_empty() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            None,
            None,
        );
        let site = builder.build();

        let nav = site.navigation("nonexistent");

        // Should return empty navigation for invalid scope
        assert!(nav.items.is_empty());
        assert!(nav.scope.is_none());
        assert!(nav.parent_scope.is_none());
    }

    // Content filtering tests

    #[test]
    fn test_navigation_excludes_virtual_pages_without_content() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        // Virtual page (no source_path) with no children
        builder.add_page(
            "Empty Section".to_string(),
            "empty".to_string(),
            None, // Virtual page
            Some(root_idx),
            None,
        );
        // Real page
        builder.add_page(
            "Guide".to_string(),
            "guide".to_string(),
            Some(PathBuf::from("guide.md")),
            Some(root_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Only the real page should be in navigation
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Guide");
    }

    #[test]
    fn test_navigation_includes_virtual_pages_with_content_in_subtree() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        // Virtual page (no source_path) but has a child with content
        let section_idx = builder.add_page(
            "Section".to_string(),
            "section".to_string(),
            None, // Virtual page
            Some(root_idx),
            None,
        );
        // Real child page
        builder.add_page(
            "Child".to_string(),
            "section/child".to_string(),
            Some(PathBuf::from("section/child.md")),
            Some(section_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Section should be included because it has a child with content
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Section");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Child");
    }

    #[test]
    fn test_navigation_filters_nested_virtual_pages_without_content() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        // Virtual page with content
        let section_idx = builder.add_page(
            "Section".to_string(),
            "section".to_string(),
            None,
            Some(root_idx),
            None,
        );
        // Empty virtual child (should be filtered)
        builder.add_page(
            "Empty Child".to_string(),
            "section/empty".to_string(),
            None,
            Some(section_idx),
            None,
        );
        // Real child page
        builder.add_page(
            "Real Child".to_string(),
            "section/real".to_string(),
            Some(PathBuf::from("section/real.md")),
            Some(section_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Section should be included, but only the real child
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Section");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Real Child");
    }

    #[test]
    fn test_navigation_filters_content() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            String::new(),
            Some(PathBuf::from("index.md")),
            None,
            None,
        );
        // Section with type
        let billing_idx = builder.add_page(
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
        // Empty virtual child (should be filtered)
        builder.add_page(
            "Empty".to_string(),
            "billing/empty".to_string(),
            None,
            Some(billing_idx),
            None,
        );
        // Real child
        builder.add_page(
            "Payments".to_string(),
            "billing/payments".to_string(),
            Some(PathBuf::from("billing/payments.md")),
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("billing");

        // Only real child should be in scoped navigation
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Payments");
    }
}
