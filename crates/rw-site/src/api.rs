//! The JSON bodies of the viewer API: `GET /_api/pages/{path}` and
//! `GET /_api/navigation`.
//!
//! `rw-server` serializes these as they are and `@rwdocs/core` copies them
//! into its napi objects, so both report the same values. Every path here is
//! a URL path with a leading slash.

use std::collections::BTreeMap;

use rw_renderer::TocEntry;
use rw_sections::{Section, SectionAnchor};
use serde::Serialize;

use crate::page::{BreadcrumbItem, PageRenderResult};
use crate::site_state::{NavItem, Navigation, ScopeInfo};

/// Body of `GET /_api/pages/{path}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResponse {
    pub meta: PageMeta,
    /// Ancestors from "Home" to the page's parent.
    pub breadcrumbs: Vec<BreadcrumbResponse>,
    pub toc: Vec<TocEntry>,
    /// Rendered HTML.
    pub content: String,
    /// See [`PageRenderResult::section_ancestry`].
    pub section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
}

/// The `meta` of a [`PageResponse`].
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    /// The same title navigation and breadcrumbs report for this page.
    pub title: String,
    pub path: String,
    /// The site path for a page with markdown, empty for a
    /// [virtual page](crate#virtual-pages). The homepage's site path is itself
    /// empty, so its `source_file` is empty too.
    pub source_file: String,
    /// RFC 3339; the Unix epoch when unknown.
    pub last_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The page's declared `kind`, which makes it a section root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Section ref of the page's enclosing section.
    pub section_ref: String,
    /// Page path relative to that section's root.
    pub subpath: String,
    /// The key the page's comments are filed under.
    pub document_id: String,
}

/// One crumb of a [`PageResponse`]; see [`BreadcrumbItem`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbResponse {
    pub title: String,
    pub path: String,
    /// The crumb's key into [`PageResponse::section_ancestry`].
    pub section_ref: String,
    pub subpath: String,
}

/// Body of `GET /_api/navigation`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationResponse {
    pub items: Vec<NavItemResponse>,
    pub scope: ScopeInfo,
    /// `None` only at the root scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_scope: Option<ScopeInfo>,
    /// See [`Navigation::section_ancestry`].
    pub section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
}

/// One item of a [`NavigationResponse`]; see [`NavItem`].
#[derive(Debug, Serialize)]
pub struct NavItemResponse {
    pub title: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<Section>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavItemResponse>,
}

impl From<PageRenderResult> for PageResponse {
    fn from(result: PageRenderResult) -> Self {
        Self {
            meta: result.response_meta,
            breadcrumbs: result
                .breadcrumbs
                .into_iter()
                .map(BreadcrumbResponse::from)
                .collect(),
            toc: result.toc,
            content: result.html,
            section_ancestry: result.section_ancestry,
        }
    }
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: item.path,
            section_ref: item.section_ref,
            subpath: item.subpath,
        }
    }
}

impl From<Navigation> for NavigationResponse {
    fn from(nav: Navigation) -> Self {
        Self {
            items: nav.items.into_iter().map(NavItemResponse::from).collect(),
            scope: nav.scope,
            parent_scope: nav.parent_scope,
            section_ancestry: nav.section_ancestry,
        }
    }
}

impl From<NavItem> for NavItemResponse {
    fn from(item: NavItem) -> Self {
        Self {
            title: item.title,
            path: item.path,
            section: item.section,
            children: item.children.into_iter().map(Self::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rw_cache::NullCache;
    use rw_storage::MockStorage;
    use serde_json::json;

    use super::*;
    use crate::{PageRendererConfig, Site};

    fn site() -> Site {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide")
            .with_file("guide/setup", "Setup", "# Setup")
            .with_mtime("guide/setup", 1000.0)
            .with_document_meta("billing", "title: Billing\nkind: domain")
            .with_content("billing", "# Billing");
        Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        )
    }

    #[test]
    fn navigation_items_carry_url_paths() {
        let nav = NavigationResponse::from(site().navigation(None).unwrap());

        let json = serde_json::to_value(nav).unwrap();

        assert_eq!(
            json["items"],
            json!([
                {
                    "title": "Billing",
                    "path": "/billing",
                    "section": {"kind": "domain", "namespace": "default", "name": "billing"},
                },
                {
                    "title": "Guide",
                    "path": "/guide",
                    "children": [{"title": "Setup", "path": "/guide/setup"}],
                },
            ])
        );
        assert!(json.get("parentScope").is_none());
    }

    #[test]
    fn breadcrumbs_carry_url_paths() {
        let page = PageResponse::from(site().render("guide/setup").unwrap());

        let json = serde_json::to_value(page).unwrap();

        assert_eq!(
            json["breadcrumbs"],
            json!([
                {"title": "Home", "path": "/", "sectionRef": "section:default/root", "subpath": ""},
                {
                    "title": "Guide",
                    "path": "/guide",
                    "sectionRef": "section:default/root",
                    "subpath": "guide",
                },
            ])
        );
    }
}
