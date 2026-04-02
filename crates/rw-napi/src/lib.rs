mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Utc};

use napi::Result;
use napi_derive::napi;
use rw_cache::{Cache, NullCache};
use rw_cache_s3::S3Cache;
use rw_config::Config;
use rw_site::{NavItem, PageRendererConfig, ScopeInfo, Site};
use rw_storage::Storage;
use rw_storage_fs::FsStorage;
use rw_storage_s3::{S3Config, S3Storage};

use crate::types::{
    BreadcrumbResponse, DiagramsConfig, NavItemResponse, NavigationResponse, PageMetaResponse,
    PageResponse, ScopeInfoResponse, SectionResponse, SiteConfig, TocEntryResponse,
};

/// Format an error with its full source chain.
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        msg.push_str(": ");
        msg.push_str(&cause.to_string());
        source = cause.source();
    }
    msg
}

fn apply_diagrams_config(
    renderer_config: &mut PageRendererConfig,
    diagrams: Option<&DiagramsConfig>,
) {
    if let Some(diagrams) = diagrams {
        if let Some(ref url) = diagrams.kroki_url {
            renderer_config.kroki_url = Some(url.clone());
        }
        if let Some(dpi) = diagrams.dpi {
            renderer_config.dpi = dpi;
        }
    }
}

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
        section: item.section.map(SectionResponse::from),
        children: if item.children.is_empty() {
            None
        } else {
            Some(item.children.into_iter().map(convert_nav_item).collect())
        },
    }
}

fn convert_scope_info(info: ScopeInfo) -> ScopeInfoResponse {
    ScopeInfoResponse {
        path: info.path,
        title: info.title,
        section: info.section.into(),
    }
}

#[napi]
pub struct RwSite {
    site: Arc<Site>,
}

// napi-rs requires owned types for JavaScript bindings.
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn create_site(config: SiteConfig) -> Result<RwSite> {
    if config.project_dir.is_some() && config.s3.is_some() {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            "Cannot specify both projectDir and s3",
        ));
    }

    let (storage, renderer_config, cache): (
        Arc<dyn Storage>,
        PageRendererConfig,
        Arc<dyn Cache>,
    ) = if let Some(s3) = config.s3 {
        if s3.access_key_id.is_some() != s3.secret_access_key.is_some() {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                "s3.accessKeyId and s3.secretAccessKey must be provided together",
            ));
        }

        let s3_config = S3Config {
            bucket: s3.bucket,
            prefix: s3.entity,
            region: s3.region.unwrap_or_else(|| "us-east-1".to_owned()),
            endpoint: s3.endpoint,
            bucket_root_path: s3.bucket_root_path,
            access_key_id: s3.access_key_id,
            secret_access_key: s3.secret_access_key,
        };
        let storage = S3Storage::new(s3_config).map_err(|e| {
            napi::Error::from_reason(format!("Failed to create S3 storage: {}", error_chain(&e)))
        })?;

        let cache: Arc<dyn Cache> = Arc::new(S3Cache::new(
            storage.client().clone(),
            storage.runtime_handle(),
            storage.config().bucket.clone(),
            storage.config().base_prefix(),
        ));

        let mut renderer_config = PageRendererConfig {
            extract_title: true,
            ..Default::default()
        };
        apply_diagrams_config(&mut renderer_config, config.diagrams.as_ref());
        (Arc::new(storage), renderer_config, cache)
    } else if let Some(project_dir) = config.project_dir {
        let project_path = PathBuf::from(&project_dir);
        let config_path = project_path.join("rw.toml");
        let config_file = if config_path.exists() {
            Some(config_path.as_path())
        } else {
            None
        };
        let rw_config = Config::load(config_file, None)
            .map_err(|e| napi::Error::from_reason(format!("Failed to load rw.toml: {e}")))?;

        let storage = Arc::new(FsStorage::with_meta_filename(
            rw_config.docs_resolved.source_dir.clone(),
            &rw_config.metadata.name,
        ));
        let mut renderer_config = PageRendererConfig {
            extract_title: true,
            kroki_url: rw_config.diagrams_resolved.kroki_url,
            include_dirs: rw_config.diagrams_resolved.include_dirs,
            dpi: rw_config.diagrams_resolved.dpi,
            ..Default::default()
        };
        apply_diagrams_config(&mut renderer_config, config.diagrams.as_ref());
        (storage, renderer_config, Arc::new(NullCache))
    } else {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            "Either projectDir or s3 must be provided",
        ));
    };

    let site = Arc::new(Site::new(storage, cache, renderer_config));
    Ok(RwSite { site })
}

