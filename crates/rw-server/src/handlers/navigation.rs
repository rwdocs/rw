//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use rw_site::NavItem;
use serde::Serialize;

use crate::handlers::to_url_path;
use crate::state::AppState;

/// Response for GET /api/navigation.
#[derive(Serialize)]
pub(crate) struct NavigationResponse {
    /// Navigation tree items.
    items: Vec<NavItemResponse>,
}

/// Navigation item for JSON response with URL paths (leading slash).
#[derive(Serialize)]
struct NavItemResponse {
    /// Display title.
    title: String,
    /// Link target path (with leading slash for frontend).
    path: String,
    /// Section type if this item is a section root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section_type: Option<String>,
    /// Child navigation items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<NavItemResponse>,
}

impl From<NavItem> for NavItemResponse {
    fn from(item: NavItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
            section_type: item.section_type,
            children: item
                .children
                .into_iter()
                .map(NavItemResponse::from)
                .collect(),
        }
    }
}

/// Handle GET /api/navigation.
pub(crate) async fn get_navigation(State(state): State<Arc<AppState>>) -> Json<NavigationResponse> {
    let items = state.site.navigation();
    Json(NavigationResponse {
        items: items.into_iter().map(NavItemResponse::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_response_serialization() {
        // Create NavItem with internal path (no leading slash)
        let nav_item = NavItem {
            title: "Guide".to_string(),
            path: "guide".to_string(),
            section_type: None,
            children: vec![],
        };
        // Convert to NavItemResponse which adds leading slash
        let response = NavigationResponse {
            items: vec![NavItemResponse::from(nav_item)],
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["items"][0]["title"], "Guide");
        // JSON serialization should have leading slash
        assert_eq!(json["items"][0]["path"], "/guide");
    }
}
