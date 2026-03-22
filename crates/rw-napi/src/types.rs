use napi_derive::napi;
use rw_site::Section;
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
    pub name: String,
}

impl From<Section> for SectionResponse {
    fn from(s: Section) -> Self {
        Self {
            kind: s.kind,
            name: s.name,
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
pub struct NavigationResponse {
    pub items: Vec<NavItemResponse>,
    pub scope: Option<ScopeInfoResponse>,
    #[napi(js_name = "parentScope")]
    pub parent_scope: Option<ScopeInfoResponse>,
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
    pub section_ref: Option<String>,
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
}
