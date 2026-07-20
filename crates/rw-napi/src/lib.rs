mod types;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use napi::Result;
use napi_derive::napi;
use rw_cache::{Cache, NullCache};
use rw_cache_s3::S3Cache;
use rw_config::Config;
use rw_site::{
    NavItem, PageEntry, PageRendererConfig, ScopeInfo, SectionAnchor, SectionEntry, Site,
    to_url_path,
};
use rw_storage::{Storage, mtime_to_datetime};
use rw_storage_fs::{FsStorage, MtimeSource};
use rw_storage_s3::{S3Config, S3Storage};

use crate::types::{
    BreadcrumbResponse, DiagramsConfig, NavItemResponse, NavigationResponse, PageEntryResponse,
    PageMarkdownResponse, PageMetaResponse, PageResponse, ScopeInfoResponse,
    SearchDocumentResponse, SectionAnchorResponse, SectionEntryResponse, SectionResponse,
    SiteConfig, TocEntryResponse,
};

/// Shared tokio runtime for all S3-backed storage instances.
///
/// Created on first use and lives for the process lifetime.
/// Avoids spawning a separate thread pool per `RwSite`.
fn shared_runtime() -> Arc<tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Arc<tokio::runtime::Runtime>> = OnceLock::new();
    Arc::clone(RUNTIME.get_or_init(|| {
        Arc::new(
            tokio::runtime::Runtime::new().expect("Failed to create tokio runtime for S3 storage"),
        )
    }))
}

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
    if let Some(url) = diagrams.and_then(|d| d.kroki_url.as_ref()) {
        renderer_config.kroki_url = Some(url.clone());
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

/// Convert a section-ancestry map from the site layer into its napi form,
/// mapping each [`SectionAnchor`] to a [`SectionAnchorResponse`].
fn convert_section_ancestry(
    map: HashMap<String, Vec<SectionAnchor>>,
) -> HashMap<String, Vec<SectionAnchorResponse>> {
    map.into_iter()
        .map(|(section_ref, anchors)| {
            (
                section_ref,
                anchors
                    .into_iter()
                    .map(SectionAnchorResponse::from)
                    .collect(),
            )
        })
        .collect()
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

    let (storage, renderer_config, cache): (Arc<dyn Storage>, PageRendererConfig, Arc<dyn Cache>) =
        if let Some(s3) = config.s3 {
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
            let storage = S3Storage::new(s3_config, shared_runtime()).map_err(|e| {
                napi::Error::from_reason(format!(
                    "Failed to create S3 storage: {}",
                    error_chain(&e)
                ))
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

            let mtime_source = match config.mtime_source.as_deref() {
                None | Some("filesystem") => MtimeSource::Filesystem,
                Some("git") => MtimeSource::Git,
                Some(other) => {
                    return Err(napi::Error::new(
                        napi::Status::InvalidArg,
                        format!(
                            "invalid mtimeSource {other:?} (expected \"filesystem\" or \"git\")"
                        ),
                    ));
                }
            };
            let storage = Arc::new(
                FsStorage::with_meta_filename(
                    rw_config.docs_resolved.source_dir.clone(),
                    &rw_config.metadata.name,
                )
                .with_mtime_source(mtime_source),
            );
            let mut renderer_config = PageRendererConfig {
                extract_title: true,
                kroki_url: rw_config.diagrams_resolved.kroki_url,
                include_dirs: rw_config.diagrams_resolved.include_dirs,
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
    pub async fn get_navigation(&self, section_ref: Option<String>) -> Result<NavigationResponse> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || {
            let nav = site
                .navigation(section_ref.as_deref())
                .map_err(|e| napi::Error::from_reason(e.display_chain()))?;
            Ok(NavigationResponse {
                items: nav.items.into_iter().map(convert_nav_item).collect(),
                scope: nav.scope.map(convert_scope_info),
                parent_scope: nav.parent_scope.map(convert_scope_info),
                section_ancestry: convert_section_ancestry(nav.section_ancestry),
            })
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn list_sections(&self) -> Result<Vec<SectionEntryResponse>> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || {
            let sections = site
                .list_sections()
                .map_err(|e| napi::Error::from_reason(e.display_chain()))?;
            Ok(sections
                .into_iter()
                .map(|s: SectionEntry| SectionEntryResponse {
                    section_ref: s.section_ref,
                    path: s.path,
                    ancestors: s.ancestors,
                })
                .collect())
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi(js_name = "listPages")]
    pub async fn list_pages(&self) -> Result<Vec<PageEntryResponse>> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || {
            let pages = site
                .list_pages()
                .map_err(|e| napi::Error::from_reason(e.display_chain()))?;
            Ok(pages
                .into_iter()
                .map(|p: PageEntry| PageEntryResponse {
                    section_ref: p.section_ref,
                    subpath: p.subpath,
                    path: p.path,
                    title: p.title,
                    has_content: p.has_content,
                    anchors: p
                        .anchors
                        .into_iter()
                        .map(SectionAnchorResponse::from)
                        .collect(),
                    last_modified: mtime_to_rfc3339(p.mtime),
                })
                .collect())
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    /// Resolves a page's canonical identity — the `(sectionRef, subpath)` pair
    /// `listPages()` and `PageMeta` hand out — to the path the read methods take.
    ///
    /// This is the inverse of the split every page response performs, so a host
    /// holding an identity (a search hit, a comment's key) can read the page
    /// without re-deriving section scopes itself:
    ///
    /// ```js
    /// const path = await site.pagePathFor(sectionRef, subpath);
    /// if (path === null) return notFound();
    /// const page = await site.getPageMarkdown(path);
    /// ```
    ///
    /// Resolves to `null` when **no section has that ref** — and only then. A
    /// site that cannot be loaded rejects rather than resolving to `null`, so a
    /// host mapping `null` to a 404 can't turn an unreachable storage backend
    /// into a missing page.
    ///
    /// It is not a page-existence check: a real section ref with a subpath
    /// naming no page still resolves to a well-formed path, and the read that
    /// follows rejects.
    ///
    /// - The site's root page resolves to the **empty string**, which is falsy in
    ///   JavaScript. Test for absence with `path === null`, never `if (!path)`,
    ///   or the homepage looks missing.
    /// - The returned path has **no leading slash** (`"guide"`), which is the form
    ///   `renderPage` / `renderSearchDocument` / `getPageMarkdown` expect. It is
    ///   deliberately not the `/`-prefixed form of `PageMeta.path`.
    /// - Two sections declaring the same ref is a site misconfiguration rw only
    ///   warns about, and a ref is resolved here to whichever of them sorts
    ///   first. An ambiguous identity therefore resolves to a path, but not
    ///   necessarily to the page the identity came from.
    #[napi(js_name = "pagePathFor")]
    pub async fn page_path_for(
        &self,
        section_ref: String,
        subpath: String,
    ) -> Result<Option<String>> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || {
            site.try_page_path_for(&section_ref, &subpath)
                .map_err(|e| napi::Error::from_reason(e.display_chain()))
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn render_page(&self, path: String) -> Result<PageResponse> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || build_page_response(&site, &path))
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    #[napi]
    pub async fn render_search_document(
        &self,
        path: String,
    ) -> Result<Option<SearchDocumentResponse>> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || match site.render_search_document(&path) {
            Ok(Some(doc)) => Ok(Some(SearchDocumentResponse {
                title: doc.title,
                text: doc.text,
            })),
            Ok(None) => Ok(None),
            Err(e) => Err(napi::Error::from_reason(error_chain(&e))),
        })
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    }

    /// Returns a page's markdown source, exactly as authored.
    ///
    /// Nothing is rendered or transformed — this is a single storage read.
    ///
    /// Resolves to `null` for a virtual page — a directory that declares
    /// metadata (a `meta.yaml`) but has no markdown of its own. Rejects if no
    /// page exists at `path`.
    #[napi(js_name = "getPageMarkdown")]
    pub async fn get_page_markdown(&self, path: String) -> Result<Option<PageMarkdownResponse>> {
        let site = Arc::clone(&self.site);
        tokio::task::spawn_blocking(move || match site.page_markdown(&path) {
            Ok(Some(markdown)) => Ok(Some(PageMarkdownResponse { markdown })),
            Ok(None) => Ok(None),
            Err(e) => Err(napi::Error::from_reason(error_chain(&e))),
        })
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

/// Render a comment's markdown to safe, restricted HTML.
///
/// Produces the same `bodyHtml` the bundled viewer expects, so a host that
/// stores its own comments (for example a Backstage backend plugin) can render
/// comment bodies identically to `rw serve`. Pure and stateless — no `RwSite`
/// instance is required. Raw HTML is escaped, headings are demoted, tables
/// flattened, images dropped, and links keep their `href` only for
/// `http`/`https`/`mailto` schemes. Blank input renders to an empty string.
#[napi]
#[allow(clippy::needless_pass_by_value)]
pub async fn render_comment_body(markdown: String) -> Result<String> {
    tokio::task::spawn_blocking(move || rw_renderer::render_comment_body(&markdown))
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}

/// Formats a raw mtime (seconds since the Unix epoch) as an RFC-3339 string,
/// e.g. `2026-07-09T10:35:00+00:00`. An unknown mtime is passed in as `0.0`;
/// it and any value that denotes no representable date render as the Unix
/// epoch.
fn mtime_to_rfc3339(mtime: f64) -> String {
    mtime_to_datetime(mtime).to_rfc3339()
}

fn build_page_response(site: &Site, path: &str) -> Result<PageResponse> {
    let result = site
        .render(path)
        .map_err(|e| napi::Error::from_reason(error_chain(&e)))?;

    let last_modified = mtime_to_rfc3339(result.source_mtime);
    let (section_ref, subpath) = site
        .section_location(path)
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
            subpath,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(|b| BreadcrumbResponse {
                title: b.title,
                path: to_url_path(&b.path),
                section_ref: b.section_ref,
                subpath: b.subpath,
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
        section_ancestry: convert_section_ancestry(result.section_ancestry),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
                namespace: rw_site::Namespace::default(),
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
        });
        let mut renderer_config = PageRendererConfig::default();
        apply_diagrams_config(&mut renderer_config, diagrams.as_ref());
        assert_eq!(
            renderer_config.kroki_url,
            Some("https://kroki.io".to_owned())
        );
    }

    #[test]
    fn apply_diagrams_config_none_is_noop() {
        let mut renderer_config = PageRendererConfig::default();
        let before_kroki = renderer_config.kroki_url.clone();
        apply_diagrams_config(&mut renderer_config, None);
        assert_eq!(renderer_config.kroki_url, before_kroki);
    }

    #[test]
    fn convert_scope_info_preserves_fields() {
        let info = ScopeInfo {
            path: "/domains".to_owned(),
            title: "Domains".to_owned(),
            section: rw_site::Section {
                kind: "domain".to_owned(),
                namespace: rw_site::Namespace::default(),
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
