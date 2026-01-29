//! Sections API endpoint.
//!
//! Returns the list of all sections defined in the site.
//! A section is created when a page has a `type` set in its metadata.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::handlers::to_url_path;
use crate::state::AppState;

/// Response for GET /api/sections.
#[derive(Serialize)]
pub(crate) struct SectionsResponse {
    /// List of sections.
    sections: Vec<SectionResponse>,
}

/// Section item for JSON response.
#[derive(Serialize)]
struct SectionResponse {
    /// Section title.
    title: String,
    /// Section path (with leading slash for frontend).
    path: String,
    /// Section type (from metadata `type` field).
    #[serde(rename = "type")]
    section_type: String,
}

/// Handle GET /api/sections.
pub(crate) async fn get_sections(State(state): State<Arc<AppState>>) -> Json<SectionsResponse> {
    let sections = state
        .site
        .sections()
        .into_iter()
        .map(|s| SectionResponse {
            title: s.title,
            path: to_url_path(&s.path),
            section_type: s.section_type,
        })
        .collect();

    Json(SectionsResponse { sections })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_response_serialization() {
        let response = SectionsResponse {
            sections: vec![SectionResponse {
                title: "My Domain".to_string(),
                path: "/domain-a".to_string(),
                section_type: "domain".to_string(),
            }],
        };

        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["sections"][0]["title"], "My Domain");
        assert_eq!(json["sections"][0]["path"], "/domain-a");
        assert_eq!(json["sections"][0]["type"], "domain");
    }
}
