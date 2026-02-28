mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Utc};

use napi::Result;
use napi_derive::napi;
use rw_cache::NullCache;
use rw_site::{NavItem, PageRendererConfig, ScopeInfo, Site};
use rw_storage_fs::FsStorage;

use crate::types::{
    BreadcrumbResponse, NavItemResponse, NavigationResponse, PageMetaResponse,
    PageResponse as NapiPageResponse, ScopeInfoResponse, TocEntryResponse,
};

/// Convert internal path (no leading slash) to URL path (with leading slash).
fn to_url_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        format!("/{path}")
    }
}

fn convert_nav_item(item: NavItem) -> NavItemResponse {
    NavItemResponse {
        title: item.title,
        path: to_url_path(&item.path),
        section_type: item.section_type,
        children: if item.children.is_empty() {
            None
        } else {
            Some(item.children.into_iter().map(convert_nav_item).collect())
        },
    }
}

fn convert_scope_info(info: ScopeInfo) -> ScopeInfoResponse {
    ScopeInfoResponse {
        // ScopeInfo.path already has leading slash
        path: info.path,
        title: info.title,
        section_type: info.section_type,
    }
}

#[napi]
pub struct RwSite {
    site: Arc<Site>,
}

// napi-rs requires owned types for JavaScript bindings.
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn create_site(
    docs_dir: String,
    kroki_url: Option<String>,
    link_prefix: Option<String>,
) -> Result<RwSite> {
    let storage = Arc::new(FsStorage::new(PathBuf::from(&docs_dir)));
    let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
    let config = PageRendererConfig {
        extract_title: true,
        kroki_url,
        link_prefix,
        ..Default::default()
    };
    let site = Arc::new(Site::new(storage, cache, config));
    Ok(RwSite { site })
}

#[napi]
#[allow(clippy::needless_pass_by_value)]
impl RwSite {
    #[napi]
    pub fn get_navigation(&self, scope: Option<String>) -> NavigationResponse {
        let scope_path = scope.as_deref().unwrap_or("");
        let nav = self.site.navigation(scope_path);
        NavigationResponse {
            items: nav.items.into_iter().map(convert_nav_item).collect(),
            scope: nav.scope.map(convert_scope_info),
            parent_scope: nav.parent_scope.map(convert_scope_info),
        }
    }

    #[napi]
    pub async fn render_page(&self, path: String) -> Result<NapiPageResponse> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || build_page_response(&site, &path))
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub fn reload(&self) {
        self.site.invalidate();
    }
}

fn build_page_response(site: &Site, path: &str) -> Result<NapiPageResponse> {
    let result = site
        .render(path)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let source_mtime = UNIX_EPOCH + Duration::from_secs_f64(result.source_mtime);
    let last_modified: DateTime<Utc> = source_mtime.into();
    let last_modified = last_modified.to_rfc3339();
    let navigation_scope = site.get_navigation_scope(path);

    let (description, page_type, vars) = if let Some(ref meta) = result.metadata {
        (
            meta.description.clone(),
            meta.page_type.clone(),
            if meta.vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&meta.vars).unwrap_or_default())
            },
        )
    } else {
        (None, None, None)
    };

    Ok(NapiPageResponse {
        meta: PageMetaResponse {
            title: result.title,
            path: to_url_path(path),
            source_file: if result.has_content {
                path.to_owned()
            } else {
                String::new()
            },
            last_modified,
            description,
            page_type,
            vars,
            navigation_scope,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(|b| BreadcrumbResponse {
                title: b.title,
                path: to_url_path(&b.path),
            })
            .collect(),
        toc: result
            .toc
            .iter()
            .map(|t| TocEntryResponse {
                level: u32::from(t.level),
                title: t.title.clone(),
                id: t.id.clone(),
            })
            .collect(),
        content: result.html,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_url_path_empty_returns_root() {
        assert_eq!(to_url_path(""), "/");
    }

    #[test]
    fn to_url_path_adds_leading_slash() {
        assert_eq!(to_url_path("guide"), "/guide");
        assert_eq!(to_url_path("guide/setup"), "/guide/setup");
    }

    #[test]
    fn convert_nav_item_leaf() {
        let item = NavItem {
            title: "Getting Started".to_owned(),
            path: "guide/start".to_owned(),
            section_type: None,
            children: vec![],
        };
        let result = convert_nav_item(item);
        assert_eq!(result.title, "Getting Started");
        assert_eq!(result.path, "/guide/start");
        assert_eq!(result.section_type, None);
        assert!(result.children.is_none());
    }

    #[test]
    fn convert_nav_item_root_path() {
        let item = NavItem {
            title: "Home".to_owned(),
            path: String::new(),
            section_type: None,
            children: vec![],
        };
        let result = convert_nav_item(item);
        assert_eq!(result.path, "/");
    }

    #[test]
    fn convert_nav_item_with_children() {
        let item = NavItem {
            title: "Guides".to_owned(),
            path: "guides".to_owned(),
            section_type: Some("guide".to_owned()),
            children: vec![NavItem {
                title: "Setup".to_owned(),
                path: "guides/setup".to_owned(),
                section_type: None,
                children: vec![],
            }],
        };
        let result = convert_nav_item(item);
        assert_eq!(result.section_type.as_deref(), Some("guide"));
        let children = result.children.as_ref().expect("should have children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "/guides/setup");
    }

    #[test]
    fn convert_scope_info_preserves_fields() {
        let info = ScopeInfo {
            path: "/domains".to_owned(),
            title: "Domains".to_owned(),
            section_type: "domain".to_owned(),
        };
        let result = convert_scope_info(info);
        assert_eq!(result.path, "/domains");
        assert_eq!(result.title, "Domains");
        assert_eq!(result.section_type, "domain");
    }
}
