mod attrs;
mod types;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use napi::Result;
use napi_derive::napi;
use rw_cache::{Cache, NullCache};
use rw_cache_s3::S3Cache;
use rw_config::Config;
use rw_error::error_chain;
use rw_site::{PageEntry, PageRendererConfig, ScopeInfo, SectionAnchor, SectionEntry, Site, api};
use rw_storage::{Storage, StorageErrorKind, file_content_type, mtime_to_datetime};
use rw_storage_fs::{FsStorage, MtimeSource};
use rw_storage_s3::{S3Config, S3Storage};
use sha2::{Digest, Sha256};

use crate::types::{
    BreadcrumbResponse, FileResponse, NavItemResponse, NavigationResponse, PageEntryResponse,
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

fn convert_nav_item(item: api::NavItemResponse) -> NavItemResponse {
    NavItemResponse {
        title: item.title,
        path: item.path,
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

fn convert_section_ancestry(
    map: BTreeMap<String, Vec<SectionAnchor>>,
) -> BTreeMap<String, Vec<SectionAnchorResponse>> {
    map.into_iter()
        .map(|(key, anchors)| (key, anchors.into_iter().map(Into::into).collect()))
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

    let kroki_url = config.diagrams.and_then(|d| d.kroki_url);
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
            let storage = S3Storage::new(s3_config, shared_runtime());

            let cache: Arc<dyn Cache> = Arc::new(S3Cache::new(
                storage.client().clone(),
                storage.runtime_handle(),
                storage.config().bucket.clone(),
                storage.config().base_prefix(),
                env!("CARGO_PKG_VERSION"),
            ));

            let renderer_config = PageRendererConfig {
                kroki_url,
                ..PageRendererConfig::default()
            };
            (Arc::new(storage), renderer_config, cache)
        } else if let Some(project_dir) = config.project_dir {
            let project_path = PathBuf::from(&project_dir);
            // Root config at the caller's directory. `Config::load(None, ..)`
            // would discover `rw.toml` by walking up from the Node process's
            // cwd, which is unrelated to the site the host asked for.
            let rw_config = Config::load_from_dir(&project_path, None)
                // Not "failed to load rw.toml": `load_from_dir` also rejects a
                // `projectDir` that does not exist, where there is no rw.toml
                // to have failed on.
                .map_err(|e| {
                    napi::Error::from_reason(format!(
                        "Failed to load configuration: {}",
                        error_chain(&e)
                    ))
                })?;

            let mtime_source = match config.mtime_source {
                None | Some(types::MtimeSource::Filesystem) => MtimeSource::Filesystem,
                Some(types::MtimeSource::Git) => MtimeSource::Git,
            };
            let storage = Arc::new(
                FsStorage::new(
                    rw_config.project_dir.clone(),
                    rw_config.docs.source_dir.clone(),
                )
                .with_mtime_source(mtime_source),
            );
            let renderer_config = PageRendererConfig {
                kroki_url: kroki_url.or(rw_config.diagrams.kroki_url),
                include_dirs: rw_config.diagrams.include_dirs,
                project_dir: Some(rw_config.project_dir),
            };
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

/// The read methods take `PageMeta.path` (`/guide`) as well as `guide`.
fn site_path(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

// By value so it fits `map_err` directly.
#[allow(clippy::needless_pass_by_value)]
fn js_error(err: impl Error) -> napi::Error {
    napi::Error::from_reason(error_chain(&err))
}

/// Runs `f` off the JS thread: site reads block on storage.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
}

impl RwSite {
    async fn with_site<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Site) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let site = Arc::clone(&self.site);
        blocking(move || f(&site)).await
    }
}

#[napi]
#[allow(clippy::needless_pass_by_value)]
impl RwSite {
    #[napi]
    pub async fn get_navigation(&self, section_ref: Option<String>) -> Result<NavigationResponse> {
        self.with_site(move |site| {
            let nav = api::NavigationResponse::from(
                site.navigation(section_ref.as_deref()).map_err(js_error)?,
            );
            Ok(NavigationResponse {
                items: nav.items.into_iter().map(convert_nav_item).collect(),
                scope: convert_scope_info(nav.scope),
                parent_scope: nav.parent_scope.map(convert_scope_info),
                section_ancestry: convert_section_ancestry(nav.section_ancestry),
            })
        })
        .await
    }

    #[napi]
    pub async fn list_sections(&self) -> Result<Vec<SectionEntryResponse>> {
        self.with_site(|site| {
            let sections = site.list_sections().map_err(js_error)?;
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
    }

    #[napi]
    pub async fn list_pages(&self) -> Result<Vec<PageEntryResponse>> {
        self.with_site(|site| {
            let pages = site.list_pages().map_err(js_error)?;
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
                    last_modified: mtime_to_datetime(p.mtime).to_rfc3339(),
                })
                .collect())
        })
        .await
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
    /// - The returned path has no leading slash (`"guide"`). The read methods
    ///   also take the `/`-prefixed form of `PageMeta.path`.
    /// - Two sections declaring the same ref is a site misconfiguration rw only
    ///   warns about, and a ref is resolved here to whichever of them sorts
    ///   first. An ambiguous identity therefore resolves to a path, but not
    ///   necessarily to the page the identity came from.
    #[napi]
    pub async fn page_path_for(
        &self,
        section_ref: String,
        subpath: String,
    ) -> Result<Option<String>> {
        self.with_site(move |site| {
            site.try_page_path_for(&section_ref, &subpath)
                .map_err(js_error)
        })
        .await
    }

    #[napi]
    pub async fn render_page(&self, path: String) -> Result<PageResponse> {
        self.with_site(move |site| build_page_response(site, site_path(&path)))
            .await
    }

    #[napi]
    pub async fn render_search_document(
        &self,
        path: String,
    ) -> Result<Option<SearchDocumentResponse>> {
        self.with_site(move |site| {
            let doc = site
                .render_search_document(site_path(&path))
                .map_err(js_error)?;
            Ok(doc.map(|doc| SearchDocumentResponse {
                title: doc.title,
                text: doc.text,
            }))
        })
        .await
    }

    /// Returns a page's markdown source, exactly as authored.
    ///
    /// Nothing is rendered or transformed — this is a single storage read.
    ///
    /// Resolves to `null` for a virtual page — a directory that declares
    /// metadata (a `meta.yaml`) but has no markdown of its own. Rejects if no
    /// page exists at `path`.
    #[napi]
    pub async fn get_page_markdown(&self, path: String) -> Result<Option<PageMarkdownResponse>> {
        self.with_site(move |site| {
            let markdown = site.page_markdown(site_path(&path)).map_err(js_error)?;
            Ok(markdown.map(|markdown| PageMarkdownResponse { markdown }))
        })
        .await
    }

    /// Reads a project file that a page's image names, for the host's
    /// `files/` route beside the viewer's other API routes.
    ///
    /// `path` is the project-relative path after `files/`, percent-decoded:
    /// `docs/img/flow.png`.
    ///
    /// Resolves to `null` when the file is missing or refused: off the image
    /// allowlist, outside the project, larger than 32 MiB, or for S3, not in
    /// the published bundle. Rejects when storage fails, for example when S3
    /// is unreachable.
    #[napi]
    pub async fn get_file(&self, path: String) -> Result<Option<FileResponse>> {
        self.with_site(move |site| read_file_response(site, &path))
            .await
    }

    #[napi]
    pub async fn reload(&self, force: Option<bool>) -> Result<bool> {
        let force = force.unwrap_or(false);
        self.with_site(move |site| site.reload(force).map_err(js_error))
            .await
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
    blocking(move || Ok(rw_renderer::render_comment_body(&markdown))).await
}

fn read_file_response(site: &Site, path: &str) -> Result<Option<FileResponse>> {
    let Some(content_type) = file_content_type(path) else {
        return Ok(None);
    };
    let bytes = match site.read_file(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind == StorageErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(js_error(err)),
    };
    Ok(Some(FileResponse {
        etag: format!("\"{}\"", hex::encode(Sha256::digest(&bytes))),
        content: bytes.into(),
        content_type: content_type.to_owned(),
    }))
}

fn build_page_response(site: &Site, path: &str) -> Result<PageResponse> {
    let result = site.render(path).map_err(js_error)?;
    let attrs = (!result.meta.attrs.is_empty()).then(|| attrs::Attrs(Arc::clone(&result.meta)));
    let page = api::PageResponse::from(result);
    let meta = page.meta;

    Ok(PageResponse {
        meta: PageMetaResponse {
            title: meta.title,
            path: meta.path,
            source_file: meta.source_file,
            last_modified: meta.last_modified,
            description: meta.description,
            page_kind: meta.kind,
            attrs,
            section_ref: meta.section_ref,
            subpath: meta.subpath,
            document_id: meta.document_id,
        },
        breadcrumbs: page
            .breadcrumbs
            .into_iter()
            .map(|b| BreadcrumbResponse {
                title: b.title,
                path: b.path,
                section_ref: b.section_ref,
                subpath: b.subpath,
            })
            .collect(),
        toc: page
            .toc
            .into_iter()
            .map(|t| TocEntryResponse {
                level: u32::from(t.level),
                title: t.title,
                id: t.id,
            })
            .collect(),
        content: page.content,
        section_ancestry: convert_section_ancestry(page.section_ancestry),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io;

    use rw_site::{NavItem, RenderError};
    use rw_storage::{MockStorage, StorageError, StorageErrorKind};

    #[test]
    fn render_storage_error_message_names_each_cause_once() {
        let err = RenderError::Storage(
            StorageError::new(StorageErrorKind::Unavailable)
                .with_backend("S3")
                .with_source(io::Error::other("connection reset")),
        );

        assert_eq!(
            js_error(err).reason,
            "Storage error: [S3] Unavailable: connection reset"
        );
    }

    #[test]
    fn render_not_found_message_is_its_display() {
        let err = RenderError::PageNotFound("guide".to_owned());

        assert_eq!(js_error(err).reason, "Page not found: guide");
    }

    #[test]
    fn build_page_response_forwards_resolved_meta_from_render_result() {
        let storage = MockStorage::new()
            .with_document_meta(
                "billing",
                "title: Resolved Billing\nkind: domain\ndescription: Money stuff",
            )
            .with_content("billing", "# Body H1\n\nOverview.")
            .with_mtime("billing", 1000.0);
        let site = Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        );

        let response = build_page_response(&site, "billing").expect("page should render");

        assert_eq!(response.meta.title, "Resolved Billing");
        assert_eq!(response.meta.path, "/billing");
        assert_eq!(response.meta.source_file, "billing");
        assert_eq!(response.meta.last_modified, "1970-01-01T00:16:40+00:00");
        assert_eq!(response.meta.description.as_deref(), Some("Money stuff"));
        assert_eq!(response.meta.page_kind.as_deref(), Some("domain"));
        assert_eq!(response.meta.section_ref, "domain:default/billing");
        assert_eq!(response.meta.subpath, "");
    }

    #[test]
    fn convert_nav_item_leaf() {
        let item = NavItem {
            title: "Getting Started".to_owned(),
            path: "/guide/start".to_owned(),
            section: None,
            children: vec![],
        };
        let result = convert_nav_item(item.into());
        assert_eq!(result.title, "Getting Started");
        assert_eq!(result.path, "/guide/start");
        assert!(result.section.is_none());
        assert!(result.children.is_none());
    }

    #[test]
    fn convert_nav_item_with_children() {
        let item = NavItem {
            title: "Guides".to_owned(),
            path: "/guides".to_owned(),
            section: Some(rw_site::Section {
                kind: "guide".to_owned(),
                namespace: rw_site::Namespace::default(),
                name: "guides".to_owned(),
            }),
            children: vec![NavItem {
                title: "Setup".to_owned(),
                path: "/guides/setup".to_owned(),
                section: None,
                children: vec![],
            }],
        };
        let result = convert_nav_item(item.into());
        assert_eq!(result.section.as_ref().unwrap().kind, "guide");
        let children = result.children.as_ref().expect("should have children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "/guides/setup");
    }

    fn file_site() -> Site {
        let storage = MockStorage::new()
            .with_project_file("docs/img/flow.png", b"png".as_slice())
            .with_project_file(".env", b"SECRET=1".as_slice())
            .with_project_file("docs/flow.puml", b"@startuml\n@enduml\n".as_slice())
            .with_unreadable_project_file("docs/locked.png", StorageErrorKind::Unavailable);
        Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        )
    }

    #[test]
    fn read_file_response_serves_the_bytes_with_their_type_and_hash() {
        let file = read_file_response(&file_site(), "docs/img/flow.png")
            .unwrap()
            .expect("file should be found");

        assert_eq!(&*file.content, b"png");
        assert_eq!(file.content_type, "image/png");
        assert_eq!(
            file.etag,
            "\"8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c\""
        );
    }

    #[test]
    fn read_file_response_is_none_for_missing_and_refused_files() {
        let site = file_site();

        for path in [
            "docs/img/missing.png",
            ".env",
            "docs/flow.puml",
            "docs/index.md",
            "",
        ] {
            assert!(read_file_response(&site, path).unwrap().is_none(), "{path}");
        }
    }

    #[test]
    fn read_file_response_rejects_a_storage_failure() {
        let err = read_file_response(&file_site(), "docs/locked.png")
            .err()
            .expect("a storage failure should reject");

        assert!(err.reason.contains("Unavailable"), "{}", err.reason);
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
