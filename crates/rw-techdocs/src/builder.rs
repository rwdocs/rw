//! Static site builder for TechDocs.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{NavItem, PageRendererConfig, Site, TocEntry};
use rw_storage::Storage;

use crate::template::{self, BreadcrumbData, NavItemData, PageData, TocData};

/// Configuration for static site building.
pub struct BuildConfig {
    /// Site name for techdocs_metadata.json.
    pub site_name: String,
    /// Optional CSS content to write. Falls back to minimal default if `None`.
    pub css_content: Option<String>,
}

/// Error returned by the static site builder.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Render error: {0}")]
    Render(#[from] rw_site::RenderError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Builds a static documentation site from a storage backend.
pub struct StaticSiteBuilder {
    storage: Arc<dyn Storage>,
    config: BuildConfig,
    renderer_config: PageRendererConfig,
}

impl StaticSiteBuilder {
    /// Create a new builder with the given storage and configuration.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, config: BuildConfig) -> Self {
        Self {
            storage,
            config,
            renderer_config: PageRendererConfig::default(),
        }
    }

    /// Set the page renderer configuration.
    #[must_use]
    pub fn with_renderer_config(mut self, config: PageRendererConfig) -> Self {
        self.renderer_config = config;
        self
    }

    /// Build the static site to the output directory.
    pub fn build(&self, output_dir: &Path) -> Result<(), BuildError> {
        let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
        let site = Site::new(
            Arc::clone(&self.storage),
            cache,
            PageRendererConfig {
                extract_title: self.renderer_config.extract_title,
                kroki_url: self.renderer_config.kroki_url.clone(),
                include_dirs: self.renderer_config.include_dirs.clone(),
                dpi: self.renderer_config.dpi,
            },
        );

        // Collect all page paths by walking scoped navigation
        let all_paths = collect_all_paths(&site);

        tracing::info!(count = all_paths.len(), "Rendering pages");

        // Render each page
        for page_path in &all_paths {
            let render_result = site.render(page_path)?;
            let scope = site.get_navigation_scope(page_path);
            let page_nav = site.navigation(&scope);

            let nav_items = convert_nav_items(&page_nav.items, page_path);
            let breadcrumbs = convert_breadcrumbs(&render_result.breadcrumbs);
            let toc = convert_toc(&render_result.toc);
            let css_path = compute_css_path(page_path);

            let title = render_result
                .title
                .unwrap_or_else(|| "Untitled".to_owned());

            let page_data = PageData {
                title,
                path: page_path.clone(),
                html_content: render_result.html,
                breadcrumbs,
                toc,
                navigation: nav_items,
                css_path,
            };

            let html = template::render_page(&page_data);

            let file_path = if page_path.is_empty() {
                output_dir.join("index.html")
            } else {
                output_dir.join(page_path).join("index.html")
            };

            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file_path, html)?;

            tracing::debug!(path = %page_path, "Wrote page");
        }

        self.write_css(output_dir)?;
        self.write_metadata(output_dir)?;

        Ok(())
    }

    fn write_css(&self, output_dir: &Path) -> Result<(), BuildError> {
        let css_dir = output_dir.join("assets");
        fs::create_dir_all(&css_dir)?;
        let css = self.config.css_content.as_deref().unwrap_or(DEFAULT_CSS);
        fs::write(css_dir.join("styles.css"), css)?;
        Ok(())
    }

    fn write_metadata(&self, output_dir: &Path) -> Result<(), BuildError> {
        let metadata = serde_json::json!({
            "site_name": self.config.site_name,
            "site_description": ""
        });
        let json = serde_json::to_string(&metadata)?;
        fs::write(output_dir.join("techdocs_metadata.json"), json)?;
        Ok(())
    }
}

/// Collect all page paths by recursively walking scoped navigation.
fn collect_all_paths(site: &Site) -> Vec<String> {
    let mut paths = Vec::new();

    // Add root page if it exists
    if site.has_page("") {
        paths.push(String::new());
    }

    // Walk from root scope
    collect_from_scope(site, "", &mut paths);
    paths
}

/// Collect page paths from a navigation scope.
fn collect_from_scope(site: &Site, scope: &str, paths: &mut Vec<String>) {
    let nav = site.navigation(scope);
    for item in &nav.items {
        paths.push(item.path.clone());
        if item.section_type.is_some() {
            // Section: recurse into its scoped navigation
            collect_from_scope(site, &item.path, paths);
        } else {
            // Regular item: recurse into children
            collect_children_paths(&item.children, paths);
        }
    }
}

/// Collect paths from navigation children recursively.
fn collect_children_paths(items: &[NavItem], paths: &mut Vec<String>) {
    for item in items {
        paths.push(item.path.clone());
        collect_children_paths(&item.children, paths);
    }
}

