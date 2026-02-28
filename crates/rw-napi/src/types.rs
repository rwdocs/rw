use napi_derive::napi;
use serde_json::Value;

#[napi(object)]
pub struct SiteConfig {
    #[napi(js_name = "projectDir")]
    pub project_dir: Option<String>,
    pub s3: Option<S3Config>,
    #[napi(js_name = "linkPrefix")]
    pub link_prefix: Option<String>,
}

#[napi(object)]
pub struct S3Config {
    pub bucket: String,
    pub entity: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    #[napi(js_name = "bucketRootPath")]
    pub bucket_root_path: Option<String>,
}

#[napi(object)]
pub struct NavItemResponse {
    pub title: String,
    pub path: String,
    #[napi(js_name = "sectionType")]
    pub section_type: Option<String>,
    pub children: Option<Vec<NavItemResponse>>,
}

#[napi(object)]
pub struct ScopeInfoResponse {
    pub path: String,
    pub title: String,
    #[napi(js_name = "type")]
    pub section_type: String,
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
    #[napi(js_name = "type")]
    pub page_type: Option<String>,
    pub vars: Option<Value>,
    #[napi(js_name = "navigationScope")]
    pub navigation_scope: String,
}

#[napi(object)]
pub struct BreadcrumbResponse {
    pub title: String,
    pub path: String,
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
