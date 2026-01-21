//! Navigation tree builder.
//!
//! Builds navigation trees from [`Site`] structures for UI presentation.
//! Navigation is a view layer over the site document hierarchy.
//!
//! # Example
//!
//! ```
//! use std::path::PathBuf;
//! use docstage_core::site::SiteBuilder;
//! use docstage_core::navigation::build_navigation;
//!
//! let mut builder = SiteBuilder::new(PathBuf::from("/docs"));
//! builder.add_page("Guide".to_string(), "/guide".to_string(), PathBuf::from("guide.md"), None);
//! let site = builder.build();
//!
//! let nav = build_navigation(&site);
//! assert_eq!(nav.len(), 1);
//! assert_eq!(nav[0].title, "Guide");
//! ```

use serde::Serialize;

use crate::site::{Page, Site};

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

/// Build navigation tree from site structure.
///
/// The root page (path="/") is excluded from navigation as it serves
/// as the home page content. Navigation shows only top-level sections.
///
/// # Arguments
///
/// * `site` - Site structure to build navigation from
///
/// # Returns
///
/// List of [`NavItem`] trees for navigation UI.
#[must_use]
pub fn build_navigation(site: &Site) -> Vec<NavItem> {
    if let Some(root_page) = site.get_page("/") {
        // Root page exists - navigation shows its children (top-level sections)
        site.get_children(&root_page.path)
            .into_iter()
            .map(|page| build_nav_item(site, page))
            .collect()
    } else {
        // No root page - navigation shows all root pages
        site.get_root_pages()
            .into_iter()
            .map(|page| build_nav_item(site, page))
            .collect()
    }
}

/// Recursively build [`NavItem`] from page.
fn build_nav_item(site: &Site, page: &Page) -> NavItem {
    let children = site
        .get_children(&page.path)
        .into_iter()
        .map(|child| build_nav_item(site, child))
        .collect();

    NavItem {
        title: page.title.clone(),
        path: page.path.clone(),
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::SiteBuilder;
    use std::path::PathBuf;

    fn source_dir() -> PathBuf {
        PathBuf::from("/docs")
    }

    #[test]
    fn test_empty_site_returns_empty_list() {
        let site = SiteBuilder::new(source_dir()).build();

        let nav = build_navigation(&site);

        assert!(nav.is_empty());
    }

    #[test]
    fn test_flat_site_builds_navigation() {
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

        let nav = build_navigation(&site);

        assert_eq!(nav.len(), 2);
        let titles: Vec<_> = nav.iter().map(|item| item.title.as_str()).collect();
        assert!(titles.contains(&"Guide"));
        assert!(titles.contains(&"API"));
    }

    #[test]
    fn test_nested_site_builds_navigation_tree() {
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

        let nav = build_navigation(&site);

        assert_eq!(nav.len(), 1);
        let domain = &nav[0];
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.path, "/domain-a");
        assert_eq!(domain.children.len(), 1);
        assert_eq!(domain.children[0].title, "Setup Guide");
        assert_eq!(domain.children[0].path, "/domain-a/guide");
    }

    #[test]
    fn test_deeply_nested_builds_full_tree() {
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

        let nav = build_navigation(&site);

        assert_eq!(nav[0].title, "A");
        assert_eq!(nav[0].children[0].title, "B");
        assert_eq!(nav[0].children[0].children[0].title, "C");
    }

    #[test]
    fn test_root_page_excluded_from_navigation() {
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

        let nav = build_navigation(&site);

        // Navigation should show children of root, not root itself
        assert_eq!(nav.len(), 2);
        let titles: Vec<_> = nav.iter().map(|item| item.title.as_str()).collect();
        assert!(titles.contains(&"Domains"));
        assert!(titles.contains(&"Usage"));
        assert!(!titles.contains(&"Home"));
    }

    #[test]
    fn test_root_page_with_file_siblings_shows_siblings() {
        let mut builder = SiteBuilder::new(source_dir());
        let root_idx = builder.add_page(
            "Home".to_string(),
            "/".to_string(),
            PathBuf::from("index.md"),
            None,
        );
        builder.add_page(
            "About".to_string(),
            "/about".to_string(),
            PathBuf::from("about.md"),
            Some(root_idx),
        );
        builder.add_page(
            "Domains".to_string(),
            "/domains".to_string(),
            PathBuf::from("domains/index.md"),
            Some(root_idx),
        );
        let site = builder.build();

        let nav = build_navigation(&site);

        assert_eq!(nav.len(), 2);
        let titles: Vec<_> = nav.iter().map(|item| item.title.as_str()).collect();
        // Both the sibling file and the subdirectory should appear
        assert!(titles.contains(&"About"));
        assert!(titles.contains(&"Domains"));
        assert!(!titles.contains(&"Home"));
    }

    // NavItem tests

    #[test]
    fn test_nav_item_creation_stores_title_and_path() {
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
    fn test_nav_item_with_children_stores_children() {
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
