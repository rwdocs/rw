//! Pages API endpoint.
//!
//! Handles page rendering and returns JSON responses with metadata,
//! table of contents, and HTML content.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use rw_renderer::TocEntry;
use rw_site::{BreadcrumbItem, SectionAnchor};
use serde::Serialize;

use crate::error::HandlerError;
use crate::handlers::to_url_path;
use crate::state::AppState;

/// Response for GET /_api/pages/{path}.
#[derive(Serialize)]
struct PageResponse {
    /// Page metadata.
    meta: PageMeta,
    /// Breadcrumb navigation items.
    breadcrumbs: Vec<BreadcrumbResponse>,
    /// Table of contents entries.
    toc: Vec<TocResponse>,
    /// Rendered HTML content.
    content: String,
    /// Ancestry chains for the sections this page is connected to (including the
    /// page's own section), keyed by section ref; each chain starts with the
    /// section itself (empty subpath), then its ancestors, root last. A
    /// `BTreeMap` (not the render result's `HashMap`) so keys serialize in
    /// sorted, stable order — `serde_json` emits a `HashMap` in randomized
    /// per-instance order, which would let identical ancestry hash to different
    /// `ETag`s.
    #[serde(rename = "sectionAncestry", skip_serializing_if = "BTreeMap::is_empty")]
    section_ancestry: BTreeMap<String, Vec<SectionAnchor>>,
}

/// Page metadata.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageMeta {
    /// Page title (from H1 heading or metadata).
    title: Option<String>,
    /// URL path.
    path: String,
    /// Source file path.
    source_file: String,
    /// Last modification time (ISO 8601).
    last_modified: String,
    /// Page description (from metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Page kind (from metadata, indicates a section).
    #[serde(skip_serializing_if = "Option::is_none", rename = "kind")]
    page_kind: Option<String>,
    /// Custom variables (from metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    vars: Option<serde_json::Value>,
    /// Section ref for this page's section.
    section_ref: String,
    /// Page path relative to its section root (stable across section moves).
    subpath: String,
}

/// Breadcrumb item for serialization.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BreadcrumbResponse {
    /// Display title.
    title: String,
    /// Link target path.
    path: String,
    /// Section ref of the nearest enclosing section — the crumb's key into the
    /// page response's `sectionAncestry` map.
    section_ref: String,
    /// This crumb's path relative to `section_ref`'s scope root.
    subpath: String,
}

impl From<BreadcrumbItem> for BreadcrumbResponse {
    fn from(item: BreadcrumbItem) -> Self {
        Self {
            title: item.title,
            path: to_url_path(&item.path),
            section_ref: item.section_ref,
            subpath: item.subpath,
        }
    }
}

/// Table of contents entry for serialization.
#[derive(Serialize)]
struct TocResponse {
    /// Heading level (2-6).
    level: u8,
    /// Heading text.
    title: String,
    /// Anchor ID.
    id: String,
}

impl From<&TocEntry> for TocResponse {
    fn from(entry: &TocEntry) -> Self {
        Self {
            level: entry.level,
            title: entry.title.clone(),
            id: entry.id.clone(),
        }
    }
}

/// Handle GET /_api/pages/ (root page).
pub(crate) async fn get_root_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(String::new(), state, headers)
}

/// Handle GET /_api/pages/{path}.
pub(crate) async fn get_page(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    get_page_impl(path, state, headers)
}