#[napi]
#[allow(clippy::needless_pass_by_value)]
impl RwSite {
    #[napi(js_name = "getNavigation")]
    pub fn get_navigation(&self, section_ref: Option<String>) -> Result<NavigationResponse> {
        let nav = self
            .site
            .navigation(section_ref.as_deref())
            .map_err(|e| napi::Error::from_reason(e.display_chain()))?;
        Ok(NavigationResponse {
            items: nav.items.into_iter().map(convert_nav_item).collect(),
            scope: nav.scope.map(convert_scope_info),
            parent_scope: nav.parent_scope.map(convert_scope_info),
        })
    }

    #[napi]
    pub async fn render_page(&self, path: String) -> Result<PageResponse> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || build_page_response(&site, &path))
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn reload(&self, force: Option<bool>) -> Result<bool> {
        let site = Arc::clone(&self.site);
        let force = force.unwrap_or(false);
        tokio::task::spawn_blocking(move || {
            site.reload(force)
                .map_err(|e| napi::Error::from_reason(e.display_chain()))
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }
}

fn build_page_response(site: &Site, path: &str) -> Result<PageResponse> {
    let result = site
        .render(path)
        .map_err(|e| napi::Error::from_reason(error_chain(&e)))?;

    let source_mtime = UNIX_EPOCH + Duration::from_secs_f64(result.source_mtime);
    let last_modified: DateTime<Utc> = source_mtime.into();
    let last_modified = last_modified.to_rfc3339();
    let section_ref = site
        .get_section_ref(path)
        .map_err(|e| napi::Error::from_reason(e.display_chain()))?;

    let (description, page_kind, vars) = if let Some(ref meta) = result.metadata {
        (
            meta.description.clone(),
            meta.page_kind.clone(),
            if meta.vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&meta.vars).unwrap_or_default())
            },
        )
    } else {
        (None, None, None)
    };

    Ok(PageResponse {
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
            page_kind,
            vars,
            section_ref,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(|b| BreadcrumbResponse {
                title: b.title,
                path: to_url_path(&b.path),
                section: b.section.map(SectionResponse::from),
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
            section: None,
            children: vec![],
        };
        let result = convert_nav_item(item);
        assert_eq!(result.title, "Getting Started");
        assert_eq!(result.path, "/guide/start");
        assert!(result.section.is_none());
        assert!(result.children.is_none());
    }

    #[test]
    fn convert_nav_item_root_path() {
        let item = NavItem {
            title: "Home".to_owned(),
            path: String::new(),
            section: None,
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
            section: Some(rw_site::Section {
                kind: "guide".to_owned(),
                name: "guides".to_owned(),
            }),
            children: vec![NavItem {
                title: "Setup".to_owned(),
                path: "guides/setup".to_owned(),
                section: None,
                children: vec![],
            }],
        };
        let result = convert_nav_item(item);
        assert_eq!(result.section.as_ref().unwrap().kind, "guide");
        let children = result.children.as_ref().expect("should have children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "/guides/setup");
    }

    #[test]
    fn apply_diagrams_config_sets_kroki_url() {
        let diagrams = Some(DiagramsConfig {
            kroki_url: Some("https://kroki.io".to_owned()),
            dpi: None,
        });
        let mut renderer_config = PageRendererConfig::default();
        apply_diagrams_config(&mut renderer_config, diagrams.as_ref());
        assert_eq!(
            renderer_config.kroki_url,
            Some("https://kroki.io".to_owned())
        );
        assert_eq!(renderer_config.dpi, 192); // default unchanged
    }

    #[test]
    fn apply_diagrams_config_sets_dpi() {
        let diagrams = Some(DiagramsConfig {
            kroki_url: None,
            dpi: Some(96),
        });
        let mut renderer_config = PageRendererConfig::default();
        apply_diagrams_config(&mut renderer_config, diagrams.as_ref());
        assert!(renderer_config.kroki_url.is_none());
        assert_eq!(renderer_config.dpi, 96);
    }

    #[test]
    fn apply_diagrams_config_none_is_noop() {
        let mut renderer_config = PageRendererConfig::default();
        let before_kroki = renderer_config.kroki_url.clone();
        let before_dpi = renderer_config.dpi;
        apply_diagrams_config(&mut renderer_config, None);
        assert_eq!(renderer_config.kroki_url, before_kroki);
        assert_eq!(renderer_config.dpi, before_dpi);
    }

    #[test]
    fn convert_scope_info_preserves_fields() {
        let info = ScopeInfo {
            path: "/domains".to_owned(),
            title: "Domains".to_owned(),
            section: rw_site::Section {
                kind: "domain".to_owned(),
                name: "domains".to_owned(),
            },
        };
        let result = convert_scope_info(info);
        assert_eq!(result.path, "/domains");
        assert_eq!(result.title, "Domains");
        assert_eq!(result.section.kind, "domain");
        assert_eq!(result.section.name, "domains");
    }
}
