//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use docstage_site::{NavItem, build_navigation};
use serde::Serialize;

use crate::state::AppState;

/// Response for GET /api/navigation.
#[derive(Serialize)]
pub struct NavigationResponse {
    /// Navigation tree items.
    pub items: Vec<NavItem>,
}

/// Handle GET /api/navigation.
pub async fn get_navigation(State(state): State<Arc<AppState>>) -> Json<NavigationResponse> {
    let mut loader = state.site_loader.write().unwrap();
    let site = loader.load(true);
    let items = build_navigation(site);
    Json(NavigationResponse { items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use docstage_site::NavItem;

    #[test]
    fn test_navigation_response_serialization() {
        let response = NavigationResponse {
            items: vec![NavItem {
                title: "Guide".to_string(),
                path: "/guide".to_string(),
                children: vec![],
            }],
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["items"][0]["title"], "Guide");
        assert_eq!(json["items"][0]["path"], "/guide");
    }
}