/// Shared implementation for page rendering.
#[allow(clippy::needless_pass_by_value)]
fn get_page_impl(
    path: String,
    state: Arc<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HandlerError> {
    // Render the page using unified Site API (path is already without leading slash)
    let result = state.site.render(&path).map_err(|e| match e {
        // A page known to the navigation tree but whose source file is missing
        // from storage (FileNotFound — e.g. deleted under a stale snapshot) is a
        // not-found, not a server error; map it to 404 like an unknown page.
        rw_site::RenderError::PageNotFound(p) | rw_site::RenderError::FileNotFound(p) => {
            HandlerError::PageNotFound(p)
        }
        rw_site::RenderError::Storage(se) => HandlerError::Storage(se),
        e @ rw_site::RenderError::Io(_) => HandlerError::Render(e),
    })?;

    // Log warnings in verbose mode
    if state.verbose && !result.warnings.is_empty() {
        for warning in &result.warnings {
            tracing::warn!(path = %path, warning = %warning, "Page render warning");
        }
    }

    // Get last modified time from render result
    let source_mtime = UNIX_EPOCH + Duration::from_secs_f64(result.source_mtime);
    let last_modified: DateTime<Utc> = source_mtime.into();

    // Build response using render result fields directly
    // Add leading slash to path for JSON response (frontend expects URLs with leading slash)
    let (description, page_kind, vars) = if let Some(ref meta) = result.metadata {
        (
            meta.description.clone(),
            meta.page_kind.clone(),
            if meta.vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&meta.vars).unwrap_or_default())
            },
        )
    } else {
        (None, None, None)
    };

    let (section_ref, subpath) = state.site.section_location(&path)?;

    let response = PageResponse {
        meta: PageMeta {
            title: result.title,
            path: to_url_path(&path),
            source_file: if result.has_content {
                path.clone()
            } else {
                String::new()
            },
            last_modified: last_modified.to_rfc3339(),
            description,
            page_kind,
            vars,
            section_ref,
            subpath,
        },
        breadcrumbs: result
            .breadcrumbs
            .into_iter()
            .map(BreadcrumbResponse::from)
            .collect(),
        toc: result.toc.iter().map(TocResponse::from).collect(),
        content: result.html,
        // HashMap -> BTreeMap; see the field doc for why key order must be stable.
        section_ancestry: result.section_ancestry.into_iter().collect(),
    };

    // Serialize once: this JSON is both the ETag input and the response body.
    // A stable ETag needs deterministic serialization: `section_ancestry` is a
    // `BTreeMap` (sorted keys), and `meta.vars` is a `serde_json::Value` whose
    // object map is `BTreeMap`-backed (sorted) as long as the `serde_json`
    // `preserve_order` feature stays off, which is the default and the current
    // build. If a dependency ever turns it on, route `vars` through a sorted
    // form too, or the ETag will vary per request for a page with ≥2 vars keys.
    let response_json =
        serde_json::to_string(&response).expect("PageResponse serialization is infallible");

    // Compute the ETag over the full response, not just the rendered HTML.
    // `sectionAncestry` is rebuilt from live sections every request, outside
    // the render cache, so hashing only `content` would let a section-identity
    // change that leaves the HTML byte-identical produce an unchanged ETag —
    // and a stale 304.
    let etag = compute_etag(&state.version, &response_json);

    // Check If-None-Match header for conditional request. This now runs after
    // the response is built, since the ETag covers the whole response.
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && if_none_match.as_bytes() == etag.as_bytes()
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    // Reuse the already-serialized JSON as the body (with an explicit
    // content-type) instead of letting `Json` serialize the response again.
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_owned()),
            (header::ETAG, etag),
            (
                header::LAST_MODIFIED,
                last_modified
                    .format("%a, %d %b %Y %H:%M:%S GMT")
                    .to_string(),
            ),
            (header::CACHE_CONTROL, "no-cache".to_owned()),
        ],
        response_json,
    )
        .into_response())
}

