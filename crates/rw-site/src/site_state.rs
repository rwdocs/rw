//! Immutable site state and navigation tree building.
//!
//! [`SiteState`] is the pure data representation of the document hierarchy —
//! a flat `Vec<Page>` with parent/child relationships tracked by indices.
//! It provides O(1) page lookups by URL path and O(d) breadcrumb building
//! (where d is the page depth).
//!
//! This module also defines the navigation types ([`NavItem`], [`Navigation`],
//! [`ScopeInfo`]) that the frontend consumes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rw_cache::{CacheBucket, CacheBucketExt};
use rw_sections::{Section, Sections};
use serde::{Deserialize, Serialize};

use crate::page::{BreadcrumbItem, Page};

/// Extracts the last path segment from a `/`-separated path.
fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A node in the navigation tree sent to the frontend.
///
/// Each `NavItem` maps to a page. Items that are
/// [section](crate#sections-and-scoped-navigation) roots have a populated
/// [`section`](Self::section) field and no children (sections are leaf
/// nodes in their parent's navigation — the section's own children appear
/// when navigating into that section).
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct NavItem {
    /// Display title for this navigation entry.
    pub title: String,
    /// URL path without leading slash (e.g., `"guide"`, `"domain/billing"`).
    pub path: String,
    /// Present when this item is a section root. Contains the section kind
    /// and name (e.g., `kind: "domain"`, `name: "billing"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<Section>,
    /// Child navigation items. Empty for section roots and leaf pages.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavItem>,
}

/// Describes which [section](crate#sections-and-scoped-navigation) the
/// navigation sidebar is currently showing.
///
/// Returned as part of [`Navigation`] to tell the frontend which section
/// is active and where "back" navigation should go.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ScopeInfo {
    /// URL path **with** leading slash (e.g., `"/domains/billing"`, `"/"`
    /// for root).
    ///
    /// **Note:** This is the only path field in the crate that includes a
    /// leading slash — all other path fields (on [`NavItem`], [`BreadcrumbItem`],
    /// etc.) omit it. The slash is included here for direct use in frontend
    /// routing URLs.
    pub path: String,
    /// Display title for the scope header.
    pub title: String,
    /// Section identity (kind + name) for this scope.
    pub section: Section,
}

/// The navigation tree for a single [section](crate#sections-and-scoped-navigation) scope.
///
/// Returned by [`Site::navigation`](crate::Site::navigation). Contains the
/// tree of [`NavItem`]s for the sidebar, the current scope, and the parent
/// scope for "back" navigation.
///
/// At the root scope, `parent_scope` is `None`. At any other scope,
/// `parent_scope` points to the nearest ancestor section (or root).
#[derive(Debug, Default, Serialize)]
pub struct Navigation {
    /// Top-level navigation items within this scope.
    pub items: Vec<NavItem>,
    /// The section this navigation belongs to. `None` only in the
    /// `Default` value (empty navigation); always `Some` when returned by
    /// [`Site::navigation`](crate::Site::navigation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopeInfo>,
    /// The parent section for "back" navigation. `None` only at the root scope.
    #[serde(rename = "parentScope", skip_serializing_if = "Option::is_none")]
    pub parent_scope: Option<ScopeInfo>,
}

/// Immutable snapshot of the document hierarchy.
///
/// Stores pages in a flat `Vec` with parent/child relationships tracked by
/// indices. Provides O(1) page lookup by URL path and O(d) breadcrumb
/// building (d = page depth). Also indexes sections for scoped navigation
/// and section ref lookups.
///
/// `SiteState` is the pure data layer — it does not own storage or trigger
/// reloads. See [`Site`](crate::Site) for the higher-level API that manages
/// loading and rendering.
pub struct SiteState {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    path_index: HashMap<String, usize>,
    sections: HashMap<String, Section>,
    sections_by_name: HashMap<String, Vec<usize>>,
    subtree_has_content: Vec<bool>,
}

