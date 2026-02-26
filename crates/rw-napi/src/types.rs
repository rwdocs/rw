use napi_derive::napi;

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