/// Compute `ETag` from version and content.
///
/// The page handler passes the fully serialized page response as `content`
/// (not just the rendered HTML), so a change to any response field — including
/// `sectionAncestry`, which is rebuilt from live sections outside the render
/// cache — invalidates the `ETag`.
///
/// Hashes `version:content` with the stdlib [`DefaultHasher`] into a 64-bit
/// fingerprint (rendered as 16 hex chars) — sufficient for cache invalidation
/// with negligible collision probability. An `ETag` is a change-detection
/// token, not a security token, so a non-cryptographic hash is the right tool.
///
/// `DefaultHasher::new()` uses a fixed seed (unlike `HashMap`'s randomized
/// `RandomState`), so identical inputs produce identical `ETag`s across process
/// restarts and replicas — the property conditional requests rely on. Its hash
/// values are not guaranteed stable across Rust versions, which is harmless
/// here: an `ETag` mismatch only triggers a one-time revalidation.
fn compute_etag(version: &str, content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    version.hash(&mut hasher);
    // Delimit so ("a", "bc") and ("ab", "c") hash distinctly.
    0u8.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("\"{:016x}\"", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use axum::http::StatusCode;
    use rw_storage::MockStorage;

    use crate::testing::TestServer;

    fn anchor(section_ref: &str, subpath: &str) -> SectionAnchor {
        SectionAnchor {
            section_ref: section_ref.to_owned(),
            subpath: subpath.to_owned(),
        }
    }

    /// A `PageResponse` with fixed `content` (identical HTML), so tests can vary
    /// only `section_ancestry` and observe its effect on serialization/ETag.
    fn page_response_with(section_ancestry: HashMap<String, Vec<SectionAnchor>>) -> PageResponse {
        PageResponse {
            meta: PageMeta {
                title: Some("Same".to_owned()),
                path: "/guide".to_owned(),
                source_file: "guide".to_owned(),
                last_modified: "2025-01-01T00:00:00Z".to_owned(),
                description: None,
                page_kind: None,
                vars: None,
                section_ref: "section:default/root".to_owned(),
                subpath: "guide".to_owned(),
            },
            breadcrumbs: Vec::new(),
            toc: Vec::new(),
            content: "<h1>Same</h1>".to_owned(),
            section_ancestry: section_ancestry.into_iter().collect(),
        }
    }

    #[tokio::test]
    async fn test_page_in_tree_with_missing_source_returns_404() {
        // Regression: a page present in the site structure (returned by
        // `scan()`) but whose source file is missing from storage yields
        // `RenderError::FileNotFound`, which must map to 404 — not 500.
        // `with_document` registers the page in the tree but sets no mtime, so
        // `storage.mtime()` returns NotFound during render.
        let storage = MockStorage::new().with_document("ghost", "Ghost");
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/ghost").await;

        assert_eq!(resp.status, StatusCode::NOT_FOUND, "body: {}", resp.text());
    }

    #[tokio::test]
    async fn test_unknown_page_returns_404() {
        // A path absent from the tree raises PageNotFound (distinct from the
        // FileNotFound case above), which must also be 404.
        let server = TestServer::with_storage(MockStorage::new()).await;

        let resp = server.get("/_api/pages/does-not-exist").await;

        assert_eq!(resp.status, StatusCode::NOT_FOUND, "body: {}", resp.text());
    }

    #[tokio::test]
    async fn test_normal_page_returns_200() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
    }

    #[tokio::test]
    async fn test_page_response_sets_json_content_type_and_etag() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/guide").await;

        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        // The reworked flow returns a raw String body with an explicit
        // content-type; the String default (text/plain) must not leak or
        // duplicate, and the body must still parse as JSON.
        assert_eq!(
            resp.header("content-type").as_deref(),
            Some("application/json")
        );
        assert!(
            resp.header("etag").is_some(),
            "200 response carries an ETag"
        );
        assert_eq!(resp.json()["meta"]["title"], "Guide");
    }

    #[tokio::test]
    async fn test_page_conditional_request_returns_304() {
        let storage = MockStorage::new()
            .with_file("guide", "Guide", "# Guide\n\nContent.")
            .with_mtime("guide", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let first = server.get("/_api/pages/guide").await;
        assert_eq!(first.status, StatusCode::OK);
        let etag = first.header("etag").expect("200 response carries an ETag");

        // Re-requesting with the returned ETag as If-None-Match short-circuits
        // to 304 — the reordered check now runs after the full response (and
        // its ETag) is built.
        let second = server
            .get_with_header("/_api/pages/guide", "if-none-match", &etag)
            .await;

        assert_eq!(second.status, StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn test_section_root_page_includes_own_section_in_ancestry() {
        // A page that IS a section root gets its own section via neither
        // breadcrumbs (which exclude the current page) nor content links, yet
        // `sectionAncestry[meta.sectionRef]` must still resolve — a host maps
        // such landing pages 1:1 to catalog entities. `render()` inserts the
        // page's own enclosing section for exactly this reason.
        let storage = MockStorage::new()
            .with_document_and_kind("billing", "Billing", "domain")
            .with_content("billing", "# Billing\n\nOverview.")
            .with_mtime("billing", 1000.0);
        let server = TestServer::with_storage(storage).await;

        let resp = server.get("/_api/pages/billing").await;
        assert_eq!(resp.status, StatusCode::OK, "body: {}", resp.text());
        let json = resp.json();

        let section_ref = json["meta"]["sectionRef"].as_str().unwrap();
        assert_eq!(section_ref, "domain:default/billing");
        let chain = &json["sectionAncestry"][section_ref];
        assert!(
            chain.is_array(),
            "section-root page must key its own section in sectionAncestry; got {}",
            json["sectionAncestry"]
        );
        // The chain starts with the section itself (empty subpath).
        assert_eq!(chain[0]["sectionRef"], "domain:default/billing");
        assert_eq!(chain[0]["subpath"], "");
    }

    #[test]
    fn test_compute_etag_includes_version() {
        let etag1 = compute_etag("1.0.0", "content");
        let etag2 = compute_etag("1.0.1", "content");

        assert_ne!(etag1, etag2);
    }

    #[test]
    fn test_compute_etag_includes_content() {
        let etag1 = compute_etag("1.0.0", "content1");
        let etag2 = compute_etag("1.0.0", "content2");

        assert_ne!(etag1, etag2);
    }

    #[test]
    fn test_compute_etag_format() {
        let etag = compute_etag("1.0.0", "content");

        assert!(etag.starts_with('"'));
        assert!(etag.ends_with('"'));
        // 16 hex chars + 2 quotes = 18 total
        assert_eq!(etag.len(), 18);
    }

    #[test]
    fn test_page_meta_serialization() {
        let meta = PageMeta {
            title: Some("Guide".to_owned()),
            path: "/guide".to_owned(),
            source_file: "/docs/guide.md".to_owned(),
            last_modified: "2025-01-01T00:00:00Z".to_owned(),
            description: None,
            page_kind: None,
            vars: None,
            section_ref: "section:default/root".to_owned(),
            subpath: "guide".to_owned(),
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Guide");
        assert_eq!(json["path"], "/guide");
        assert_eq!(json["sourceFile"], "/docs/guide.md");
        assert_eq!(json["lastModified"], "2025-01-01T00:00:00Z");
        assert_eq!(json["sectionRef"], "section:default/root");
        assert_eq!(json["subpath"], "guide");
        // description, kind, and vars should be omitted when None
        assert!(json.get("description").is_none());
        assert!(json.get("kind").is_none());
        assert!(json.get("vars").is_none());
    }

    #[test]
    fn test_page_meta_serialization_with_metadata() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("owner".to_owned(), serde_json::json!("team-a"));

        let meta = PageMeta {
            title: Some("Domain Guide".to_owned()),
            path: "/domain".to_owned(),
            source_file: "/docs/domain/index.md".to_owned(),
            last_modified: "2025-01-01T00:00:00Z".to_owned(),
            description: Some("Domain overview".to_owned()),
            page_kind: Some("domain".to_owned()),
            vars: Some(serde_json::to_value(vars).unwrap()),
            section_ref: "domain:default/domain".to_owned(),
            // This page IS the section root (path == section scope), so its
            // section-relative subpath is the empty string.
            subpath: String::new(),
        };

        let json = serde_json::to_value(&meta).unwrap();

        assert_eq!(json["title"], "Domain Guide");
        assert_eq!(json["description"], "Domain overview");
        assert_eq!(json["kind"], "domain");
        assert_eq!(json["vars"]["owner"], "team-a");
        assert_eq!(json["sectionRef"], "domain:default/domain");
        assert_eq!(json["subpath"], "");
    }

    #[test]
    fn test_page_response_serializes_section_ancestry() {
        let resp = page_response_with(HashMap::from([(
            "domain:default/billing".to_owned(),
            vec![
                anchor("domain:default/billing", "overview"),
                anchor("section:default/root", ""),
            ],
        )]));

        let json = serde_json::to_value(&resp).unwrap();

        let chain = &json["sectionAncestry"]["domain:default/billing"];
        assert_eq!(chain[0]["sectionRef"], "domain:default/billing");
        assert_eq!(chain[0]["subpath"], "overview");
        assert_eq!(chain[1]["sectionRef"], "section:default/root");
        assert_eq!(chain[1]["subpath"], "");
    }

    #[test]
    fn test_page_response_omits_empty_section_ancestry() {
        let json = serde_json::to_value(page_response_with(HashMap::new())).unwrap();

        assert!(json.get("sectionAncestry").is_none());
    }

    #[test]
    fn test_etag_changes_when_section_ancestry_differs_with_identical_html() {
        // Both responses have byte-identical `content`; only the ancestry map
        // differs. The ETag must still change — hashing only the HTML would not.
        let a = page_response_with(HashMap::from([(
            "domain:default/a".to_owned(),
            vec![anchor("domain:default/a", "")],
        )]));
        let b = page_response_with(HashMap::from([(
            "domain:default/b".to_owned(),
            vec![anchor("domain:default/b", "")],
        )]));

        let etag_a = compute_etag("1.0.0", &serde_json::to_string(&a).unwrap());
        let etag_b = compute_etag("1.0.0", &serde_json::to_string(&b).unwrap());

        assert_ne!(etag_a, etag_b);
    }

    #[test]
    fn test_etag_stable_across_section_ancestry_insertion_order() {
        // The same three-key ancestry, inserted in opposite orders. The
        // `BTreeMap` conversion sorts the keys, so both serialize byte-for-byte
        // identically regardless of the source `HashMap`'s iteration order —
        // and therefore hash to the same ETag. A `HashMap` in the body would
        // instead produce spurious ETag mismatches (and 200s) across instances.
        let mut forward = HashMap::new();
        forward.insert(
            "domain:default/a".to_owned(),
            vec![anchor("domain:default/a", "")],
        );
        forward.insert(
            "domain:default/b".to_owned(),
            vec![anchor("domain:default/b", "")],
        );
        forward.insert(
            "domain:default/c".to_owned(),
            vec![anchor("domain:default/c", "")],
        );

        let mut reverse = HashMap::new();
        reverse.insert(
            "domain:default/c".to_owned(),
            vec![anchor("domain:default/c", "")],
        );
        reverse.insert(
            "domain:default/b".to_owned(),
            vec![anchor("domain:default/b", "")],
        );
        reverse.insert(
            "domain:default/a".to_owned(),
            vec![anchor("domain:default/a", "")],
        );

        let json_forward = serde_json::to_string(&page_response_with(forward)).unwrap();
        let json_reverse = serde_json::to_string(&page_response_with(reverse)).unwrap();

        // The keys must appear in sorted order in the serialized output,
        // regardless of the source HashMap's seed. This is the deterministic
        // guard: it fails outright if the response field is ever swapped back
        // to a HashMap (a 3-key HashMap could otherwise coincidentally satisfy
        // the cross-instance equality check below). `"<ref>":` matches only the
        // map key, not the same ref string appearing inside an anchor value.
        let pos_a = json_forward.find("\"domain:default/a\":").unwrap();
        let pos_b = json_forward.find("\"domain:default/b\":").unwrap();
        let pos_c = json_forward.find("\"domain:default/c\":").unwrap();
        assert!(
            pos_a < pos_b && pos_b < pos_c,
            "section ancestry keys must serialize in sorted order: {json_forward}"
        );

        // Two independently-built maps therefore serialize identically and
        // hash to the same ETag.
        assert_eq!(json_forward, json_reverse);
        assert_eq!(
            compute_etag("1.0.0", &json_forward),
            compute_etag("1.0.0", &json_reverse),
        );
    }
}
