mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

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
        children: item.children.into_iter().map(convert_nav_item).collect(),
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
pub fn create_site(docs_dir: String, kroki_url: Option<String>) -> Result<RwSite> {
    let storage = Arc::new(FsStorage::new(PathBuf::from(&docs_dir)));
    let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
    let config = PageRendererConfig {
        extract_title: true,
        kroki_url,
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
    pub fn render_page(&self, path: String) -> Result<NapiPageResponse> {
        let result = self
            .site
            .render(&path)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let source_mtime = UNIX_EPOCH + Duration::from_secs_f64(result.source_mtime);
        let last_modified = humantime::format_rfc3339(source_mtime).to_string();
        let navigation_scope = self.site.get_navigation_scope(&path);

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
                path: to_url_path(&path),
                source_file: if result.has_content {
                    path.clone()
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

    #[napi]
    pub fn reload(&self) {
        self.site.invalidate();
    }
}
