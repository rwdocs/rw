//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use rw_site::{NavItem, ScopeInfo, Section, SectionAnchor, to_url_path};
use serde::{Deserialize, Serialize};

use crate::error::HandlerError;
use crate::handlers::with_site;
use crate::state::AppState;

/// Query parameters for GET /_api/navigation.
#[derive(Deserialize)]
pub(crate) struct NavigationQuery {
    /// Section ref (optional). If provided, returns navigation for that section.
    /// Format: "kind:namespace/name" (e.g., "domain:default/billing", "domain:payments/billing").
    #[serde(rename = "sectionRef")]
    section_ref: Option<String>,
}

/// Response for GET /_api/navigation.
#[derive(Serialize)]
pub(crate) struct NavigationResponse {
    /// Navigation tree items.
    items: Vec<NavItemResponse>,
    /// Current scope info (implicit root section at root, explicit section otherwise).
    scope: ScopeInfo,
    /// Parent scope for back navigation (null only at root).
    #[serde(rename = "parentScope", skip_serializing_if = "Option::is_none")]
    parent_scope: Option<ScopeInfo>,
    /// Ancestry chains for the sections reachable from this navigation view,
    /// keyed by section ref; each chain starts with the section itself (empty
    /// subpath), then its ancestors, root last. Omitted when empty.
    #[serde(rename = "sectionAncestry", skip_serializing_if = "BTreeMap::is_empty")]
    section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
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

/// Handle GET /_api/navigation.
pub(crate) async fn get_navigation(
    Query(query): Query<NavigationQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<NavigationResponse>, HandlerError> {
    let scoped_nav = with_site(&state.site, move |site| {
        site.navigation(query.section_ref.as_deref())
            .map_err(HandlerError::from)
    })
    .await?;

    Ok(Json(NavigationResponse {
        items: scoped_nav
            .items
            .into_iter()
            .map(NavItemResponse::from)
            .collect(),
        scope: scoped_nav.scope,
        parent_scope: scoped_nav.parent_scope,
        section_ancestry: scoped_nav.section_ancestry,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::http::StatusCode;
    use rw_site::Namespace;
    use rw_storage::MockStorage;
    use serde_json::json;

    use crate::testing::TestServer;

    fn root_scope() -> ScopeInfo {
        ScopeInfo {
            path: "/".to_owned(),
            title: "Home".to_owned(),
            section: Section::root(Namespace::default()),
        }
    }

    #[tokio::test]
    async fn scoped_navigation_serializes_scope_and_parent_scope() {
        let storage = MockStorage::new()
            .with_document_meta("billing", "title: Billing\nkind: domain")
            .with_content("billing", "# Billing")
            .with_file("billing/overview", "Overview", "# Overview");
        let server = TestServer::with_storage(storage).await;

        let resp = server
            .get("/_api/navigation?sectionRef=domain:default/billing")
            .await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        let json = resp.json();
        assert_eq!(
            json["scope"],
            json!({
                "path": "/billing",
                "title": "Billing",
                "section": {"kind": "domain", "namespace": "default", "name": "billing"},
            })
        );
        assert_eq!(
            json["parentScope"],
            json!({
                "path": "/",
                "title": "Home",
                "section": {"kind": "section", "namespace": "default", "name": "root"},
            })
        );
    }

    #[tokio::test]
    async fn repeated_navigation_requests_return_identical_bodies() {
        let storage = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
            .into_iter()
            .fold(MockStorage::new(), |storage, name| {
                storage
                    .with_document_meta(name, "kind: domain")
                    .with_content(name, "# Domain")
            });
        let server = TestServer::with_storage(storage).await;

        let first = server.get("/_api/navigation").await.text();
        for _ in 0..20 {
            assert_eq!(server.get("/_api/navigation").await.text(), first);
        }
    }

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
            scope: root_scope(),
            parent_scope: None,
            section_ancestry: BTreeMap::new(),
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["items"][0]["title"], "Guide");
        // JSON serialization should have leading slash
        assert_eq!(json["items"][0]["path"], "/guide");
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
            scope: ScopeInfo {
                path: "/domains/billing".to_owned(),
                title: "Billing".to_owned(),
                section: Section {
                    kind: "domain".to_owned(),
                    namespace: "payments".parse().unwrap(),
                    name: "billing".to_owned(),
                },
            },
            parent_scope: None,
            section_ancestry: BTreeMap::new(),
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["scope"]["path"], "/domains/billing");
        assert_eq!(json["scope"]["title"], "Billing");
        assert_eq!(json["scope"]["section"]["kind"], "domain");
        assert_eq!(json["scope"]["section"]["namespace"], "payments");
        assert_eq!(json["scope"]["section"]["name"], "billing");
        assert!(json.get("parentScope").is_none());
    }

    #[test]
    fn test_navigation_response_serializes_section_ancestry() {
        let mut ancestry = BTreeMap::new();
        ancestry.insert(
            "domain:default/billing".to_owned(),
            vec![
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: "overview".to_owned(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: String::new(),
                },
            ],
        );
        let response = NavigationResponse {
            items: vec![],
            scope: root_scope(),
            parent_scope: None,
            section_ancestry: ancestry,
        };

        let json = serde_json::to_value(&response).unwrap();

        let chain = &json["sectionAncestry"]["domain:default/billing"];
        assert_eq!(chain[0]["sectionRef"], "domain:default/billing");
        assert_eq!(chain[0]["subpath"], "overview");
        assert_eq!(chain[1]["sectionRef"], "section:default/root");
        assert_eq!(chain[1]["subpath"], "");
    }

    #[test]
    fn test_navigation_response_omits_empty_section_ancestry() {
        let response = NavigationResponse {
            items: vec![],
            scope: root_scope(),
            parent_scope: None,
            section_ancestry: BTreeMap::new(),
        };

        let json = serde_json::to_value(&response).unwrap();

        assert!(json.get("sectionAncestry").is_none());
    }
}
