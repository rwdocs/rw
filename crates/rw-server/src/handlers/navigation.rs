//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use rw_site::{NavItem, ScopeInfo, Section};
use serde::{Deserialize, Serialize};

use crate::error::HandlerError;
use crate::handlers::to_url_path;
use crate::state::AppState;

/// Query parameters for GET /api/navigation.
#[derive(Deserialize)]
pub(crate) struct NavigationQuery {
    /// Section ref (optional). If provided, returns navigation for that section.
    /// Format: "kind:default/name" (e.g., "domain:default/billing").
    #[serde(rename = "sectionRef")]
    section_ref: Option<String>,
}

/// Response for GET /api/navigation.
#[derive(Serialize)]
pub(crate) struct NavigationResponse {
    /// Navigation tree items.
    items: Vec<NavItemResponse>,
    /// Current scope info (implicit root section at root, explicit section otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<ScopeInfoResponse>,
    /// Parent scope for back navigation (null only at root).
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
    /// Section identity.
    section: Section,
}

impl From<ScopeInfo> for ScopeInfoResponse {
    fn from(info: ScopeInfo) -> Self {
        Self {
            path: info.path,
            title: info.title,
            section: info.section,
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
    /// Section identity if this item's path matches a section.
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<Section>,
    /// Child navigation items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<NavItemResponse>,
}

impl From<NavItem> for NavItemResponse {
    fn from(item: NavItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
            section: item.section,
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
) -> Result<Json<NavigationResponse>, HandlerError> {
    let scoped_nav = state.site.navigation(query.section_ref.as_deref())?;

    Ok(Json(NavigationResponse {
        items: scoped_nav
            .items
            .into_iter()
            .map(NavItemResponse::from)
            .collect(),
        scope: scoped_nav.scope.map(ScopeInfoResponse::from),
        parent_scope: scoped_nav.parent_scope.map(ScopeInfoResponse::from),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_response_serialization() {
        // Create NavItem with internal path (no leading slash)
        let nav_item = NavItem {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            section: None,
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
    fn test_navigation_query_deserializes_section_ref() {
        let query: NavigationQuery =
            serde_urlencoded::from_str("sectionRef=domain:default/billing").unwrap();
        assert_eq!(query.section_ref.as_deref(), Some("domain:default/billing"));
    }

    #[test]
    fn test_navigation_response_with_scope() {
        let response = NavigationResponse {
            items: vec![],
            scope: Some(ScopeInfoResponse {
                path: "/domains/billing".to_owned(),
                title: "Billing".to_owned(),
                section: Section {
                    kind: "domain".to_owned(),
                    name: "billing".to_owned(),
                },
            }),
            parent_scope: None,
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["scope"]["path"], "/domains/billing");
        assert_eq!(json["scope"]["title"], "Billing");
        assert_eq!(json["scope"]["section"]["kind"], "domain");
        assert_eq!(json["scope"]["section"]["name"], "billing");
        assert!(json.get("parentScope").is_none());
    }
}
