//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use rw_site::{NavItem, ScopeInfo};
use serde::{Deserialize, Serialize};

use crate::handlers::to_url_path;
use crate::state::AppState;

/// Query parameters for GET /api/navigation.
#[derive(Deserialize)]
pub(crate) struct NavigationQuery {
    /// Scope path (optional). If provided, returns navigation for that section.
    /// Path should be without leading slash (e.g., "domains/billing").
    scope: Option<String>,
}

/// Response for GET /api/navigation.
#[derive(Serialize)]
pub(crate) struct NavigationResponse {
    /// Navigation tree items.
    items: Vec<NavItemResponse>,
    /// Current scope info (null at root).
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<ScopeInfoResponse>,
    /// Parent scope for back navigation (null at root or if no parent section).
    #[serde(rename = "parentScope", skip_serializing_if = "Option::is_none")]
    parent_scope: Option<ScopeInfoResponse>,
}

/// Scope info for JSON response.
#[derive(Serialize)]
struct ScopeInfoResponse {
    /// URL path (with leading slash for frontend).
    path: String,
    /// Display title.
    title: String,
    /// Section type.
    #[serde(rename = "type")]
    section_type: String,
}

impl From<ScopeInfo> for ScopeInfoResponse {
    fn from(info: ScopeInfo) -> Self {
        Self {
            // ScopeInfo.path already has leading slash
            path: info.path,
            title: info.title,
            section_type: info.section_type,
        }
    }
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
pub(crate) async fn get_navigation(
    Query(query): Query<NavigationQuery>,
    State(state): State<Arc<AppState>>,
) -> Json<NavigationResponse> {
    // Normalize scope path: remove leading slash if present
    let scope_path = query
        .scope
        .as_deref()
        .map_or("", |s| s.strip_prefix('/').unwrap_or(s));

    let scoped_nav = state.site.scoped_navigation(scope_path);

    Json(NavigationResponse {
        items: scoped_nav
            .items
            .into_iter()
            .map(NavItemResponse::from)
            .collect(),
        scope: scoped_nav.scope.map(ScopeInfoResponse::from),
        parent_scope: scoped_nav.parent_scope.map(ScopeInfoResponse::from),
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
            scope: None,
            parent_scope: None,
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["items"][0]["title"], "Guide");
        // JSON serialization should have leading slash
        assert_eq!(json["items"][0]["path"], "/guide");
        // scope and parentScope should be omitted when None
        assert!(json.get("scope").is_none());
        assert!(json.get("parentScope").is_none());
    }

    #[test]
    fn test_navigation_response_with_scope() {
        let response = NavigationResponse {
            items: vec![],
            scope: Some(ScopeInfoResponse {
                path: "/domains/billing".to_string(),
                title: "Billing".to_string(),
                section_type: "domain".to_string(),
            }),
            parent_scope: None,
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["scope"]["path"], "/domains/billing");
        assert_eq!(json["scope"]["title"], "Billing");
        assert_eq!(json["scope"]["type"], "domain");
        assert!(json.get("parentScope").is_none());
    }
}
