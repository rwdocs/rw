//! HTTP request handlers.

use rw_site::Section;
use serde::Serialize;

pub(crate) mod config;
pub(crate) mod navigation;
pub(crate) mod pages;

/// Section identity for JSON response.
#[derive(Serialize)]
pub(crate) struct SectionResponse {
    kind: String,
    name: String,
}

impl From<Section> for SectionResponse {
    fn from(s: Section) -> Self {
        Self {
            kind: s.kind,
            name: s.name,
        }
    }
}

/// Convert internal path (without leading slash) to URL path (with leading slash).
///
/// The site stores paths without leading slashes (e.g., "guide", "domain/page", "" for root),
/// but the frontend expects URL paths with leading slashes (e.g., "/guide", "/domain/page", "/").
pub(crate) fn to_url_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        format!("/{path}")
    }
}
