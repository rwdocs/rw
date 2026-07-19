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
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use rw_cache::{CacheBucket, CacheBucketExt};
use rw_sections::{Namespace, Section, SectionAnchor, Sections};
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

/// A single section in the flat hierarchy, as returned by
/// [`SiteState::list_sections`].
///
/// Unlike [`NavItem`], which is scoped (nested sections appear as childless
/// leaves), every section in the site appears here once — with its canonical
/// ref, scope path, title, and full ancestry — so a consumer can build a
/// nearest-ancestor roll-up in one pass without per-section round trips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionEntry {
    /// Canonical section ref (`kind:namespace/name`).
    pub section_ref: String,
    /// Scope path, no leading slash (`""` for the root section).
    pub path: String,
    /// Ancestor section refs, nearest-first with the root section last;
    /// excludes the section itself. Empty for the root section.
    pub ancestors: Vec<String>,
}

/// A single page in the site, as returned by [`SiteState::list_pages`].
///
/// Keyed by the same `(section_ref, subpath)` pair the comment system uses as a
/// page's `document_id` (`PageMeta.sectionRef` + `PageMeta.subpath`), so a
/// consumer can join these entries directly against stored comments. See
/// [`list_pages`](SiteState::list_pages) for which pages are included.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageEntry {
    /// Canonical section ref (`kind:namespace/name`) of the enclosing section.
    /// Always equal to `anchors[0].section_ref`.
    pub section_ref: String,
    /// Page path relative to its section root (empty for the section's own
    /// root page; the full path for pages outside any explicit section).
    /// Always equal to `anchors[0].subpath`.
    pub subpath: String,
    /// Site path, no leading slash (empty for the site's root page) — the key
    /// [`Site::render`](crate::Site::render) and
    /// [`Site::page_markdown`](crate::Site::page_markdown) take, so a
    /// consumer can read a listed page without reversing its
    /// `(section_ref, subpath)` key back into a path.
    pub path: String,
    /// Display title (metadata `title`, first H1, or filename).
    pub title: String,
    /// Whether the page has a markdown body. `false` for a virtual directory
    /// page (a directory with no `index.md`), which has a title and a place in
    /// the navigation tree but nothing to render — so a consumer indexing a
    /// site can skip it instead of rendering it just to get nothing back.
    pub has_content: bool,
    /// Every section enclosing this page, innermost first with the root section
    /// last, each paired with the page's path relative to *that* section — the
    /// full chain [`section_ref`](Self::section_ref)/[`subpath`](Self::subpath)
    /// keep only the first link of. Never empty: the root section encloses every
    /// page.
    ///
    /// Lets a consumer whose sections map to owning entities find the nearest
    /// enclosing owner and get a path relative to it in one pass, with no path
    /// arithmetic:
    ///
    /// ```text
    /// let anchor = page.anchors.iter().find(|a| owners.contains_key(&a.section_ref));
    /// ```
    pub anchors: Vec<SectionAnchor>,
    /// Last-modified time as seconds since the Unix epoch — the same per-page
    /// mtime [`render`](crate::Site::render) reports (git author-time for clean
    /// tracked files, filesystem mtime otherwise; the S3 manifest `mtimes` table
    /// for published bundles). `0.0` when the mtime is unknown.
    ///
    /// Filled by [`Site::list_pages`](crate::Site::list_pages), which owns
    /// storage; [`SiteState::list_pages`] itself leaves it `0.0`.
    pub mtime: f64,
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
    /// Ancestry chains for the sections reachable from this navigation view:
    /// each nav item's section, the scope, and the parent scope, mapped to that
    /// section's chain of [`SectionAnchor`]s. Each chain starts with the section
    /// itself (empty subpath), then its ancestors nearest-first with the root
    /// last. Empty (and omitted from JSON) when no section is in view.
    #[serde(rename = "sectionAncestry", skip_serializing_if = "HashMap::is_empty")]
    pub section_ancestry: HashMap<String, Vec<SectionAnchor>>,
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
    sections: Arc<Sections>,
    sections_by_name: HashMap<String, Vec<usize>>,
    subtree_has_content: Vec<bool>,
    root_namespace: Namespace,
    /// Hash of the cross-page inputs that page rendering resolves from this
    /// state (page title/description/`has_content`, the sections map, and the
    /// root namespace). Folded into the page render cache etag so that changing
    /// one page busts the rendered-HTML cache of pages that reference it.
    /// Recomputed in [`SiteState::new`]; never serialized.
    resolution_fingerprint: u64,
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

