use std::fs;
use std::path::{Component, Path};
use std::sync::Arc;

use rw_cache::NullCache;
use rw_comments::SqliteCommentStore;
use rw_config::Config;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

use crate::commands::ensure_data_dir;
use crate::error::CliError;

const README: &str = "README.md";

/// Per-invocation context shared across subcommands.
pub(super) struct Context {
    pub config: Config,
    pub store: SqliteCommentStore,
}

impl Context {
    pub(super) async fn load() -> Result<Self, CliError> {
        let config = Config::load(None, None)?;
        ensure_data_dir(&config.docs.data_dir)?;
        let path = SqliteCommentStore::default_path(&config.docs.data_dir);
        let store = SqliteCommentStore::open(&path).await?;
        Ok(Self { config, store })
    }
}

/// Build an [`FsStorage`] over the project's docs directory.
fn build_storage(config: &Config) -> FsStorage {
    FsStorage::with_meta_filename(
        config.project_dir.clone(),
        config.docs.source_dir.clone(),
        &config.metadata.name,
    )
}

/// Build a read-only [`Site`] over the project's docs for the comment CLI.
pub(super) fn build_site(config: &Config) -> Site {
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(build_storage(config));
    let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
    let renderer_config = PageRendererConfig {
        kroki_url: config.diagrams.kroki_url.clone(),
        include_dirs: config.diagrams.include_dirs.clone(),
        ..PageRendererConfig::default()
    };
    Site::new(storage, cache, renderer_config)
}

/// Resolve the `--document` argument to a page URL path. A value ending in
/// `.md` is a source file, looked up among the site's pages; any other value
/// is a URL path, tolerating a leading slash.
///
/// A relative source path may start at the docs root or at the project root
/// (`guide.md` or `docs/guide.md`). When the two readings name different
/// pages the input is ambiguous and the caller must retry with the URL path.
pub(super) fn document_url_path(
    config: &Config,
    site: &Site,
    document: &str,
) -> Result<String, CliError> {
    // Case-sensitive `.md` to match the scanner's classification
    // (`classify_relpath` uses `ext == "md"`); a `.MD` value falls through to
    // URL-path handling rather than erroring as "not a markdown page".
    let is_md = Path::new(document)
        .extension()
        .is_some_and(|ext| ext == "md");
    if !is_md {
        return Ok(document.trim_start_matches('/').to_owned());
    }

    let mut urls = Vec::new();
    for source_path in source_paths(config, Path::new(document)) {
        if let Some(url) = site.page_for_source(&source_path)?
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }
    urls.sort();
    match urls.len() {
        0 => Err(CliError::Validation(format!(
            "'{document}' is not a markdown page under {}",
            config.docs.source_dir.display()
        ))),
        1 => Ok(urls.pop().unwrap()),
        _ => Err(CliError::Validation(format!(
            "'{document}' is ambiguous — it could be the page '{}'. \
             Re-run with the page's URL path to disambiguate.",
            urls.join("' or '")
        ))),
    }
}

/// The docs-root-relative paths `document` may name.
fn source_paths(config: &Config, document: &Path) -> Vec<String> {
    let storage = build_storage(config);
    let label = storage.source_label().unwrap_or_default();
    if document.is_relative() {
        // Read from two roots, so `..` has no single meaning.
        if document.components().any(|c| c == Component::ParentDir) {
            return Vec::new();
        }
        let document = slash_path(document);
        let from_source = relative_from(label, &document);
        return vec![document, from_source];
    }
    // Only the parent is canonicalized, so a symlinked page keeps its name.
    let canonical = |path: &Path| fs::canonicalize(path).ok();
    let Some(file) = document
        .parent()
        .and_then(canonical)
        .zip(document.file_name())
        .map(|(dir, name)| dir.join(name))
    else {
        return Vec::new();
    };
    let in_source = canonical(&config.docs.source_dir)
        .and_then(|source| Some(slash_path(file.strip_prefix(source).ok()?)));
    let readme = canonical(&config.project_dir)
        .is_some_and(|project| file == project.join(README))
        .then(|| relative_from(label, README));
    in_source.into_iter().chain(readme).collect()
}

/// `path`'s normal components joined with `/`.
fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The project-relative `path` as seen from the project-relative `dir`.
fn relative_from(dir: &str, path: &str) -> String {
    let dir: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let path: Vec<&str> = path.split('/').collect();
    let shared = dir.iter().zip(&path).take_while(|(a, b)| a == b).count();
    format!(
        "{}{}",
        "../".repeat(dir.len() - shared),
        path[shared..].join("/")
    )
}

/// Resolve a page's URL path to its stable comment key — `sectionRef#subpath` —
/// matching the key the viewer derives from page metadata. Keying comments on
/// `(sectionRef, subpath)` keeps them anchored when a whole section is relocated,
/// and keeps CLI-created and browser-created comments on the same key.
pub(super) fn document_key(site: &Site, page_path: &str) -> Result<String, CliError> {
    let (section_ref, subpath) = site.section_location(page_path)?;
    Ok(format!("{section_ref}#{subpath}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use rw_cache::NullCache;
    use rw_site::{PageRendererConfig, Site};
    use rw_storage_fs::FsStorage;
    use tempfile::TempDir;

    fn site_with_billing_section() -> (TempDir, Site) {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        let billing = docs.join("billing");
        fs::create_dir_all(&billing).unwrap();
        fs::write(billing.join("meta.yaml"), "kind: domain\n").unwrap();
        fs::write(billing.join("overview.md"), "# Overview\n").unwrap();

        let storage = Arc::new(FsStorage::new(dir.path().to_path_buf(), docs));
        let site = Site::new(
            storage as Arc<dyn rw_storage::Storage>,
            Arc::new(NullCache),
            PageRendererConfig::default(),
        );
        (dir, site)
    }

    #[test]
    fn document_key_for_page_inside_section() {
        let (_dir, site) = site_with_billing_section();
        let key = super::document_key(&site, "billing/overview").unwrap();
        assert_eq!(key, "domain:default/billing#overview");
    }

    #[test]
    fn document_key_for_section_root_has_empty_subpath() {
        // A section's own root page has an empty subpath, so the key ends in a
        // bare `#`. This is the load-bearing case for comments on a section
        // landing page.
        let (_dir, site) = site_with_billing_section();
        let key = super::document_key(&site, "billing").unwrap();
        assert_eq!(key, "domain:default/billing#");
    }
}
