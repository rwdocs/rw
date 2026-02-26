mod types;

use std::path::PathBuf;
use std::sync::Arc;

use napi::Result;
use napi_derive::napi;
use rw_cache::NullCache;
use rw_site::{NavItem, PageRendererConfig, ScopeInfo, Site};
use rw_storage_fs::FsStorage;

use crate::types::{NavItemResponse, NavigationResponse, ScopeInfoResponse};

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
    pub fn reload(&self) {
        self.site.invalidate();
    }
}
