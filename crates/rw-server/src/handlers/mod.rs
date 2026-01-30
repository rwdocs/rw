//! HTTP request handlers.

pub(crate) mod config;
pub(crate) mod navigation;
pub(crate) mod pages;

/// Convert internal path (without leading slash) to URL path (with leading slash).
///
/// The site stores paths without leading slashes (e.g., "guide", "domain/page", "" for root),
/// but the frontend expects URL paths with leading slashes (e.g., "/guide", "/domain/page", "/").
pub(crate) fn to_url_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    }
}