/// Hash the `SiteState` inputs that cross-page rendering reads.
///
/// Covers wikilink display text + heading-anchor IDs (page titles),
/// section-ref link attributes (the sections map), and C4 meta-include entity
/// info (page title/description/`has_content` + section kind/namespace/name).
/// Deliberately excludes `Page::origin` (read only when rendering the page that
/// owns it — to set that page's own link-resolution base — never read about a
/// page by another page's render, so it is not a cross-page input) and
/// `Page::pages` (navigation ordering, not page-body output).
///
/// `Section` is hashed whole, so any future field is auto-included — adding a
/// field to `Section` widens the page-cache invalidation surface (a deliberate
/// fail-safe toward over-invalidation rather than a silent stale-cache
/// omission).
///
/// The fingerprint ends up in the on-disk page-cache etag, so it must be stable
/// across process restarts. In the current stdlib [`DefaultHasher::new`] uses a
/// fixed seed (unlike [`std::collections::hash_map::RandomState`], which is
/// randomized per process), so identical inputs hash identically in every run of
/// the same binary — an implementation detail we rely on, not a documented
/// guarantee. That is fine: were it ever randomized, or simply changed across
/// Rust-stdlib versions, the only effect is a one-time cold cache miss (a safe
/// re-render, never stale data), and a crate version bump wipes the cache anyway.
fn compute_resolution_fingerprint(
    pages: &[Page],
    sections: &Sections,
    root_namespace: &Namespace,
) -> u64 {
    let mut page_entries: Vec<(&str, &str, Option<&str>, bool)> = pages
        .iter()
        .map(|p| {
            (
                p.path.as_str(),
                p.title.as_str(),
                p.description.as_deref(),
                p.has_content,
            )
        })
        .collect();
    page_entries.sort_by_key(|t| t.0);

    let mut section_entries: Vec<(&str, &Section)> = sections.iter().collect();
    section_entries.sort_by_key(|t| t.0);

    let mut hasher = DefaultHasher::new();
    page_entries.hash(&mut hasher);
    section_entries.hash(&mut hasher);
    root_namespace.hash(&mut hasher);
    hasher.finish()
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
        root_namespace: Namespace,
    ) -> Self {
        let path_index: HashMap<String, usize> = pages
            .iter()
            .enumerate()
            .map(|(i, page)| (page.path.clone(), i))
            .collect();
        let subtree_has_content = compute_subtree_has_content(&pages, &children, &roots);

        let sections = Arc::new(Sections::with_implicit_root(
            sections,
            root_namespace.clone(),
        ));

        // Index sections by directory name (the last path segment). Skip the
        // synthetic "" root: its last segment is "", which no C4 `!include`
        // entity name can match, so indexing it would add a phantom entry.
        let mut sections_by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for path in sections.paths() {
            if path.is_empty() {
                continue;
            }
            if let Some(&idx) = path_index.get(path) {
                let dir_name = last_segment(path);
                sections_by_name
                    .entry(dir_name.to_owned())
                    .or_default()
                    .push(idx);
            }
        }

        let resolution_fingerprint =
            compute_resolution_fingerprint(&pages, &sections, &root_namespace);

        Self {
            pages,
            children,
            parents,
            roots,
            path_index,
            sections,
            sections_by_name,
            subtree_has_content,
            root_namespace,
            resolution_fingerprint,
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
                section_ref: String::new(),
                subpath: String::new(),
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
            section_ref: String::new(),
            subpath: String::new(),
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
                    section_ref: String::new(),
                    subpath: String::new(),
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
        let (items, scope, parent_scope) = if scope_path.is_empty() {
            // Root scope: show children of root page (or root pages if no index.md)
            let items: Vec<NavItem> = self
                .get_children_with_content("")
                .into_iter()
                .map(|page| self.build_nav_item_with_section_cutoff(page))
                .collect();

            (items, Some(self.root_scope_info()), None)
        } else {
            // Section scope: show section's children
            let Some(section) = self.sections.get(scope_path) else {
                // Not a valid section, return empty navigation
                return Navigation::default();
            };

            // Get children of this section
            let items: Vec<NavItem> = self
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

            (items, scope, parent_scope)
        };

        let section_ancestry =
            self.nav_section_ancestry(&items, scope.as_ref(), parent_scope.as_ref());

        Navigation {
            items,
            scope,
            parent_scope,
            section_ancestry,
        }
    }

    /// Build the ancestry map for a navigation view: every section reachable
    /// from `items`, `scope`, and `parent_scope`, mapped to its own nearest-
    /// first, root-last chain. Refs are derived from the existing `section`
    /// objects, so nav items and scopes carry no separate ref field.
    fn nav_section_ancestry(
        &self,
        items: &[NavItem],
        scope: Option<&ScopeInfo>,
        parent_scope: Option<&ScopeInfo>,
    ) -> HashMap<String, Vec<SectionAnchor>> {
        let mut refs: HashSet<String> = HashSet::new();
        collect_nav_item_refs(items, &mut refs);
        if let Some(scope) = scope {
            refs.insert(scope.section.to_string());
        }
        if let Some(parent) = parent_scope {
            refs.insert(parent.section.to_string());
        }
        self.sections.ancestry_for(refs.iter().map(String::as_str))
    }

    /// Returns `(section_ref, subpath)` for the section enclosing `page_path`.
    ///
    /// `section_ref` is the [section ref](crate#sections-and-scoped-navigation)
    /// of the nearest section whose scope path is a prefix of `page_path`;
    /// `subpath` is `page_path` relative to that section's root (empty for the
    /// section's own root page, the full path when only the implicit root
    /// matches). Both come from a single [`Sections::find`] lookup, so they are
    /// always mutually consistent.
    #[must_use]
    pub fn section_location(&self, page_path: &str) -> (String, String) {
        self.sections
            .find(page_path)
            // The "" root in the map makes `find` always match; this fallback
            // is unreachable in practice but avoids an unwrap.
            .map_or_else(
                || {
                    (
                        Section::root(self.root_namespace.clone()).to_string(),
                        page_path.to_owned(),
                    )
                },
                |sp| (sp.section.to_string(), sp.path.to_owned()),
            )
    }

    /// Returns every section in the site as a flat list, sorted by scope path
    /// (the root section's empty path sorts first).
    ///
    /// Includes the implicit or explicit root section. Each entry carries the
    /// canonical ref, scope path, and full ancestry — see
    /// [`SectionEntry`]. This is the unscoped counterpart to
    /// [`navigation`](Self::navigation): it never hides nested sections behind a
    /// scope, so the whole hierarchy is available in one call.
    #[must_use]
    pub fn list_sections(&self) -> Vec<SectionEntry> {
        let mut entries: Vec<SectionEntry> = self
            .sections
            .iter()
            .map(|(path, section)| SectionEntry {
                section_ref: section.to_string(),
                path: path.to_owned(),
                ancestors: self.section_ancestors(path),
            })
            .collect();
        // Scope paths are unique map keys, so stability is irrelevant.
        entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        entries
    }

    /// Returns every page in the site, each carrying its site path, its
    /// `(section_ref, subpath)` key, its full section [`anchors`](PageEntry::anchors)
    /// chain, and its title — the per-page counterpart to
    /// [`list_sections`](Self::list_sections).
    ///
    /// Includes **every** page: the root page (empty `subpath`) and virtual
    /// pages (directory containers without an `index.md`, flagged
    /// [`has_content: false`](PageEntry::has_content)) among them. The
    /// `(section_ref, subpath)` key matches what
    /// [`section_location`](Self::section_location) produces, so it is
    /// byte-identical to the `document_id` the comment system stores.
    ///
    /// `SiteState` owns no storage, so each entry's `mtime` is left `0.0` for a
    /// storage-owning caller ([`Site::list_pages`](crate::Site::list_pages)) to
    /// fill from the entry's `path`.
    ///
    /// Each entry's anchors come from one [`Sections::anchors`] walk, which is
    /// O(depth) with O(1) map lookups, so this is O(pages × depth). Sorted by
    /// `(section_ref, subpath)`, then by `path` to break ties: the key is unique
    /// per page in the common case, but two sections sharing a last segment and
    /// kind (`a/billing` and `b/billing`, both `kind: domain`) collapse to one
    /// ref, so their pages collide. `path` is unique per page, making the order
    /// total regardless.
    #[must_use]
    pub(crate) fn list_pages(&self) -> Vec<PageEntry> {
        let mut entries: Vec<PageEntry> = self
            .pages
            .iter()
            .map(|page| {
                let anchors = self.sections.anchors(&page.path);
                // The root anchor encloses every page, so `anchors` is never
                // empty; fall back to `section_location`, which owns the single
                // (unreachable) rootless-map fallback, rather than add a second.
                let (section_ref, subpath) = anchors.first().map_or_else(
                    || self.section_location(&page.path),
                    |a| (a.section_ref.clone(), a.subpath.clone()),
                );
                PageEntry {
                    section_ref,
                    subpath,
                    path: page.path.clone(),
                    title: page.title.clone(),
                    has_content: page.has_content,
                    anchors,
                    mtime: 0.0,
                }
            })
            .collect();
        entries.sort_unstable_by(|a, b| {
            a.section_ref
                .cmp(&b.section_ref)
                .then_with(|| a.subpath.cmp(&b.subpath))
                .then_with(|| a.path.cmp(&b.path))
        });
        entries
    }

    /// Returns the ancestor section refs for the section at `path`, nearest-first
    /// with the root section last, excluding the section itself.
    fn section_ancestors(&self, path: &str) -> Vec<String> {
        self.sections
            .ancestors(path)
            .map(ToString::to_string)
            .collect()
    }

    /// Inverse of [`section_location`](Self::section_location): the page URL
    /// path for a `(section_ref, subpath)` pair, or `None` if no section has
    /// that ref.
    ///
    /// Reconstructs the path `section_location` would have split into this
    /// pair, in the same no-leading-slash form [`render`](crate::Site::render)
    /// expects.
    #[must_use]
    pub fn page_path_for(&self, section_ref: &str, subpath: &str) -> Option<String> {
        // The implicit-root ref (`section:<ns>/root`) reverse-maps here only
        // because the section map is built with `Sections::with_implicit_root`,
        // which inserts the `""` scope entry that `find_by_ref` matches.
        let scope = self.sections.find_by_ref(section_ref)?;
        Some(match (scope.is_empty(), subpath.is_empty()) {
            (true, _) => subpath.to_owned(),
            (false, true) => scope.to_owned(),
            (false, false) => format!("{scope}/{subpath}"),
        })
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

        // A top-level section's parent is the root, which routes through
        // `root_scope_info` so the "Home" title and any explicit root kind are
        // preserved.
        let (section, scope_path) = self.sections.parent(path)?;
        if scope_path.is_empty() {
            Some(self.root_scope_info())
        } else {
            Some(ScopeInfo {
                path: format!("/{scope_path}"),
                title: self.page_title_or(scope_path, scope_path),
                section: section.clone(),
            })
        }
    }

    /// Build a `ScopeInfo` for the root section.
    ///
    /// Prefers an explicit root section registered at the empty scope path
    /// (when the root page declares a `kind`), falling back to the implicit
    /// `Section::root` otherwise. This keeps the navigation API's root scope
    /// consistent with the page API's [`section_location`](Self::section_location),
    /// which already resolves the root via the sections map.
    fn root_scope_info(&self) -> ScopeInfo {
        let section = self
            .sections
            .get("")
            .cloned()
            .unwrap_or_else(|| Section::root(self.root_namespace.clone()));
        ScopeInfo {
            path: "/".to_owned(),
            title: self.page_title_or("", "Home"),
            section,
        }
    }

    /// Returns the sections map for this site state.
    ///
    /// The map always contains at least the implicit root entry (`""`), so
    /// [`Sections::find`] returns a match for any page path.
    #[must_use]
    pub fn sections(&self) -> &Arc<Sections> {
        &self.sections
    }

    /// Returns the resolution fingerprint — a hash of the cross-page inputs
    /// that page rendering resolves from this state. Used as part of the page
    /// render cache etag so cross-page changes invalidate stale renders.
    #[must_use]
    pub(crate) fn resolution_fingerprint(&self) -> u64 {
        self.resolution_fingerprint
    }
}

