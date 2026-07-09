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
    /// to match `PageMeta.sectionRef`.
    #[napi(js_name = "sectionRef")]
    pub section_ref: String,
    /// Page path relative to its section root (`""` for the section root).
    pub subpath: String,
    /// Display title.
    pub title: String,
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
    pub section: Option<SectionResponse>,
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