fn convert_nav_items(items: &[NavItem], current_path: &str) -> Vec<NavItemData> {
    items
        .iter()
        .map(|item| NavItemData {
            title: item.title.clone(),
            path: format!("/{}", item.path),
            is_active: item.path == current_path,
            children: convert_nav_items(&item.children, current_path),
        })
        .collect()
}

fn convert_breadcrumbs(breadcrumbs: &[rw_site::BreadcrumbItem]) -> Vec<BreadcrumbData> {
    breadcrumbs
        .iter()
        .map(|b| BreadcrumbData {
            title: b.title.clone(),
            path: format!("/{}", b.path),
        })
        .collect()
}

fn convert_toc(toc: &[TocEntry]) -> Vec<TocData> {
    toc.iter()
        .filter(|e| e.level >= 2 && e.level <= 3)
        .map(|e| TocData {
            level: e.level,
            title: e.title.clone(),
            id: e.id.clone(),
        })
        .collect()
}

fn compute_css_path(page_path: &str) -> String {
    if page_path.is_empty() {
        "assets/styles.css".to_owned()
    } else {
        let depth = page_path.matches('/').count() + 1;
        let prefix = "../".repeat(depth);
        format!("{prefix}assets/styles.css")
    }
}

/// Minimal fallback CSS when frontend build output is not available.
const DEFAULT_CSS: &str = r"/* Minimal fallback styles for rw techdocs build */
*,::before,::after{box-sizing:border-box}
body{margin:0;font-family:system-ui,-apple-system,sans-serif;line-height:1.6}
.min-h-screen{min-height:100vh}
.flex{display:flex}.flex-col{flex-direction:column}.flex-1{flex:1 1 0%}
.items-center{align-items:center}
.hidden{display:none}
.block{display:block}
.sticky{position:sticky}.top-0{top:0}.top-6{top:1.5rem}
.overflow-y-auto{overflow-y:auto}
.h-screen{height:100vh}
.w-\[280px\]{width:280px}.w-\[240px\]{width:240px}.w-\[22px\]{width:22px}
.w-5{width:1.25rem}.h-5{height:1.25rem}.w-3\.5{width:0.875rem}.h-3\.5{height:0.875rem}
.flex-shrink-0{flex-shrink:0}.min-w-0{min-width:0}
.border-r{border-right:1px solid}.border-gray-200{border-color:#e5e7eb}
.bg-white{background:#fff}.text-gray-900{color:#111827}
.text-gray-700{color:#374151}.text-gray-600{color:#4b5563}.text-gray-500{color:#6b7280}.text-gray-400{color:#9ca3af}
.text-blue-700{color:#1d4ed8}
.text-xl{font-size:1.25rem}.text-sm{font-size:0.875rem}.text-xs{font-size:0.75rem}
.font-semibold{font-weight:600}.font-medium{font-weight:500}
.uppercase{text-transform:uppercase}.tracking-wider{letter-spacing:0.05em}
.leading-snug{line-height:1.375}
.antialiased{-webkit-font-smoothing:antialiased}
.rounded{border-radius:0.25rem}
.pt-6{padding-top:1.5rem}.pb-4{padding-bottom:1rem}.pb-12{padding-bottom:3rem}
.px-4{padding-left:1rem;padding-right:1rem}.px-8{padding-left:2rem;padding-right:2rem}
.px-1\.5{padding-left:0.375rem;padding-right:0.375rem}.py-1\.5{padding-top:0.375rem;padding-bottom:0.375rem}
.pl-8{padding-left:2rem}.pl-\[6px\]{padding-left:6px}
.mb-3{margin-bottom:0.75rem}.mb-5{margin-bottom:1.25rem}.mb-6{margin-bottom:1.5rem}
.ml-3{margin-left:0.75rem}.mr-0\.5{margin-right:0.125rem}
.max-w-6xl{max-width:72rem}.mx-auto{margin-left:auto;margin-right:auto}.max-w-none{max-width:none}
.space-y-1\.5>:not(:first-child){margin-top:0.375rem}
.rotate-90{transform:rotate(90deg)}
.justify-center{justify-content:center}
@media(min-width:768px){.md\\:flex-row{flex-direction:row}.md\\:block{display:block}.md\\:px-8{padding-left:2rem;padding-right:2rem}}
@media(min-width:1024px){.lg\\:block{display:block}}
.prose{max-width:65ch;color:#334155}
.prose h1{font-size:2.25em;font-weight:800;margin-top:0;margin-bottom:0.9em;line-height:1.1}
.prose h2{font-size:1.5em;font-weight:700;margin-top:2em;margin-bottom:1em;line-height:1.3;border-bottom:1px solid #e2e8f0;padding-bottom:0.3em}
.prose h3{font-size:1.25em;font-weight:600;margin-top:1.6em;margin-bottom:0.6em;line-height:1.6}
.prose p{margin-top:1.25em;margin-bottom:1.25em}
.prose a{color:#2563eb;text-decoration:underline}
.prose code{font-size:0.875em;font-weight:600;color:#1e293b}
.prose pre{overflow-x:auto;font-size:0.875em;line-height:1.7;margin-top:1.7em;margin-bottom:1.7em;border-radius:0.375rem;padding:0.85em 1.1em;background:#1e293b;color:#e2e8f0}
.prose pre code{font-weight:inherit;color:inherit;font-size:inherit;background:transparent;padding:0}
.prose ul{list-style-type:disc;margin-top:1.25em;margin-bottom:1.25em;padding-left:1.625em}
.prose ol{list-style-type:decimal;margin-top:1.25em;margin-bottom:1.25em;padding-left:1.625em}
.prose li{margin-top:0.5em;margin-bottom:0.5em}
.prose table{width:100%;table-layout:auto;text-align:left;margin-top:2em;margin-bottom:2em;font-size:0.875em;line-height:1.7}
.prose thead{border-bottom:1px solid #cbd5e1}
.prose thead th{font-weight:600;padding:0 0.57em 0.57em}
.prose tbody td{padding:0.57em}
.prose tbody tr{border-bottom:1px solid #e2e8f0}
.prose blockquote{font-weight:500;font-style:italic;color:#64748b;border-left:0.25rem solid #e2e8f0;padding-left:1em;margin:1.6em 0}
.prose img{margin-top:2em;margin-bottom:2em;max-width:100%}
.prose hr{border-color:#e2e8f0;margin-top:3em;margin-bottom:3em}
";

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rw_storage::MockStorage;
    use tempfile::TempDir;

    use super::*;

    fn mock_storage_with_pages() -> MockStorage {
        MockStorage::new()
            .with_file("", "Home", "# Home\n\nWelcome")
            .with_mtime("", 1000.0)
            .with_file("guide", "Guide", "# Guide\n\nHello")
            .with_mtime("guide", 1000.0)
    }

    #[test]
    fn build_creates_output_files() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test Site".to_owned(),
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        assert!(output_dir.join("index.html").exists());
        assert!(output_dir.join("guide/index.html").exists());
        assert!(output_dir.join("techdocs_metadata.json").exists());
        assert!(output_dir.join("assets/styles.css").exists());
    }

    #[test]
    fn build_metadata_json() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "My Docs".to_owned(),
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let meta = std::fs::read_to_string(output_dir.join("techdocs_metadata.json")).unwrap();
        assert!(meta.contains("My Docs"));
    }

    #[test]
    fn build_writes_custom_css() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test".to_owned(),
            css_content: Some("body { color: red; }".to_owned()),
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let css = std::fs::read_to_string(output_dir.join("assets/styles.css")).unwrap();
        assert!(css.contains("color: red"));
    }

    #[test]
    fn build_writes_default_css_when_none() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test".to_owned(),
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let css = std::fs::read_to_string(output_dir.join("assets/styles.css")).unwrap();
        assert!(css.contains(".prose"));
    }

    #[test]
    fn build_page_contains_content() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test".to_owned(),
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let home_html = std::fs::read_to_string(output_dir.join("index.html")).unwrap();
        assert!(home_html.contains("Welcome"));

        let guide_html = std::fs::read_to_string(output_dir.join("guide/index.html")).unwrap();
        assert!(guide_html.contains("Hello"));
    }

    #[test]
    fn build_nested_pages() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(
            MockStorage::new()
                .with_file("", "Home", "# Home")
                .with_mtime("", 1000.0)
                .with_file("domain", "Domain", "# Domain")
                .with_mtime("domain", 1000.0)
                .with_file("domain/api", "API", "# API\n\nEndpoints")
                .with_mtime("domain/api", 1000.0),
        );
        let config = BuildConfig {
            site_name: "Test".to_owned(),
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        assert!(output_dir.join("index.html").exists());
        assert!(output_dir.join("domain/index.html").exists());
        assert!(output_dir.join("domain/api/index.html").exists());

        let api_html =
            std::fs::read_to_string(output_dir.join("domain/api/index.html")).unwrap();
        assert!(api_html.contains("Endpoints"));
        // CSS path should have correct relative depth
        assert!(api_html.contains("../../assets/styles.css"));
    }

    #[test]
    fn compute_css_path_root() {
        assert_eq!(compute_css_path(""), "assets/styles.css");
    }

    #[test]
    fn compute_css_path_nested() {
        assert_eq!(compute_css_path("guide"), "../assets/styles.css");
        assert_eq!(
            compute_css_path("domain/api"),
            "../../assets/styles.css"
        );
        assert_eq!(
            compute_css_path("a/b/c"),
            "../../../assets/styles.css"
        );
    }
}