/// Collect the section ref of every nav item and its descendants into `refs`.
fn collect_nav_item_refs(items: &[NavItem], refs: &mut HashSet<String>) {
    for item in items {
        if let Some(section) = &item.section {
            refs.insert(section.to_string());
        }
        collect_nav_item_refs(&item.children, refs);
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

/// Find a page's parent index by walking up its URL path.
///
/// Returns the nearest **existing** ancestor — not simply the immediate
/// parent segment. A page at `"a/billing"` whose `"a"` directory has no page
/// of its own parents to the root, not to a nonexistent `"a"`.
fn parent_from_url(url_path: &str, path_index: &HashMap<String, usize>) -> Option<usize> {
    let mut current = url_path;
    while !current.is_empty() {
        let parent_url = current.rsplit_once('/').map_or("", |(parent, _)| parent);
        if let Some(&idx) = path_index.get(parent_url) {
            return Some(idx);
        }
        current = parent_url;
    }
    None
}

/// Builder for constructing [`SiteState`] instances.
pub(crate) struct SiteStateBuilder {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    sections: HashMap<String, Section>,
    root_namespace: Option<Namespace>,
    path_index: HashMap<String, usize>,
    namespaces: Vec<Namespace>,
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
            root_namespace: None,
            path_index: HashMap::new(),
            namespaces: Vec::new(),
        }
    }

    /// Set the namespace of the implicit root section.
    ///
    /// Only exercised by tests (`list_sections_root_ref_honors_custom_root_namespace`,
    /// `fingerprint_changes_on_root_namespace`); production callers always derive
    /// the root namespace from the root page itself. `#[cfg(test)]` avoids a
    /// `dead_code` warning on a production-only build.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn root_namespace(mut self, namespace: Namespace) -> Self {
        self.root_namespace = Some(namespace);
        self
    }

    /// Add a page, deriving its parent from `page.path`.
    ///
    /// The parent is the nearest existing ancestor. `namespace` of `None`
    /// inherits the parent's namespace, matching how storage-loaded pages
    /// inherit down the directory tree. `page_kind` of `Some` registers the
    /// page as a section (visible via [`sections`](SiteState::sections),
    /// [`list_sections`](SiteState::list_sections), and navigation scoping);
    /// `None` leaves it a plain page.
    ///
    /// Returns the index of the added page.
    pub(crate) fn add_page(
        &mut self,
        page: Page,
        page_kind: Option<&str>,
        namespace: Option<Namespace>,
    ) -> usize {
        let parent_idx = parent_from_url(&page.path, &self.path_index);
        let namespace = namespace
            .or_else(|| parent_idx.map(|p| self.namespaces[p].clone()))
            .unwrap_or_default();
        let namespace_for_index = namespace.clone();
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
                    namespace,
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

        self.path_index.insert(self.pages[idx].path.clone(), idx);
        self.namespaces.push(namespace_for_index);

        idx
    }

    /// Reorder children of `parent_idx` according to `slugs`.
    ///
    /// Listed slugs appear first in declared order, unlisted children
    /// appear after sorted alphabetically by path. Section directories,
    /// missing slugs, and duplicates are warned and skipped.
    fn reorder_children(&mut self, parent_idx: usize, slugs: &[String]) {
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

    /// Reorder children of the page at `path`. No-op if no such page exists.
    pub(crate) fn reorder_children_at(&mut self, path: &str, slugs: &[String]) {
        if let Some(&idx) = self.path_index.get(path) {
            self.reorder_children(idx, slugs);
        }
    }

    /// Build the [`SiteState`] instance.
    #[must_use]
    pub(crate) fn build(self) -> SiteState {
        let Self {
            pages,
            children,
            parents,
            roots,
            sections,
            root_namespace,
            path_index,
            namespaces,
        } = self;
        let root_namespace = root_namespace.unwrap_or_else(|| {
            path_index
                .get("")
                .map_or_else(Namespace::default, |&idx| namespaces[idx].clone())
        });
        SiteState::new(pages, children, parents, roots, sections, root_namespace)
    }
}

/// Borrowed view of cached site state for serialization (zero-copy).
///
/// NOTE: `SiteState::resolution_fingerprint` is intentionally NOT part of this
/// struct — it is recomputed in `SiteState::new` on every load (including cache
/// reload) to avoid a serialized value desyncing from the data.
#[derive(Serialize)]
struct CachedSiteStateRef<'a> {
    pages: &'a [Page],
    children: &'a [Vec<usize>],
    parents: &'a [Option<usize>],
    roots: &'a [usize],
    sections: &'a Sections,
    root_namespace: &'a Namespace,
}

