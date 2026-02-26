use napi_derive::napi;
use serde_json::Value;

#[napi(object)]
pub struct NavItemResponse {
    pub title: String,
    pub path: String,
    pub section_type: Option<String>,
    pub children: Vec<NavItemResponse>,
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
    pub parent_scope: Option<ScopeInfoResponse>,
}

#[napi(object)]
pub struct PageMetaResponse {
    pub title: Option<String>,
    pub path: String,
    pub source_file: String,
    pub last_modified: String,
    pub description: Option<String>,
    #[napi(js_name = "type")]
    pub page_type: Option<String>,
    pub vars: Option<Value>,
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
