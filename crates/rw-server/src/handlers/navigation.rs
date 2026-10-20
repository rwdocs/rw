//! Navigation API endpoint.
//!
//! Returns the navigation tree for the documentation site.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use rw_site::api::NavigationResponse;
use serde::Deserialize;

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

    Ok(Json(NavigationResponse::from(scoped_nav)))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use rw_storage::MockStorage;
    use serde_json::json;

    use crate::testing::TestServer;

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
}