impl<'a> From<&'a SiteState> for CachedSiteStateRef<'a> {
    fn from(state: &'a SiteState) -> Self {
        Self {
            pages: &state.pages,
            children: &state.children,
            parents: &state.parents,
            roots: &state.roots,
            sections: &state.sections,
            root_namespace: &state.root_namespace,
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
    #[serde(default)]
    root_namespace: Namespace,
}

impl From<CachedSiteState> for SiteState {
    fn from(cached: CachedSiteState) -> Self {
        SiteState::new(
            cached.pages,
            cached.children,
            cached.parents,
            cached.roots,
            cached.sections,
            cached.root_namespace,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Declarative page descriptor for building test sites.
    ///
    /// Parentage is derived from `path`, so fixtures list pages in any
    /// parent-before-child order and never thread indices.
    struct TestPage {
        path: String,
        title: String,
        kind: Option<String>,
        has_content: bool,
        namespace: Option<Namespace>,
    }

    impl TestPage {
        /// Give this page an explicit namespace instead of inheriting one.
        fn ns(mut self, namespace: Namespace) -> Self {
            self.namespace = Some(namespace);
            self
        }
    }

    /// A content page with no section kind.
    fn page(path: impl Into<String>, title: impl Into<String>) -> TestPage {
        TestPage {
            path: path.into(),
            title: title.into(),
            kind: None,
            has_content: true,
            namespace: None,
        }
    }

    /// A content page that registers a section of `kind`.
    fn section(
        path: impl Into<String>,
        title: impl Into<String>,
        kind: impl Into<String>,
    ) -> TestPage {
        TestPage {
            path: path.into(),
            title: title.into(),
            kind: Some(kind.into()),
            has_content: true,
            namespace: None,
        }
    }

    /// A virtual directory page — no content of its own.
    fn dir(path: impl Into<String>, title: impl Into<String>) -> TestPage {
        TestPage {
            path: path.into(),
            title: title.into(),
            kind: None,
            has_content: false,
            namespace: None,
        }
    }

    /// Build a `SiteState` from page descriptors, in order.
    fn site(pages: &[TestPage]) -> SiteState {
        let mut builder = SiteStateBuilder::new();
        for p in pages {
            builder.add_page(
                Page {
                    title: p.title.clone(),
                    path: p.path.clone(),
                    has_content: p.has_content,
                    ..Default::default()
                },
                p.kind.as_deref(),
                p.namespace.clone(),
            );
        }
        builder.build()
    }

    #[test]
    fn fixture_derives_nearest_existing_ancestor() {
        // "a" has no page of its own, so "a/billing" parents to the root.
        let state = site(&[
            page("", "Home"),
            page("a/billing", "Billing"),
            page("guides", "Guides"),
            page("guides/intro", "Intro"),
        ]);

        // Only the root page is a root-level page; "a/billing" is parented to
        // it rather than becoming a root itself, which is what a naive
        // "strip one path segment" parent lookup for the nonexistent "a"
        // would produce.
        let root_pages = state.get_root_pages();
        assert_eq!(root_pages.len(), 1);
        assert_eq!(root_pages[0].path, "");

        // Its breadcrumb trail is just Home, matching a page whose parent is
        // the root page (the root itself is never listed as a breadcrumb).
        let billing_breadcrumbs = state.get_breadcrumbs("a/billing");
        assert_eq!(billing_breadcrumbs.len(), 1);
        assert_eq!(billing_breadcrumbs[0].title, "Home");

        // Normal nested case: "guides/intro" parents to the existing
        // "guides" page.
        let intro_breadcrumbs = state.get_breadcrumbs("guides/intro");
        assert_eq!(intro_breadcrumbs.len(), 2);
        assert_eq!(intro_breadcrumbs[0].title, "Home");
        assert_eq!(intro_breadcrumbs[1].title, "Guides");
        assert_eq!(intro_breadcrumbs[1].path, "guides");
    }

    // SiteState tests

    #[test]
    fn test_get_page_returns_page() {
        let site = site(&[page("guide", "Guide")]);

        let page = site.get_page("guide");

        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Guide");
        assert_eq!(page.path, "guide");
        assert!(page.has_content);
    }

    #[test]
    fn test_get_page_not_found_returns_none() {
        let site = site(&[]);

        let page = site.get_page("nonexistent");

        assert!(page.is_none());
    }

    #[test]
    fn test_get_breadcrumbs_empty_path_returns_empty() {
        let site = site(&[]);

        let breadcrumbs = site.get_breadcrumbs("");

        assert!(breadcrumbs.is_empty());
    }

    #[test]
    fn test_get_breadcrumbs_root_page_returns_home() {
        let site = site(&[page("guide", "Guide")]);

        let breadcrumbs = site.get_breadcrumbs("guide");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "");
    }

    #[test]
    fn test_get_breadcrumbs_nested_page_returns_ancestors() {
        let site = site(&[page("parent", "Parent"), page("parent/child", "Child")]);

        let breadcrumbs = site.get_breadcrumbs("parent/child");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[1].title, "Parent");
        assert_eq!(breadcrumbs[1].path, "parent");
    }

    #[test]
    fn test_get_breadcrumbs_not_found_returns_home() {
        let site = site(&[]);

        let breadcrumbs = site.get_breadcrumbs("nonexistent");

        assert_eq!(breadcrumbs.len(), 1);
        assert_eq!(breadcrumbs[0].title, "Home");
    }

    #[test]
    fn test_get_breadcrumbs_with_root_index_excludes_root() {
        let site = site(&[
            page("", "Welcome"),
            page("domain", "Domain"),
            page("domain/page", "Page"),
        ]);

        let breadcrumbs = site.get_breadcrumbs("domain/page");

        assert_eq!(breadcrumbs.len(), 2);
        assert_eq!(breadcrumbs[0].title, "Home");
        assert_eq!(breadcrumbs[0].path, "");
        assert_eq!(breadcrumbs[1].title, "Domain");
        assert_eq!(breadcrumbs[1].path, "domain");
    }

    #[test]
    fn test_get_root_pages_returns_roots() {
        let site = site(&[page("a", "A"), page("b", "B")]);

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
    fn test_add_page_links_child() {
        let site = site(&[
            section("parent", "Parent", "section"),
            page("parent/child", "Child"),
        ]);

        // Verify child is linked via scoped navigation
        let nav = site.navigation("parent");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "parent/child");
    }

    #[test]
    fn test_build_creates_site() {
        let site = site(&[page("guide", "Guide")]);

        assert!(site.get_page("guide").is_some());
    }

    // Navigation tests

    #[test]
    fn test_navigation_empty_site_returns_empty_list() {
        let site = site(&[]);

        let nav = site.navigation("");

        assert!(nav.items.is_empty());
    }

    #[test]
    fn test_navigation_flat_site() {
        let site = site(&[page("guide", "Guide"), page("api", "API")]);

        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 2);
        let titles: Vec<_> = nav.items.iter().map(|item| item.title.as_str()).collect();
        assert!(titles.contains(&"Guide"));
        assert!(titles.contains(&"API"));
    }

    #[test]
    fn test_navigation_nested_site() {
        let site = site(&[
            page("domain-a", "Domain A"),
            page("domain-a/guide", "Setup Guide"),
        ]);

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
        let site = site(&[page("a", "A"), page("a/b", "B"), page("a/b/c", "C")]);

        let nav = site.navigation("");

        // Non-section pages expand children recursively
        assert_eq!(nav.items[0].title, "A");
        assert_eq!(nav.items[0].children[0].title, "B");
        assert_eq!(nav.items[0].children[0].children[0].title, "C");
    }

    #[test]
    fn test_navigation_root_page_excluded() {
        let site = site(&[
            page("", "Home"),
            page("domains", "Domains"),
            page("usage", "Usage"),
        ]);

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
        let site = site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            section("payments", "Payments", "system"),
            page("getting-started", "Getting Started"),
        ]);

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
    fn nav_item_serialization_children_shape() {
        let childless = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
            children: vec![],
        };
        let json = serde_json::to_value(&childless).expect("serialize");
        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "guide");
        assert!(json.get("children").is_none(), "empty children are skipped");

        let parent = NavItem {
            title: "Parent".to_owned(),
            path: "parent".to_owned(),
            section: None,
            children: vec![NavItem {
                title: "Child".to_owned(),
                path: "parent/child".to_owned(),
                section: None,
                children: vec![],
            }],
        };
        let json = serde_json::to_value(&parent).expect("serialize");
        assert!(json["children"].is_array());
        assert_eq!(json["children"][0]["title"], "Child");
        assert_eq!(json["children"][0]["path"], "parent/child");
    }

    #[test]
    fn nav_item_serialization_section_shape() {
        let with_section = NavItem {
            title: "Billing".to_owned(),
            path: "billing".to_owned(),
            section: Some(Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            }),
            children: vec![],
        };
        let json = serde_json::to_value(&with_section).expect("serialize");
        assert_eq!(json["section"]["kind"], "domain");
        assert_eq!(json["section"]["name"], "billing");
        // Namespace is #[serde(into = "String")]; guard against it regressing
        // to a tuple-struct object like {"0":"default"}.
        assert_eq!(json["section"]["namespace"], "default");

        let without = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
            children: vec![],
        };
        let json = serde_json::to_value(&without).expect("serialize");
        assert!(json.get("section").is_none(), "None section is skipped");
    }

    // Scoped navigation tests

    #[test]
    fn test_navigation_root_scope() {
        let site = site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            page("guide", "Guide"),
        ]);

        let nav = site.navigation("");

        // Root scope should have implicit root section scope
        let scope = nav.scope.as_ref().unwrap();
        assert_eq!(scope.path, "/");
        assert_eq!(scope.title, "Home");
        assert_eq!(scope.section, Section::root(Namespace::default()));
        assert!(nav.parent_scope.is_none());

        // Should show both items
        assert_eq!(nav.items.len(), 2);

        // Billing (a section) should have no children in root scope
        let billing = nav.items.iter().find(|i| i.title == "Billing").unwrap();
        assert!(billing.children.is_empty());
        assert_eq!(billing.section.as_ref().unwrap().kind, "domain");
    }

    #[test]
    fn test_navigation_root_scope_honors_explicit_root_kind() {
        let site = site(&[section("", "Home", "component"), page("guide", "Guide")]);

        let nav = site.navigation("");

        // The root page declared `kind: component`, so the navigation scope
        // must report the explicit root section — not the synthetic one — and
        // agree with the page API's section_ref for the same root path.
        let scope = nav.scope.as_ref().unwrap();
        assert_eq!(scope.section.kind, "component");
        assert_eq!(scope.section.name, Section::ROOT_NAME);
        assert_eq!(scope.section.to_string(), site.section_location("").0);
        assert_eq!(scope.section.to_string(), "component:default/root");
    }

    #[test]
    fn test_navigation_parent_scope_honors_explicit_root_kind() {
        let site = site(&[
            section("", "Home", "component"),
            section("billing", "Billing", "domain"),
            page("billing/payments", "Payments"),
        ]);

        let nav = site.navigation("billing");

        // The top-level "billing" section's parent is the root. Since the root
        // declared `kind: component`, the back-navigation parent scope must
        // carry that kind rather than the synthetic `section` kind.
        let parent = nav.parent_scope.as_ref().unwrap();
        assert_eq!(parent.path, "/");
        assert_eq!(parent.section.kind, "component");
        assert_eq!(parent.section.name, Section::ROOT_NAME);
    }

    #[test]
    fn test_navigation_root_scope_falls_back_to_synthetic_root() {
        let site = site(&[page("", "Home"), page("guide", "Guide")]);

        let nav = site.navigation("");

        // No root kind declared: the scope still reports the synthetic root,
        // unchanged from prior behavior.
        let scope = nav.scope.as_ref().unwrap();
        assert_eq!(scope.section, Section::root(Namespace::default()));
    }

    #[test]
    fn test_navigation_sections_are_leaves_in_root() {
        let site = site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            page("billing/payments", "Payments"),
        ]);

        let nav = site.navigation("");

        // Billing is a section, so it should have no children in root scope
        let billing = nav.items.iter().find(|i| i.title == "Billing").unwrap();
        assert!(billing.children.is_empty());
    }

    #[test]
    fn test_navigation_section_scope() {
        let site = site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            page("billing/payments", "Payments"),
            page("billing/invoicing", "Invoicing"),
        ]);

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
        assert_eq!(parent.section, Section::root(Namespace::default()));

        // Should show billing's children
        assert_eq!(nav.items.len(), 2);
        let titles: Vec<_> = nav.items.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Payments"));
        assert!(titles.contains(&"Invoicing"));
    }

    #[test]
    fn test_navigation_nested_sections() {
        let site = site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            section("billing/payments", "Payments", "system"),
            page("billing/payments/api", "API"),
        ]);

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
    fn test_section_location_page_is_section() {
        let site = site(&[section("billing", "Billing", "domain")]);

        let (section_ref, subpath) = site.section_location("billing");

        assert_eq!(section_ref, "domain:default/billing");
        assert_eq!(subpath, "");
    }

    #[test]
    fn test_section_location_page_inside_section() {
        let site = site(&[
            section("billing", "Billing", "domain"),
            page("billing/payments", "Payments"),
        ]);

        let (section_ref, subpath) = site.section_location("billing/payments");

        assert_eq!(section_ref, "domain:default/billing");
        assert_eq!(subpath, "payments");
    }

    #[test]
    fn test_section_location_page_deeply_nested() {
        let site = site(&[
            section("billing", "Billing", "domain"),
            section("billing/payments", "Payments", "system"),
            page("billing/payments/api", "API"),
        ]);

        // API page belongs to the nearest ancestor section (payments); subpath is
        // relative to THAT section, not the outer billing domain.
        let (section_ref, subpath) = site.section_location("billing/payments/api");

        assert_eq!(section_ref, "system:default/payments");
        assert_eq!(subpath, "api");
    }

    #[test]
    fn test_section_location_page_not_in_section() {
        let site = site(&[page("guide", "Guide")]);

        let (section_ref, subpath) = site.section_location("guide");

        // Falls back to implicit root section; subpath is the full page path.
        assert_eq!(section_ref, Section::root(Namespace::default()).to_string());
        assert_eq!(subpath, "guide");
    }

    #[test]
    fn test_section_location_root_index_page() {
        let site = site(&[page("", "Home")]);

        // Empty page path: implicit root, and "full page path" == "" == empty subpath.
        let (section_ref, subpath) = site.section_location("");

        assert_eq!(section_ref, Section::root(Namespace::default()).to_string());
        assert_eq!(subpath, "");
    }

    // list_sections tests

    fn nested_sections_site() -> SiteState {
        // root (no kind) -> billing (domain) -> payments (system) -> api (page)
        site(&[
            page("", "Home"),
            section("billing", "Billing", "domain"),
            section("billing/payments", "Payments", "system"),
            page("billing/payments/api", "API"),
        ])
    }

    #[test]
    fn list_sections_includes_root_and_all_sections_sorted_by_path() {
        let site = nested_sections_site();
        let entries = site.list_sections();

        // root + billing + payments
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["", "billing", "billing/payments"]);

        let refs: Vec<&str> = entries.iter().map(|e| e.section_ref.as_str()).collect();
        assert_eq!(
            refs,
            vec![
                "section:default/root",
                "domain:default/billing",
                "system:default/payments"
            ]
        );
    }

    #[test]
    fn list_sections_root_entry_has_no_ancestors() {
        let site = nested_sections_site();
        let entries = site.list_sections();
        let root = entries.iter().find(|e| e.path.is_empty()).unwrap();
        assert_eq!(root.section_ref, "section:default/root");
        assert!(root.ancestors.is_empty());
    }

    #[test]
    fn list_sections_ancestors_are_nearest_first_root_last() {
        let site = nested_sections_site();
        let entries = site.list_sections();

        let billing = entries.iter().find(|e| e.path == "billing").unwrap();
        assert_eq!(billing.ancestors, vec!["section:default/root".to_owned()]);

        let payments = entries
            .iter()
            .find(|e| e.path == "billing/payments")
            .unwrap();
        assert_eq!(
            payments.ancestors,
            vec![
                "domain:default/billing".to_owned(),
                "section:default/root".to_owned()
            ]
        );
    }

    #[test]
    fn list_sections_ancestry_skips_non_section_intermediate() {
        // billing (domain) -> sub (plain directory, NOT a section) -> deep
        // (system). deep's ancestry must skip the non-section "sub" and be
        // [billing, root] — exercising the sections.get(parent) skip branch.
        let site = site(&[
            section("billing", "Billing", "domain"),
            page("billing/sub", "Sub"), // no kind -> not a section
            section("billing/sub/deep", "Deep", "system"),
        ]);
        let entries = site.list_sections();

        let deep = entries
            .iter()
            .find(|e| e.path == "billing/sub/deep")
            .unwrap();
        assert_eq!(deep.section_ref, "system:default/deep");
        assert_eq!(
            deep.ancestors,
            vec![
                "domain:default/billing".to_owned(),
                "section:default/root".to_owned()
            ]
        );
    }

    #[test]
    fn list_sections_root_ref_honors_custom_root_namespace() {
        // A site whose root declares `namespace: payments` yields
        // `section:payments/root`, not a hardcoded-default `section:default/root`.
        // list_sections and section_location must agree, so a consumer can key
        // root-level pages off one canonical ref instead of a brittle constant.
        // (issue #567 follow-up.)
        let mut builder = SiteStateBuilder::new().root_namespace("payments".parse().unwrap());
        builder.add_page(
            Page {
                title: "Guide".to_owned(),
                path: "guide".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            Some("payments".parse().unwrap()),
        );
        let site = builder.build();
        let entries = site.list_sections();

        let root = entries.iter().find(|e| e.path.is_empty()).unwrap();
        assert_eq!(root.section_ref, "section:payments/root");
        // section_location of a root-level page resolves to the same custom-ns root.
        assert_eq!(site.section_location("guide").0, "section:payments/root");
    }

    #[test]
    fn list_sections_honors_explicit_root_kind() {
        let site = site(&[
            section("", "Home", "component"),
            section("billing", "Billing", "domain"),
        ]);
        let entries = site.list_sections();

        let root = entries.iter().find(|e| e.path.is_empty()).unwrap();
        assert_eq!(root.section_ref, "component:default/root");

        // The explicit root is still the universal ancestor of top-level sections.
        let billing = entries.iter().find(|e| e.path == "billing").unwrap();
        assert_eq!(billing.ancestors, vec!["component:default/root".to_owned()]);
    }

    #[test]
    fn list_sections_root_ref_derives_namespace_from_root_page() {
        // Unlike `list_sections_root_ref_honors_custom_root_namespace`, this
        // goes through the production-only path: no `root_namespace()`
        // builder override, just a root page carrying an explicit namespace.
        // `SiteStateBuilder::build()` must derive the root section's
        // namespace from that page.
        let state = site(&[page("", "Home").ns("payments".parse().unwrap())]);
        let entries = state.list_sections();

        let root = entries.iter().find(|e| e.path.is_empty()).unwrap();
        assert_eq!(root.section_ref, "section:payments/root");
    }

    // list_pages tests

    #[test]
    fn list_pages_includes_every_page_with_title_and_key() {
        // root (Home) -> billing (domain) -> billing/payments (system)
        //   -> billing/payments/api (page, no kind)
        let site = nested_sections_site();
        let pages = site.list_pages();

        // One entry per page (4 pages: root, billing, payments, api).
        assert_eq!(pages.len(), 4);

        // Root page: keyed under the root section ref with an empty subpath.
        let root = pages.iter().find(|p| p.title == "Home").unwrap();
        assert_eq!(root.section_ref, "section:default/root");
        assert_eq!(root.subpath, "");

        // A page that is itself a section root keys under its own section,
        // empty subpath (matches section_location).
        let billing = pages.iter().find(|p| p.title == "Billing").unwrap();
        assert_eq!(billing.section_ref, "domain:default/billing");
        assert_eq!(billing.subpath, "");

        // A page nested inside a section keys under that section with a
        // section-relative subpath.
        let api = pages.iter().find(|p| p.title == "API").unwrap();
        assert_eq!(api.section_ref, "system:default/payments");
        assert_eq!(api.subpath, "api");
    }

    #[test]
    fn list_pages_page_outside_any_section_keys_under_root_with_full_path() {
        // root -> guide (no kind, not a section)
        let site = site(&[page("", "Home"), page("guide", "Guide")]);
        let pages = site.list_pages();

        let guide = pages.iter().find(|p| p.title == "Guide").unwrap();
        assert_eq!(guide.section_ref, "section:default/root");
        assert_eq!(guide.subpath, "guide");
    }

    /// A directory container with no `index.md` — "Guides", a virtual page
    /// (`has_content == false`) — holding one real page, "Intro".
    fn virtual_directory_site() -> SiteState {
        site(&[dir("guides", "Guides"), page("guides/intro", "Intro")])
    }

    #[test]
    fn list_pages_includes_virtual_pages() {
        // A virtual page is still a page with a title and a key.
        let pages = virtual_directory_site().list_pages();

        let dir = pages.iter().find(|p| p.title == "Guides").unwrap();
        assert_eq!(dir.section_ref, "section:default/root");
        assert_eq!(dir.subpath, "guides");
    }

    #[test]
    fn list_pages_keys_round_trip_through_page_path_for() {
        // Every (section_ref, subpath) list_pages emits must reverse-map
        // (via page_path_for) back to a real page path — i.e. it agrees with
        // section_location / page_path_for.
        let site = nested_sections_site();
        let pages = site.list_pages();

        for page in &pages {
            let path = site
                .page_path_for(&page.section_ref, &page.subpath)
                .unwrap_or_else(|| panic!("no path for {page:?}"));
            assert!(
                site.get_page(&path).is_some(),
                "round-tripped path {path:?} is not a real page"
            );
        }
    }

    #[test]
    fn list_pages_sorted_by_section_ref_then_subpath() {
        let site = nested_sections_site();
        let pages = site.list_pages();

        let keys: Vec<(String, String)> = pages
            .iter()
            .map(|p| (p.section_ref.clone(), p.subpath.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn list_pages_order_is_total_when_two_sections_share_a_ref() {
        // A section's name is its last path segment, so `a/billing` and
        // `b/billing` — both `kind: domain` — collapse to the single ref
        // `domain:default/billing`. Their pages then share a
        // `(section_ref, subpath)` key, and sorting on that key alone would
        // leave their relative order unspecified. `path` breaks the tie.
        let mut fixture = vec![page("", "Home")];
        for dir in ["b", "a"] {
            fixture.push(section(
                format!("{dir}/billing"),
                format!("{dir} Billing"),
                "domain",
            ));
            fixture.push(page(
                format!("{dir}/billing/overview"),
                format!("{dir} Overview"),
            ));
        }
        let site = site(&fixture);

        let pages = site.list_pages();

        // Both directories really did collapse to one ref — otherwise this test
        // would pass without ever exercising a tie.
        let colliding: Vec<&PageEntry> = pages
            .iter()
            .filter(|p| p.section_ref == "domain:default/billing")
            .collect();
        assert_eq!(colliding.len(), 4);

        // Tied keys are ordered by `path`, which is unique per page.
        let paths: Vec<&str> = colliding.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "a/billing",
                "b/billing",
                "a/billing/overview",
                "b/billing/overview",
            ]
        );
    }

    #[test]
    fn list_pages_anchors_are_innermost_first_root_last_with_relative_subpaths() {
        let site = nested_sections_site();
        let pages = site.list_pages();

        let api = pages.iter().find(|p| p.title == "API").unwrap();
        assert_eq!(
            api.anchors,
            vec![
                SectionAnchor {
                    section_ref: "system:default/payments".to_owned(),
                    subpath: "api".to_owned(),
                },
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: "payments/api".to_owned(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "billing/payments/api".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn list_pages_first_anchor_is_the_entry_identity_and_last_is_the_root() {
        let site = nested_sections_site();
        let pages = site.list_pages();
        assert!(!pages.is_empty());

        for page in &pages {
            let first = page.anchors.first().expect("every page has a root anchor");
            assert_eq!(first.section_ref, page.section_ref);
            assert_eq!(first.subpath, page.subpath);

            // The last anchor is the root section, whose subpath is the site path.
            let last = page.anchors.last().expect("every page has a root anchor");
            assert_eq!(last.section_ref, "section:default/root");
            assert_eq!(last.subpath, page.path);
        }
    }

    #[test]
    fn list_pages_carries_the_site_path() {
        let site = nested_sections_site();
        let pages = site.list_pages();

        let api = pages.iter().find(|p| p.title == "API").unwrap();
        assert_eq!(api.path, "billing/payments/api");

        // The root page's path is empty, like its subpath.
        let root = pages.iter().find(|p| p.title == "Home").unwrap();
        assert_eq!(root.path, "");
    }

    #[test]
    fn list_pages_page_outside_any_section_has_a_single_root_anchor() {
        let site = site(&[page("", "Home"), page("guide", "Guide")]);

        let pages = site.list_pages();
        let guide = pages.iter().find(|p| p.title == "Guide").unwrap();
        assert_eq!(
            guide.anchors,
            vec![SectionAnchor {
                section_ref: "section:default/root".to_owned(),
                subpath: "guide".to_owned(),
            }]
        );
    }

    #[test]
    fn list_pages_marks_virtual_directory_pages_as_content_free() {
        let pages = virtual_directory_site().list_pages();

        // The directory container has no body to render...
        let dir = pages.iter().find(|p| p.title == "Guides").unwrap();
        assert!(!dir.has_content);
        // ...while the real page inside it does.
        let intro = pages.iter().find(|p| p.title == "Intro").unwrap();
        assert!(intro.has_content);
    }

    #[test]
    fn test_section_location_multi_segment_page_not_in_section() {
        let site = site(&[page("guide", "Guide"), page("guide/overview", "Overview")]);

        let (section_ref, subpath) = site.section_location("guide/overview");

        // Implicit-root fallback returns the *full* multi-segment path (with the
        // separator), not just the last segment — the case the single-segment and
        // root-index tests can't distinguish.
        assert_eq!(section_ref, Section::root(Namespace::default()).to_string());
        assert_eq!(subpath, "guide/overview");
    }

    #[test]
    fn test_section_location_root_is_named_section() {
        let site = site(&[section("", "Home", "domain")]);

        // Root index page that is itself an explicit section: the direct-match
        // arm (page_path == section scope "") returns an empty subpath, not the
        // implicit-root fallback.
        let (section_ref, subpath) = site.section_location("");

        assert_eq!(section_ref, "domain:default/root");
        assert_eq!(subpath, "");
    }

    #[test]
    fn test_navigation_invalid_scope_returns_empty() {
        let site = site(&[page("guide", "Guide")]);

        let nav = site.navigation("nonexistent");

        // Should return empty navigation for invalid scope
        assert!(nav.items.is_empty());
        assert!(nav.scope.is_none());
        assert!(nav.parent_scope.is_none());
    }

    // Content filtering tests

    #[test]
    fn test_navigation_excludes_virtual_pages_without_content() {
        let site = site(&[
            page("", "Home"),
            // Virtual page (no content) with no children
            dir("empty", "Empty Section"),
            // Real page
            page("guide", "Guide"),
        ]);

        let nav = site.navigation("");

        // Only the real page should be in navigation
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Guide");
    }

    #[test]
    fn test_navigation_includes_virtual_pages_with_content_in_subtree() {
        let site = site(&[
            page("", "Home"),
            // Virtual page (no content) but has a child with content
            dir("section", "Section"),
            // Real child page
            page("section/child", "Child"),
        ]);

        let nav = site.navigation("");

        // Section should be included because it has a child with content
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Section");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Child");
    }

    #[test]
    fn test_navigation_filters_nested_virtual_pages_without_content() {
        let site = site(&[
            page("", "Home"),
            // Virtual page with content
            dir("section", "Section"),
            // Empty virtual child (should be filtered)
            dir("section/empty", "Empty Child"),
            // Real child page
            page("section/real", "Real Child"),
        ]);

        let nav = site.navigation("");

        // Section should be included, but only the real child
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Section");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Real Child");
    }

    #[test]
    fn test_navigation_filters_content() {
        let site = site(&[
            page("", "Home"),
            // Section with type
            section("billing", "Billing", "domain"),
            // Empty virtual child (should be filtered)
            dir("billing/empty", "Empty"),
            // Real child
            page("billing/payments", "Payments"),
        ]);

        let nav = site.navigation("billing");

        // Only real child should be in scoped navigation
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "Payments");
    }

    // Implicit root section tests

    #[test]
    fn sections_contains_implicit_root_when_none_declared() {
        let site = site(&[page("", "Home"), page("guide", "Guide")]);

        let sections = site.sections();

        // Should have implicit root section
        let root = sections
            .get("")
            .expect("implicit root section should exist");
        assert_eq!(*root, Section::root(Namespace::default()));
    }

    #[test]
    fn sections_preserves_explicit_root_over_implicit_one() {
        let site = site(&[section("", "Home", "component")]);

        let sections = site.sections();

        let root = sections
            .get("")
            .expect("explicit root section should exist");
        assert_eq!(root.kind, "component");
        assert_eq!(root.name, "root");
    }

    #[test]
    fn sections_contains_implicit_root_alongside_nested_sections() {
        let site = site(&[page("", "Home"), section("billing", "Billing", "domain")]);

        let sections = site.sections();

        // Should have both implicit root and explicit nested section
        let root = sections
            .get("")
            .expect("implicit root section should exist");
        assert_eq!(*root, Section::root(Namespace::default()));

        let billing = sections
            .get("billing")
            .expect("explicit section should exist");
        assert_eq!(billing.kind, "domain");
        assert_eq!(billing.name, "billing");
    }

    #[test]
    fn sections_find_by_ref_resolves_implicit_root() {
        let site = site(&[page("", "Home")]);

        let sections = site.sections();

        let root_ref = Section::root(Namespace::default()).to_string();
        assert_eq!(sections.find_by_ref(&root_ref), Some(""));
    }

    #[test]
    fn test_root_section_uses_root_name() {
        let section = Section {
            name: Section::ROOT_NAME.to_owned(),
            kind: "component".to_owned(),
            namespace: Namespace::default(),
        };

        assert_eq!(section.kind, "component");
        assert_eq!(section.name, "root");
    }

    #[test]
    fn cached_site_state_deserializes_old_section_without_namespace_field() {
        // Cache entries written before the `namespace` field existed contain
        // sections like {"kind":"domain","name":"billing"} with no namespace
        // key. The serde default on Section::namespace must fill in "default"
        // so an upgrade without a cache-version bump still loads the cache
        // instead of silently turning every reload into a full storage scan.
        let json = r#"{
            "pages": [],
            "children": [],
            "parents": [],
            "roots": [],
            "sections": {"billing": {"kind": "domain", "name": "billing"}}
        }"#;
        let cached: CachedSiteState = serde_json::from_str(json).unwrap();
        let section = cached.sections.get("billing").expect("billing exists");
        assert_eq!(section.kind, "domain");
        assert_eq!(section.namespace, "default");
        assert_eq!(section.name, "billing");
    }

    #[test]
    fn add_page_with_namespace_builds_namespaced_section() {
        let site = site(&[section("billing", "Billing", "domain").ns("payments".parse().unwrap())]);
        assert_eq!(
            site.section_location("billing").0,
            "domain:payments/billing"
        );
    }

    // The reorder tests below build a raw `SiteStateBuilder` instead of the
    // `site()` DSL: `site()` builds and finalizes a `SiteState` in one call,
    // but these tests need `reorder_children_at` on the still-mutable builder
    // before `.build()`.

    #[test]
    fn test_navigation_respects_page_order() {
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
                title: "Advanced".to_owned(),
                path: "advanced".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Config".to_owned(),
                path: "config".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Getting Started".to_owned(),
                path: "getting-started".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );

        // Reorder: getting-started first, then config; advanced is unlisted
        let order = vec!["getting-started".to_owned(), "config".to_owned()];
        builder.reorder_children_at("", &order);

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
                title: "C".to_owned(),
                path: "c".to_owned(),
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

        // Only list "b" — "a" and "c" sorted alphabetically after
        builder.reorder_children_at("", &["b".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "b");
        assert_eq!(nav.items[1].path, "a"); // alphabetical
        assert_eq!(nav.items[2].path, "c"); // alphabetical
    }

    #[test]
    fn test_reorder_all_children_listed() {
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
                title: "B".to_owned(),
                path: "b".to_owned(),
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
            None,
            None,
        );

        // All children listed — no unlisted remainder
        builder.reorder_children_at("", &["a".to_owned(), "b".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "a");
        assert_eq!(nav.items[1].path, "b");
    }

    #[test]
    fn test_reorder_skips_section_slugs() {
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
        builder.add_page(
            Page {
                title: "Domain".to_owned(),
                path: "domain".to_owned(),
                has_content: true,
                ..Default::default()
            },
            Some("domain"),
            None,
        );

        // "domain" is a section — should be skipped, order unchanged
        builder.reorder_children_at("", &["domain".to_owned(), "guide".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        // guide is listed (non-section), domain stays in unlisted position
        assert_eq!(nav.items[0].path, "guide");
        assert_eq!(nav.items[1].path, "domain");
    }

    #[test]
    fn test_reorder_skips_missing_slugs() {
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
                title: "A".to_owned(),
                path: "a".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );

        // "nonexistent" is not a child — should be skipped
        builder.reorder_children_at("", &["nonexistent".to_owned(), "a".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "a");
    }

    #[test]
    fn test_reorder_skips_duplicate_slugs() {
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

        // "a" listed twice — second occurrence ignored
        builder.reorder_children_at("", &["a".to_owned(), "b".to_owned(), "a".to_owned()]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items.len(), 2);
        assert_eq!(nav.items[0].path, "a");
        assert_eq!(nav.items[1].path, "b");
    }

    #[test]
    fn test_reorder_no_pages_field_keeps_original_order() {
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
                title: "B".to_owned(),
                path: "b".to_owned(),
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
            None,
            None,
        );

        // Empty slugs = no reorder
        builder.reorder_children_at("", &[]);

        let site = builder.build();
        let nav = site.navigation("");
        assert_eq!(nav.items[0].path, "b"); // original insertion order
        assert_eq!(nav.items[1].path, "a");
    }

    #[test]
    fn test_reorder_nested_directory() {
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
                title: "Guides".to_owned(),
                path: "guides".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Advanced".to_owned(),
                path: "guides/advanced".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );
        builder.add_page(
            Page {
                title: "Getting Started".to_owned(),
                path: "guides/getting-started".to_owned(),
                has_content: true,
                ..Default::default()
            },
            None,
            None,
        );

        let order = vec!["getting-started".to_owned(), "advanced".to_owned()];
        builder.reorder_children_at("guides", &order);

        let site = builder.build();
        let nav = site.navigation("");
        let guides_nav = &nav.items[0];
        assert_eq!(guides_nav.children[0].path, "guides/getting-started");
        assert_eq!(guides_nav.children[1].path, "guides/advanced");
    }

    // ---- resolution fingerprint ----

    // These helpers build a raw `Page`/`SiteStateBuilder` instead of the
    // `site()`/`page()` DSL: they need `description` and `is_dir`, which the
    // DSL's constructors don't expose, and they return the raw `u64`
    // fingerprint rather than an assembled `SiteState`.

    fn fingerprint_page(path: &str, title: &str, desc: Option<&str>, has_content: bool) -> Page {
        Page {
            title: title.to_owned(),
            path: path.to_owned(),
            has_content,
            description: desc.map(str::to_owned),
            origin: None,
            pages: None,
            is_dir: true,
        }
    }

    /// Build a single-root-page state and return its fingerprint.
    fn fingerprint_of(page: Page, kind: Option<&str>, ns: Namespace) -> u64 {
        let mut b = SiteStateBuilder::new();
        b.add_page(page, kind, Some(ns));
        b.build().resolution_fingerprint()
    }

    #[test]
    fn fingerprint_changes_on_title() {
        let a = fingerprint_of(
            fingerprint_page("g", "Guide", None, true),
            None,
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("g", "Guide X", None, true),
            None,
            Namespace::default(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_on_description() {
        let a = fingerprint_of(
            fingerprint_page("g", "Guide", None, true),
            None,
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("g", "Guide", Some("d"), true),
            None,
            Namespace::default(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_on_has_content() {
        let a = fingerprint_of(
            fingerprint_page("g", "Guide", None, true),
            None,
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("g", "Guide", None, false),
            None,
            Namespace::default(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_on_section_kind_flip() {
        // The C4 get_entity path resolves an !include entity via
        // `.find(|(_, s)| s.kind == entity_type)`, so a kind change re-targets
        // which entity a diagram include resolves to.
        let a = fingerprint_of(
            fingerprint_page("billing", "Billing", None, true),
            Some("domain"),
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("billing", "Billing", None, true),
            Some("system"),
            Namespace::default(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_on_section_namespace() {
        let a = fingerprint_of(
            fingerprint_page("billing", "Billing", None, true),
            Some("domain"),
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("billing", "Billing", None, true),
            Some("domain"),
            "payments".parse().unwrap(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_on_root_namespace() {
        // No pages and no explicit sections, so `root_namespace` is the only
        // differing input — directly guards the `root_namespace.hash(...)` line
        // (the other tests vary the namespace via a section instead).
        let make = |ns: Namespace| {
            SiteStateBuilder::new()
                .root_namespace(ns)
                .build()
                .resolution_fingerprint()
        };
        assert_ne!(
            make(Namespace::default()),
            make("payments".parse().unwrap())
        );
    }

    #[test]
    fn fingerprint_stable_across_excluded_fields() {
        // `origin` and `pages` are excluded from the fingerprint.
        let base = fingerprint_of(
            fingerprint_page("g", "Guide", None, true),
            None,
            Namespace::default(),
        );

        let mut p_origin = fingerprint_page("g", "Guide", None, true);
        p_origin.origin = Some("docs".to_owned());
        let with_origin = fingerprint_of(p_origin, None, Namespace::default());

        let mut p_pages = fingerprint_page("g", "Guide", None, true);
        p_pages.pages = Some(vec!["x".to_owned()]);
        let with_pages = fingerprint_of(p_pages, None, Namespace::default());

        assert_eq!(base, with_origin);
        assert_eq!(base, with_pages);
    }

    #[test]
    fn fingerprint_changes_on_path() {
        let a = fingerprint_of(
            fingerprint_page("g", "Guide", None, true),
            None,
            Namespace::default(),
        );
        let b = fingerprint_of(
            fingerprint_page("h", "Guide", None, true),
            None,
            Namespace::default(),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_stable_across_structure_cache_rebuild() {
        let mut b = SiteStateBuilder::new();
        b.add_page(
            fingerprint_page("billing", "Billing", Some("desc"), true),
            Some("domain"),
            None,
        );
        let state = b.build();

        // Round-trip through the on-disk structure-cache representation.
        let json = serde_json::to_string(&CachedSiteStateRef::from(&state)).unwrap();
        let cached: CachedSiteState = serde_json::from_str(&json).unwrap();
        let rebuilt: SiteState = cached.into();

        assert_eq!(
            state.resolution_fingerprint(),
            rebuilt.resolution_fingerprint()
        );
    }

    #[test]
    fn explicit_root_section_survives_cache_roundtrip() {
        // A root page declared with a section kind registers an explicit "" root.
        // Reloading from cache must keep it: `new`'s implicit-root insert
        // (`entry("").or_insert_with`) must not replace it with the synthetic
        // `section:default/root`. (`fingerprint_stable_across_structure_cache_rebuild`
        // already covers the no-explicit-root case.)
        let fresh = site(&[section("", "Home", "component")]);

        let json = serde_json::to_string(&CachedSiteStateRef::from(&fresh)).unwrap();
        let cached: CachedSiteState = serde_json::from_str(&json).unwrap();
        let reloaded: SiteState = cached.into();

        // The explicit root (kind "component") is preserved, not overwritten by
        // the synthetic root (kind "section").
        let root = reloaded.sections().get("").expect("root section present");
        assert_eq!(root.kind, "component");
        assert_eq!(root.name, Section::ROOT_NAME);
        // The fingerprint also stays stable across the round-trip.
        assert_eq!(
            fresh.resolution_fingerprint(),
            reloaded.resolution_fingerprint()
        );
    }

    #[test]
    fn page_path_for_inverts_section_root_page() {
        let site = site(&[section("billing", "Billing", "domain")]);

        let (section_ref, subpath) = site.section_location("billing");
        assert_eq!(
            site.page_path_for(&section_ref, &subpath),
            Some("billing".to_owned())
        );
    }

    #[test]
    fn page_path_for_inverts_page_inside_section() {
        let site = site(&[
            section("billing", "Billing", "domain"),
            page("billing/payments", "Payments"),
        ]);

        let (section_ref, subpath) = site.section_location("billing/payments");
        assert_eq!(
            site.page_path_for(&section_ref, &subpath),
            Some("billing/payments".to_owned())
        );
    }

    #[test]
    fn page_path_for_inverts_deeply_nested_page() {
        let site = site(&[
            section("billing", "Billing", "domain"),
            section("billing/payments", "Payments", "system"),
            page("billing/payments/api", "API"),
        ]);

        let (section_ref, subpath) = site.section_location("billing/payments/api");
        assert_eq!(
            site.page_path_for(&section_ref, &subpath),
            Some("billing/payments/api".to_owned())
        );
    }

    #[test]
    fn page_path_for_inverts_page_not_in_section() {
        // Load-bearing case: a page in no explicit section keys on the IMPLICIT
        // root ref. It round-trips only because `SiteStateBuilder::build` (like
        // the live site) constructs the map with `Sections::with_implicit_root`,
        // so `find_by_ref("section:default/root")` resolves to the "" scope.
        let site = site(&[page("guide", "Guide")]);

        let (section_ref, subpath) = site.section_location("guide");
        assert_eq!(
            site.page_path_for(&section_ref, &subpath),
            Some("guide".to_owned())
        );
    }

    #[test]
    fn page_path_for_inverts_root_index_page() {
        let site = site(&[page("", "Home")]);

        let (section_ref, subpath) = site.section_location("");
        assert_eq!(
            site.page_path_for(&section_ref, &subpath),
            Some(String::new())
        );
    }

    #[test]
    fn page_path_for_unknown_ref_is_none() {
        let site = SiteStateBuilder::new().build();
        assert_eq!(site.page_path_for("domain:default/nope", "api"), None);
    }
}
