//! Static site builder for TechDocs.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{NavItem, Navigation, PageRendererConfig, Site, TocEntry};
use rw_storage::Storage;

use crate::template::{
    self, BreadcrumbData, NavGroupData, NavItemData, PageData, ScopeHeaderData, TocData,
};

/// Configuration for static site building.
pub struct BuildConfig {
    /// Site name for techdocs_metadata.json.
    pub site_name: String,
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
                relative_links: self.renderer_config.relative_links,
                trailing_slash: self.renderer_config.trailing_slash,
                static_tabs: self.renderer_config.static_tabs,
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
            let scope = convert_scope(&page_nav);
            let nav_groups = group_nav_items(nav_items);
            let breadcrumbs = convert_breadcrumbs(&render_result.breadcrumbs);
            let toc = convert_toc(&render_result.toc);
            let css_path = compute_css_path(page_path);

            let title = render_result.title.unwrap_or_else(|| "Untitled".to_owned());

            let page_data = PageData {
                title,
                path: page_path.clone(),
                html_content: render_result.html,
                breadcrumbs,
                toc,
                scope,
                nav_groups,
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

        self.write_assets(output_dir)?;
        self.write_metadata(output_dir)?;

        Ok(())
    }

    fn write_assets(&self, output_dir: &Path) -> Result<(), BuildError> {
        let assets_dir = output_dir.join("assets");
        fs::create_dir_all(&assets_dir)?;

        let mut css_written = false;

        for path in rw_assets::iter() {
            let path = path.as_ref();
            if !path.starts_with("assets/") {
                continue;
            }
            let filename = &path["assets/".len()..];
            let Some(data) = rw_assets::get(path) else {
                continue;
            };

            if !css_written && filename.ends_with(".css") {
                fs::write(assets_dir.join("styles.css"), data.as_ref())?;
                css_written = true;
            } else if filename.ends_with(".woff") || filename.ends_with(".woff2") {
                fs::write(assets_dir.join(filename), data.as_ref())?;
            }
        }

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
            section_type: item.section_type.clone(),
        })
        .collect()
}

fn convert_scope(nav: &Navigation) -> Option<ScopeHeaderData> {
    let scope = nav.scope.as_ref()?;
    let back = nav
        .parent_scope
        .as_ref()
        .map(|p| (p.title.clone(), p.path.clone()));
    let (back_link_title, back_link_path) =
        back.unwrap_or_else(|| ("Home".to_owned(), "/".to_owned()));
    Some(ScopeHeaderData {
        title: scope.title.clone(),
        back_link_title,
        back_link_path,
    })
}

/// Group navigation items by section type (matches frontend `groupNavItems()`).
fn group_nav_items(items: Vec<NavItemData>) -> Vec<NavGroupData> {
    let mut typed_groups: std::collections::BTreeMap<String, Vec<NavItemData>> =
        std::collections::BTreeMap::new();
    let mut ungrouped = Vec::new();

    for item in items {
        if let Some(ref st) = item.section_type {
            typed_groups.entry(st.clone()).or_default().push(item);
        } else {
            ungrouped.push(item);
        }
    }

    let mut groups = Vec::new();

    if !ungrouped.is_empty() {
        groups.push(NavGroupData {
            label: None,
            items: ungrouped,
        });
    }

    for (section_type, group_items) in typed_groups {
        groups.push(NavGroupData {
            label: Some(pluralize_type(&section_type)),
            items: group_items,
        });
    }

    groups
}

fn pluralize_type(t: &str) -> String {
    match t.to_lowercase().as_str() {
        "domain" => "Domains".to_owned(),
        "system" => "Systems".to_owned(),
        "service" => "Services".to_owned(),
        "api" => "APIs".to_owned(),
        "guide" => "Guides".to_owned(),
        _ => format!("{t}s"),
    }
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

        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        assert!(output_dir.join("index.html").exists());
        assert!(output_dir.join("guide/index.html").exists());
        assert!(output_dir.join("techdocs_metadata.json").exists());
    }

    #[test]
    fn build_metadata_json() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "My Docs".to_owned(),

        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let meta = std::fs::read_to_string(output_dir.join("techdocs_metadata.json")).unwrap();
        assert!(meta.contains("My Docs"));
    }

    #[test]
    fn build_page_contains_content() {
        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test".to_owned(),

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

        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        assert!(output_dir.join("index.html").exists());
        assert!(output_dir.join("domain/index.html").exists());
        assert!(output_dir.join("domain/api/index.html").exists());

        let api_html = std::fs::read_to_string(output_dir.join("domain/api/index.html")).unwrap();
        assert!(api_html.contains("Endpoints"));
        // CSS path should have correct relative depth
        assert!(api_html.contains("../../assets/styles.css"));
    }

    #[test]
    fn build_copies_font_assets() {
        // rw-assets in dev mode reads from `frontend/dist` relative to CWD.
        // cargo test sets CWD to the crate directory, so move to workspace root.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if !workspace_root.join("frontend/dist/assets").exists() {
            // Frontend not built; skip.
            return;
        }
        std::env::set_current_dir(&workspace_root).unwrap();

        let tmp = TempDir::new().unwrap();
        let output_dir = tmp.path().join("site");
        let storage = Arc::new(mock_storage_with_pages());
        let config = BuildConfig {
            site_name: "Test".to_owned(),

        };

        let builder = StaticSiteBuilder::new(storage, config);
        builder.build(&output_dir).unwrap();

        let assets_dir = output_dir.join("assets");
        let font_files: Vec<_> = std::fs::read_dir(&assets_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("woff" | "woff2")
                )
            })
            .collect();
        assert!(
            !font_files.is_empty(),
            "Font files should be copied to assets/"
        );
    }

    #[test]
    fn compute_css_path_root() {
        assert_eq!(compute_css_path(""), "assets/styles.css");
    }

    #[test]
    fn compute_css_path_nested() {
        assert_eq!(compute_css_path("guide"), "../assets/styles.css");
        assert_eq!(compute_css_path("domain/api"), "../../assets/styles.css");
        assert_eq!(compute_css_path("a/b/c"), "../../../assets/styles.css");
    }
}
