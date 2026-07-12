use std::collections::HashMap;

use napi_derive::napi;
use rw_site::{Section, SectionAnchor};
use serde_json::Value;

#[napi(object)]
pub struct DiagramsConfig {
    #[napi(js_name = "krokiUrl")]
    pub kroki_url: Option<String>,
    pub dpi: Option<u32>,
}

#[napi(object)]
pub struct SiteConfig {
    #[napi(js_name = "projectDir")]
    pub project_dir: Option<String>,
    pub s3: Option<S3Config>,
    pub diagrams: Option<DiagramsConfig>,
    /// Modification-time source for a `projectDir` (filesystem) site:
    /// `"filesystem"` (default — a fast `stat`, reflecting on-disk edits) or
    /// `"git"` (commit times, matching S3-served pages, at the cost of a
    /// per-page git query). Ignored for `s3` sites (their mtimes come from the
    /// published manifest). For a `projectDir` site, an unrecognized value is
    /// rejected.
    #[napi(js_name = "mtimeSource")]
    pub mtime_source: Option<String>,
}

#[napi(object)]
pub struct S3Config {
    pub bucket: String,
    pub entity: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    #[napi(js_name = "bucketRootPath")]
    pub bucket_root_path: Option<String>,
    #[napi(js_name = "accessKeyId")]
    pub access_key_id: Option<String>,
    #[napi(js_name = "secretAccessKey")]
    pub secret_access_key: Option<String>,
}

#[napi(object)]
pub struct SectionResponse {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

impl From<Section> for SectionResponse {
    fn from(s: Section) -> Self {
        Self {
            kind: s.kind,
            namespace: s.namespace.into(),
            name: s.name,
        }
    }
}

/// One link in a section's ancestry chain: a section ref and the subpath of
/// that section's root.
#[napi(object)]
pub struct SectionAnchorResponse {
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    pub subpath: String,
}

impl From<SectionAnchor> for SectionAnchorResponse {
    fn from(a: SectionAnchor) -> Self {
        Self {
            section_ref: a.section_ref,
            subpath: a.subpath,
        }
    }
}

#[napi(object)]
pub struct NavItemResponse {
    pub title: String,
    pub path: String,
    pub section: Option<SectionResponse>,
    pub children: Option<Vec<NavItemResponse>>,
}

#[napi(object)]
pub struct ScopeInfoResponse {
    pub path: String,
    pub title: String,
    pub section: SectionResponse,
}

#[napi(object)]
pub struct SectionEntryResponse {
    /// Canonical section ref (`kind:namespace/name`). Named `sectionRef` in JS
    /// to match `PageMeta.sectionRef`.
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    /// Scope path, no leading slash (`""` for the root section).
    pub path: String,
    /// Ancestor section refs, nearest-first with the root last; excludes self.
    pub ancestors: Vec<String>,
}

#[napi(object)]
pub struct PageEntryResponse {
    /// Canonical section ref (`kind:namespace/name`). Named `sectionRef` in JS
    /// to match `PageMeta.sectionRef`. Always equal to `anchors[0].sectionRef`.
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    /// Page path relative to its section root (`""` for the section root).
    /// Always equal to `anchors[0].subpath`.
    pub subpath: String,
    /// Site path, no leading slash (`""` for the site's root page) — the form
    /// `renderPage()`, `renderSearchDocument()`, and `getPageMarkdown()` take,
    /// so a listed page can be read without mapping its `(sectionRef, subpath)`
    /// identity back through `pagePathFor()`.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Whether the page has a markdown body. `false` for a virtual directory
    /// page (a directory with no `index.md`) — it has a title and a place in the
    /// navigation, but nothing to render, so a host indexing a site should skip
    /// it rather than call `renderSearchDocument()` on it just to get `null`.
    #[napi(js_name = "hasContent")]
    pub has_content: bool,
    /// Every section enclosing this page, innermost first with the root section
    /// last, each paired with the page's path relative to *that* section.
    /// `anchors[0]` is the page's own `(sectionRef, subpath)` identity;
    /// `anchors[anchors.length - 1]` is the root section, whose subpath is
    /// `path`. Never empty.
    ///
    /// A host whose sections map to catalog entities can find the nearest
    /// enclosing entity and get a path relative to it in one pass:
    ///
    /// ```js
    /// const anchor = page.anchors.find((a) => claims.has(a.sectionRef))
    /// const viewerPath = anchor ? anchor.subpath : page.path
    /// ```
    pub anchors: Vec<SectionAnchorResponse>,
    /// Last-modified time as an RFC-3339 string (e.g.
    /// `2026-07-09T10:35:00+00:00`), matching `PageMeta.lastModified`. Sourced
    /// from the same per-page mtime `renderPage()` uses (git author-time for
    /// clean tracked files, filesystem mtime otherwise; the S3 manifest
    /// `mtimes` table for published bundles).
    ///
    /// Falls back to the Unix epoch (`1970-01-01T00:00:00+00:00`) when the mtime
    /// is unknown — a site served from a legacy S3 manifest published before
    /// per-page mtimes were recorded (republishing repopulates it), or a page
    /// with no backing markdown file (`hasContent: false`). Here "epoch" means
    /// unknown, not an actual 1970 modification.
    #[napi(js_name = "lastModified")]
    pub last_modified: String,
}

#[napi(object)]
pub struct NavigationResponse {
    pub items: Vec<NavItemResponse>,
    pub scope: Option<ScopeInfoResponse>,
    #[napi(js_name = "parentScope")]
    pub parent_scope: Option<ScopeInfoResponse>,
    /// Ancestry chains for the sections reachable from this navigation view,
    /// keyed by section ref.
    #[napi(js_name = "sectionAncestry")]
    pub section_ancestry: HashMap<String, Vec<SectionAnchorResponse>>,
}

#[napi(object)]
pub struct PageMetaResponse {
    pub title: Option<String>,
    pub path: String,
    #[napi(js_name = "sourceFile")]
    pub source_file: String,
    #[napi(js_name = "lastModified")]
    pub last_modified: String,
    pub description: Option<String>,
    #[napi(js_name = "kind")]
    pub page_kind: Option<String>,
    pub vars: Option<Value>,
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    /// Page path relative to its section root. Stable across whole-section
    /// moves, so embedding hosts can key comments on `(sectionRef, subpath)`.
    pub subpath: String,
}

#[napi(object)]
pub struct BreadcrumbResponse {
    pub title: String,
    pub path: String,
    /// Section ref of the nearest enclosing section — the crumb's key into the
    /// page response's `sectionAncestry` map.
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    /// This crumb's path relative to `sectionRef`'s scope root.
    pub subpath: String,
}

#[napi(object)]
pub struct TocEntryResponse {
    pub level: u32,
    pub title: String,
    pub id: String,
}

#[napi(object)]
pub struct PageResponse {
    pub meta: PageMetaResponse,
    pub breadcrumbs: Vec<BreadcrumbResponse>,
    pub toc: Vec<TocEntryResponse>,
    pub content: String,
    /// Ancestry chains for the sections this page is connected to, keyed by
    /// section ref.
    #[napi(js_name = "sectionAncestry")]
    pub section_ancestry: HashMap<String, Vec<SectionAnchorResponse>>,
}

#[napi(object)]
pub struct SearchDocumentResponse {
    pub title: String,
    pub text: String,
}

/// A page's markdown source, exactly as authored (frontmatter included).
#[napi(object)]
pub struct PageMarkdownResponse {
    pub markdown: String,
}