/// Compute which pages have markdown content in their subtree.
///
/// Uses post-order DFS to compute the values efficiently in O(N) time.
fn compute_subtree_has_content(
    pages: &[Page],
    children: &[Vec<usize>],
    roots: &[usize],
) -> Vec<bool> {
    fn dfs(idx: usize, pages: &[Page], children: &[Vec<usize>], result: &mut [bool]) {
        // Process children first (post-order)
        for &child in &children[idx] {
            dfs(child, pages, children, result);
        }
        // Page has content if it has content OR any child has content
        result[idx] = pages[idx].has_content || children[idx].iter().any(|&c| result[c]);
    }

    let mut subtree_has_content = vec![false; pages.len()];

    // Traverse from roots to ensure all pages are visited
    for &root in roots {
        dfs(root, pages, children, &mut subtree_has_content);
    }

    subtree_has_content
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
        sections: HashMap<String, Section>,
    ) -> Self {
        let path_index: HashMap<String, usize> = pages
            .iter()
            .enumerate()
            .map(|(i, page)| (page.path.clone(), i))
            .collect();
        let subtree_has_content = compute_subtree_has_content(&pages, &children, &roots);

        // Build name-based section index (key = raw directory name, last path segment)
        let mut sections_by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for path in sections.keys() {
            if let Some(&idx) = path_index.get(path.as_str()) {
                let dir_name = last_segment(path);
                sections_by_name
                    .entry(dir_name.to_owned())
                    .or_default()
                    .push(idx);
            }
        }

        Self {
            pages,
            children,
            parents,
            roots,
            path_index,
            sections,
            sections_by_name,
            subtree_has_content,
        }
    }

    /// Returns the page at `path`, or `None` if no page exists there.
    ///
    /// `path` is a URL path without leading slash (e.g., `"guide"`,
    /// `"domain/billing"`, `""` for root).
    #[must_use]
    pub fn get_page(&self, path: &str) -> Option<&Page> {
        self.path_index.get(path).map(|&i| &self.pages[i])
    }

    /// Returns the page title at `path`, falling back to `default` if the page
    /// doesn't exist.
    #[must_use]
    pub fn page_title_or(&self, path: &str, default: impl Into<String>) -> String {
        self.get_page(path)
            .map_or_else(|| default.into(), |p| p.title.clone())
    }

    /// Returns children of `path` whose subtree contains at least one page
    /// with markdown content.
    ///
    /// When `path` is empty and no root `index.md` exists, returns top-level
    /// pages as a fallback.
    #[must_use]
    fn get_children_with_content(&self, path: &str) -> Vec<&Page> {
        if let Some(&idx) = self.path_index.get(path) {
            self.children[idx]
                .iter()
                .filter(|&&j| self.subtree_has_content[j])
                .map(|&j| &self.pages[j])
                .collect()
        } else if path.is_empty() {
            // No root page exists, return root pages as fallback
            self.roots
                .iter()
                .filter(|&&i| self.subtree_has_content[i])
                .map(|&i| &self.pages[i])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Returns the breadcrumb trail for `path`.
    ///
    /// The trail starts with "Home" (path `""`) and includes each ancestor
    /// up to but not including the page itself. Returns an empty `Vec` for
    /// the root path (`""`). For unknown paths, returns `[Home]` so the
    /// frontend always has at least minimal navigation.
    #[must_use]
    pub fn get_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        if path.is_empty() {
            return Vec::new();
        }

        let Some(&idx) = self.path_index.get(path) else {
            // Unknown path - return minimal Home breadcrumb
            return vec![BreadcrumbItem {
                title: "Home".to_owned(),
                path: String::new(),
                section: None,
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
            title: "Home".to_owned(),
            path: String::new(),
            section: None,
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
                    section: None,
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

    /// Finds sections whose last path segment matches `name`.
    ///
    /// For example, `find_sections_by_name("payment-gateway")` matches a
    /// section at `"domains/billing/systems/payment-gateway"`. Returns an
    /// empty `Vec` if no section has that directory name.
    #[must_use]
    pub fn find_sections_by_name(&self, name: &str) -> Vec<(&str, &Section)> {
        self.sections_by_name
            .get(name)
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|&idx| {
                        let path = &self.pages[idx].path;
                        self.sections.get(path).map(|info| (path.as_str(), info))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Load site state from cache.
    ///
    /// Returns `None` on cache miss, etag mismatch, or deserialization failure.
    #[must_use]
    pub(crate) fn from_cache(bucket: &dyn CacheBucket, etag: &str) -> Option<Self> {
        bucket
            .get_json::<CachedSiteState>("structure", etag)
            .map(Into::into)
    }

    /// Store site state in cache.
    pub(crate) fn to_cache(&self, bucket: &dyn CacheBucket, etag: &str) {
        bucket.set_json("structure", etag, &CachedSiteStateRef::from(self));
    }

    /// Builds a navigation tree scoped to `scope_path`.
    ///
    /// Pass `""` for root navigation, or a section root path (e.g.,
    /// `"domains/billing"`) to get that section's children. Section roots
    /// appear as leaf nodes — they do not expand their children inline.
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
                scope: Some(self.root_scope_info()),
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
                title: self.page_title_or(scope_path, scope_path),
                section: section.clone(),
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

    /// Returns the [section ref](crate#sections-and-scoped-navigation) for
    /// the section containing `page_path`.
    ///
    /// Walks up the path hierarchy to find the nearest ancestor that is a
    /// section root. Falls back to `"section:default/root"` when no explicit
    /// section is found.
    #[must_use]
    pub fn get_section_ref(&self, page_path: &str) -> String {
        if let Some(section) = self.sections.get(page_path) {
            return section.to_string();
        }

        let mut current = page_path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if let Some(section) = self.sections.get(parent) {
                return section.to_string();
            }
            current = parent;
        }

        if let Some(section) = self.sections.get("") {
            return section.to_string();
        }

        Section::root().to_string()
    }

    /// Build [`NavItem`] but stop recursion at section boundaries.
    ///
    /// Sections become leaf nodes - they don't include their children.
    /// Only includes children that have markdown content in their subtree.
    fn build_nav_item_with_section_cutoff(&self, page: &Page) -> NavItem {
        let section = self.sections.get(&page.path);

        // Sections become leaf nodes - don't include children
        let children = if section.is_some() {
            Vec::new()
        } else {
            self.get_children_with_content(&page.path)
                .into_iter()
                .map(|child| self.build_nav_item_with_section_cutoff(child))
                .collect()
        };

        NavItem {
            title: page.title.clone(),
            path: page.path.clone(),
            section: section.cloned(),
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
    /// `ScopeInfo` for the parent section. Falls back to the implicit root
    /// section for top-level sections. Returns `None` only for the root scope
    /// itself (which has no parent).
    fn find_parent_section(&self, path: &str) -> Option<ScopeInfo> {
        if path.is_empty() {
            return None;
        }

        let mut current = path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if let Some(section) = self.sections.get(parent) {
                return Some(ScopeInfo {
                    path: format!("/{parent}"),
                    title: self.page_title_or(parent, parent),
                    section: section.clone(),
                });
            }
            current = parent;
        }

        Some(self.root_scope_info())
    }

    /// Build a `ScopeInfo` for the implicit root section.
    fn root_scope_info(&self) -> ScopeInfo {
        ScopeInfo {
            path: "/".to_owned(),
            title: self.page_title_or("", "Home"),
            section: Section::root(),
        }
    }

    /// Builds a [`Sections`] map from this state's section index.
    ///
    /// The resulting map always contains at least a root entry so that
    /// embedded consumers always have a section ref to resolve. Maps
    /// section root URL paths to [`Section`] structs.
    #[must_use]
    pub fn build_sections(&self) -> Arc<Sections> {
        let mut map: HashMap<String, Section> = self
            .sections
            .iter()
            .map(|(path, section)| (path.clone(), section.clone()))
            .collect();

        // Insert implicit root section if no explicit section exists at root
        map.entry(String::new()).or_insert_with(Section::root);

        Arc::new(Sections::new(map))
    }
}

impl Navigation {
    /// Apply sections to navigation items.
    pub fn apply_sections(&mut self, sections: &Sections) {
        if sections.is_empty() {
            return;
        }
        for item in &mut self.items {
            item.apply_sections(sections);
        }
    }
}

impl NavItem {
    fn apply_sections(&mut self, sections: &Sections) {
        if self.section.is_none()
            && let Some(sr) = sections.get(&self.path)
        {
            self.section = Some(sr.clone());
        }
        for child in &mut self.children {
            child.apply_sections(sections);
        }
    }
}

/// Builder for constructing [`SiteState`] instances.
pub(crate) struct SiteStateBuilder {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    sections: HashMap<String, Section>,
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
    /// Returns the index of the added page.
    pub(crate) fn add_page(
        &mut self,
        page: Page,
        parent_idx: Option<usize>,
        page_kind: Option<&str>,
    ) -> usize {
        let idx = self.pages.len();

        // Register section if page has a kind
        if let Some(section_kind) = page_kind {
            let name = if page.path.is_empty() {
                Section::ROOT_NAME.to_owned()
            } else {
                last_segment(&page.path).to_owned()
            };
            self.sections.insert(
                page.path.clone(),
                Section {
                    name,
                    kind: section_kind.to_owned(),
                },
            );
        }

        self.pages.push(page);
        self.children.push(Vec::new());
        self.parents.push(parent_idx);

        if let Some(parent) = parent_idx {
            self.children[parent].push(idx);
        } else {
            self.roots.push(idx);
        }

        idx
    }

    /// Reorder children of `parent_idx` according to `slugs`.
    ///
    /// Listed slugs appear first in declared order, unlisted children
    /// appear after sorted alphabetically by path. Section directories,
    /// missing slugs, and duplicates are warned and skipped.
    pub(crate) fn reorder_children(&mut self, parent_idx: usize, slugs: &[String]) {
        let children = &self.children[parent_idx];
        if children.is_empty() || slugs.is_empty() {
            return;
        }

        let parent_path = self.pages[parent_idx].path.as_str();

        let child_by_path: HashMap<&str, usize> = children
            .iter()
            .map(|&idx| (self.pages[idx].path.as_str(), idx))
            .collect();

        let mut listed = Vec::new();
        let mut seen = HashSet::new();

        for slug in slugs {
            if !seen.insert(slug) {
                tracing::warn!(
                    parent = parent_path,
                    slug = slug.as_str(),
                    "duplicate slug in pages, ignoring"
                );
                continue;
            }

            let child_path = if parent_path.is_empty() {
                slug.to_owned()
            } else {
                format!("{parent_path}/{slug}")
            };

            let Some(&idx) = child_by_path.get(child_path.as_str()) else {
                tracing::warn!(
                    parent = parent_path,
                    slug = slug.as_str(),
                    "slug in pages has no matching child, skipping"
                );
                continue;
            };

            if self.sections.contains_key(child_path.as_str()) {
                tracing::warn!(
                    parent = parent_path,
                    slug = slug.as_str(),
                    "slug in pages matches a section directory, skipping"
                );
                continue;
            }

            listed.push(idx);
        }

        let mut unlisted: Vec<usize> = children
            .iter()
            .filter(|idx| !listed.contains(idx))
            .copied()
            .collect();
        unlisted.sort_by(|&a, &b| self.pages[a].path.cmp(&self.pages[b].path));

        let mut reordered = listed;
        reordered.extend(unlisted);
        self.children[parent_idx] = reordered;
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

/// Borrowed view of cached site state for serialization (zero-copy).
#[derive(Serialize)]
struct CachedSiteStateRef<'a> {
    pages: &'a [Page],
    children: &'a [Vec<usize>],
    parents: &'a [Option<usize>],
    roots: &'a [usize],
    sections: &'a HashMap<String, Section>,
}

impl<'a> From<&'a SiteState> for CachedSiteStateRef<'a> {
    fn from(state: &'a SiteState) -> Self {
        Self {
            pages: &state.pages,
            children: &state.children,
            parents: &state.parents,
            roots: &state.roots,
            sections: &state.sections,
        }
    }
}

/// Cache format for site state deserialization (owned).
#[derive(Deserialize)]
struct CachedSiteState {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    #[serde(default)]
    sections: HashMap<String, Section>,
}

impl From<CachedSiteState> for SiteState {
    fn from(cached: CachedSiteState) -> Self {
        SiteState::new(
            cached.pages,
            cached.children,
            cached.parents,
            cached.roots,
            cached.sections,
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
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let site = builder.build();

        let page = site.get_page("guide");

        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "guide");
        assert!(page.has_content);
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
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Parent".to_owned(),
                path: "parent".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Child".to_owned(),
                path: "parent/child".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Welcome".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let domain_idx = builder.add_page(
            Page {
                title: "Domain".to_owned(),
                path: "domain".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        builder.add_page(
            Page {
                title: "Page".to_owned(),
                path: "domain/page".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let site = builder.build();

        let roots = site.get_root_pages();

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].title, "A");
        assert_eq!(roots[1].title, "B");
    }

    // SiteStateBuilder tests

    #[test]
    fn test_add_page_returns_index() {
        let mut builder = SiteStateBuilder::new();

        let idx = builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );

        assert_eq!(idx, 0);
    }

    #[test]
    fn test_add_page_increments_index() {
        let mut builder = SiteStateBuilder::new();

        let idx1 = builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let idx2 = builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Parent".to_owned(),
                path: "parent".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("section"),
        );
        builder.add_page(
            Page {
                title: "Child".to_owned(),
                path: "parent/child".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            has_content: true,
            description: None,
            origin: None,
            pages: None,
        };

        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "guide");
        assert!(page.has_content);
    }

    // BreadcrumbItem tests

    #[test]
    fn test_breadcrumb_item_creation_stores_values() {
        let item = BreadcrumbItem {
            title: "Home".to_owned(),
            path: String::new(),
            section: None,
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
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "API".to_owned(),
                path: "api".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Domain A".to_owned(),
                path: "domain-a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Setup Guide".to_owned(),
                path: "domain-a/guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let idx_b = builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "a/b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(idx_a),
            None,
        );
        builder.add_page(
            Page {
                title: "C".to_owned(),
                path: "a/b/c".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Domains".to_owned(),
                path: "domains".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        builder.add_page(
            Page {
                title: "Usage".to_owned(),
                path: "usage".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
    fn test_navigation_includes_section() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("system"),
        );
        builder.add_page(
            Page {
                title: "Getting Started".to_owned(),
                path: "getting-started".to_owned(),
                has_content: true,
                ..Default::default()
            },
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

        assert_eq!(billing.section.as_ref().unwrap().kind, "domain");
        assert_eq!(payments.section.as_ref().unwrap().kind, "system");
        assert!(getting_started.section.is_none());
    }

    // NavItem tests

    #[test]
    fn test_nav_item_creation() {
        let item = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
            children: Vec::new(),
        };

        assert_eq!(item.title, "Guide");
        assert_eq!(item.path, "guide");
        assert!(item.children.is_empty());
    }

    #[test]
    fn test_nav_item_with_children() {
        let child = NavItem {
            title: "Child".to_owned(),
            path: "parent/child".to_owned(),
            section: None,
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_owned(),
            path: "parent".to_owned(),
            section: None,
            children: vec![child],
        };

        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].title, "Child");
    }

    #[test]
    fn test_nav_item_serialization_without_children() {
        let item = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
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
            title: "Child".to_owned(),
            path: "parent/child".to_owned(),
            section: None,
            children: Vec::new(),
        };
        let item = NavItem {
            title: "Parent".to_owned(),
            path: "parent".to_owned(),
            section: None,
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
    fn test_nav_item_serialization_with_section() {
        let item = NavItem {
            title: "Billing".to_owned(),
            path: "domains/billing".to_owned(),
            section: Some(Section {
                kind: "domain".to_owned(),
                name: "billing".to_owned(),
            }),
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert_eq!(json["title"], "Billing");
        assert_eq!(json["path"], "domains/billing");
        assert_eq!(json["section"]["kind"], "domain");
        assert_eq!(json["section"]["name"], "billing");
    }

    #[test]
    fn test_nav_item_serialization_skips_none_section() {
        let item = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
            children: Vec::new(),
        };

        let json = serde_json::to_value(&item).unwrap();

        assert!(json.get("section").is_none()); // Skipped when None
    }

    // Scoped navigation tests

    #[test]
    fn test_navigation_root_scope() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("");

        // Root scope should have implicit root section scope
        let scope = nav.scope.as_ref().unwrap();
        assert_eq!(scope.path, "/");
        assert_eq!(scope.title, "Home");
        assert_eq!(scope.section, Section::root());
        assert!(nav.parent_scope.is_none());

        // Should show both items
        assert_eq!(nav.items.len(), 2);

        // Billing (a section) should have no children in root scope
        let billing = nav.items.iter().find(|i| i.title == "Billing").unwrap();
        assert!(billing.children.is_empty());
        assert_eq!(billing.section.as_ref().unwrap().kind, "domain");
    }

    #[test]
    fn test_navigation_sections_are_leaves_in_root() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        // Add child under section
        builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(billing_idx),
            None,
        );
        builder.add_page(
            Page {
                title: "Invoicing".to_owned(),
                path: "billing/invoicing".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
        assert_eq!(scope.section.kind, "domain");
        assert_eq!(scope.section.name, "billing");

        // Parent is implicit root section
        let parent = nav.parent_scope.unwrap();
        assert_eq!(parent.path, "/");
        assert_eq!(parent.title, "Home");
        assert_eq!(parent.section, Section::root());

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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        let payments_idx = builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(billing_idx),
            Some("system"),
        );
        builder.add_page(
            Page {
                title: "API".to_owned(),
                path: "billing/payments/api".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
        assert_eq!(scope.section.kind, "system");
        assert_eq!(scope.section.name, "payments");

        // Should have parent scope pointing to billing
        let parent = nav.parent_scope.as_ref().unwrap();
        assert_eq!(parent.path, "/billing");
        assert_eq!(parent.title, "Billing");
        assert_eq!(parent.section.kind, "domain");
        assert_eq!(parent.section.name, "billing");

        // Should show payments' children
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "API");
    }

    #[test]
    fn test_get_section_ref_page_is_section() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("domain"),
        );
        let site = builder.build();

        let section_ref = site.get_section_ref("billing");

        assert_eq!(section_ref, "domain:default/billing");
    }

    #[test]
    fn test_get_section_ref_page_inside_section() {
        let mut builder = SiteStateBuilder::new();
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("domain"),
        );
        builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let section_ref = site.get_section_ref("billing/payments");

        assert_eq!(section_ref, "domain:default/billing");
    }

    #[test]
    fn test_get_section_ref_page_deeply_nested() {
        let mut builder = SiteStateBuilder::new();
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("domain"),
        );
        let payments_idx = builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(billing_idx),
            Some("system"),
        );
        builder.add_page(
            Page {
                title: "API".to_owned(),
                path: "billing/payments/api".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(payments_idx),
            None,
        );
        let site = builder.build();

        // API page should belong to the payments section (nearest ancestor)
        let section_ref = site.get_section_ref("billing/payments/api");
        assert_eq!(section_ref, "system:default/payments");
    }

    #[test]
    fn test_get_section_ref_page_not_in_section() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let site = builder.build();

        let section_ref = site.get_section_ref("guide");

        // Falls back to implicit root section
        assert_eq!(section_ref, Section::root().to_string());
    }

    #[test]
    fn test_navigation_invalid_scope_returns_empty() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        // Virtual page (no content) with no children
        builder.add_page(
            Page {
                title: "Empty Section".to_owned(),
                path: "empty".to_owned(),
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        // Real page
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        // Virtual page (no content) but has a child with content
        let section_idx = builder.add_page(
            Page {
                title: "Section".to_owned(),
                path: "section".to_owned(),
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        // Real child page
        builder.add_page(
            Page {
                title: "Child".to_owned(),
                path: "section/child".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        // Virtual page with content
        let section_idx = builder.add_page(
            Page {
                title: "Section".to_owned(),
                path: "section".to_owned(),
                ..Default::default()
            },
            Some(root_idx),
            None,
        );
        // Empty virtual child (should be filtered)
        builder.add_page(
            Page {
                title: "Empty Child".to_owned(),
                path: "section/empty".to_owned(),
                ..Default::default()
            },
            Some(section_idx),
            None,
        );
        // Real child page
        builder.add_page(
            Page {
                title: "Real Child".to_owned(),
                path: "section/real".to_owned(),
                has_content: true,
                ..Default::default()
            },
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
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        // Section with type
        let billing_idx = builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        // Empty virtual child (should be filtered)
        builder.add_page(
            Page {
                title: "Empty".to_owned(),
                path: "billing/empty".to_owned(),
                ..Default::default()
            },
            Some(billing_idx),
            None,
        );
        // Real child
        builder.add_page(
            Page {
                title: "Payments".to_owned(),
                path: "billing/payments".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(billing_idx),
            None,
        );
        let site = builder.build();

        let nav = site.navigation("billing");

        // Only real child should be in scoped navigation
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Payments");
    }

    // Implicit root section tests

    #[test]
    fn test_build_sections_implicit_root_when_no_sections() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let site = builder.build();

        let sections = site.build_sections();

        // Should have implicit root section
        let root = sections
            .get("")
            .expect("implicit root section should exist");
        assert_eq!(*root, Section::root());
    }

    #[test]
    fn test_build_sections_no_implicit_root_when_explicit_root_exists() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("component"),
        );
        let site = builder.build();

        let sections = site.build_sections();

        let root = sections
            .get("")
            .expect("explicit root section should exist");
        assert_eq!(root.kind, "component");
        assert_eq!(root.name, "root");
    }

    #[test]
    fn test_build_sections_implicit_root_with_nested_sections() {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Billing".to_owned(),
                path: "billing".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root_idx),
            Some("domain"),
        );
        let site = builder.build();

        let sections = site.build_sections();

        // Should have both implicit root and explicit nested section
        let root = sections
            .get("")
            .expect("implicit root section should exist");
        assert_eq!(*root, Section::root());

        let billing = sections
            .get("billing")
            .expect("explicit section should exist");
        assert_eq!(billing.kind, "domain");
        assert_eq!(billing.name, "billing");
    }

    #[test]
    fn test_build_sections_find_by_ref_implicit_root() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let site = builder.build();

        let sections = site.build_sections();

        let root_ref = Section::root().to_string();
        assert_eq!(sections.find_by_ref(&root_ref), Some(""));
    }

    #[test]
    fn test_root_section_uses_root_name() {
        let section = Section {
            name: Section::ROOT_NAME.to_owned(),
            kind: "component".to_owned(),
        };

        assert_eq!(section.kind, "component");
        assert_eq!(section.name, "root");
    }

    #[test]
    fn test_navigation_respects_page_order() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Advanced".to_owned(),
                path: "advanced".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "Config".to_owned(),
                path: "config".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "Getting Started".to_owned(),
                path: "getting-started".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // Reorder: getting-started first, then config; advanced is unlisted
        let order = vec!["getting-started".to_owned(), "config".to_owned()];
        builder.reorder_children(root, &order);

        let site = builder.build();
        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 3);
        assert_eq!(nav.items[0].path, "getting-started");
        assert_eq!(nav.items[1].path, "config");
        assert_eq!(nav.items[2].path, "advanced"); // unlisted, alphabetical
    }

    #[test]
    fn test_reorder_unlisted_pages_sorted_alphabetically() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "C".to_owned(),
                path: "c".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // Only list "b" — "a" and "c" sorted alphabetically after
        builder.reorder_children(root, &["b".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "b");
        assert_eq!(nav.items[1].path, "a"); // alphabetical
        assert_eq!(nav.items[2].path, "c"); // alphabetical
    }

    #[test]
    fn test_reorder_all_children_listed() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // All children listed — no unlisted remainder
        builder.reorder_children(root, &["a".to_owned(), "b".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "a");
        assert_eq!(nav.items[1].path, "b");
    }

    #[test]
    fn test_reorder_skips_section_slugs() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "Domain".to_owned(),
                path: "domain".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            Some("domain"),
        );

        // "domain" is a section — should be skipped, order unchanged
        builder.reorder_children(root, &["domain".to_owned(), "guide".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        // guide is listed (non-section), domain stays in unlisted position
        assert_eq!(nav.items[0].path, "guide");
        assert_eq!(nav.items[1].path, "domain");
    }

    #[test]
    fn test_reorder_skips_missing_slugs() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // "nonexistent" is not a child — should be skipped
        builder.reorder_children(root, &["nonexistent".to_owned(), "a".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "a");
    }

    #[test]
    fn test_reorder_skips_duplicate_slugs() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // "a" listed twice — second occurrence ignored
        builder.reorder_children(root, &["a".to_owned(), "b".to_owned(), "a".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items.len(), 2);
        assert_eq!(nav.items[0].path, "a");
        assert_eq!(nav.items[1].path, "b");
    }

    #[test]
    fn test_reorder_no_pages_field_keeps_original_order() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "B".to_owned(),
                path: "b".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );

        // Empty slugs = no reorder
        builder.reorder_children(root, &[]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "b"); // original insertion order
        assert_eq!(nav.items[1].path, "a");
    }

    #[test]
    fn test_reorder_nested_directory() {
        let mut builder = SiteStateBuilder::new();
        let root = builder.add_page(
            Page {
                title: "Home".to_owned(),
                path: String::new(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        let guides = builder.add_page(
            Page {
                title: "Guides".to_owned(),
                path: "guides".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(root),
            None,
        );
        builder.add_page(
            Page {
                title: "Advanced".to_owned(),
                path: "guides/advanced".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(guides),
            None,
        );
        builder.add_page(
            Page {
                title: "Getting Started".to_owned(),
                path: "guides/getting-started".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some(guides),
            None,
        );

        let order = vec!["getting-started".to_owned(), "advanced".to_owned()];
        builder.reorder_children(guides, &order);

        let site = builder.build();
        let nav = site.navigation("");
        let guides_nav = &nav.items[0];
        assert_eq!(guides_nav.children[0].path, "guides/getting-started");
        assert_eq!(guides_nav.children[1].path, "guides/advanced");
    }
}
