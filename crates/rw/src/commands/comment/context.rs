use std::path::Path;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_comments::SqliteCommentStore;
use rw_config::Config;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

use crate::error::CliError;

/// Per-invocation context shared across subcommands.
pub(super) struct Context {
    pub config: Config,
    pub store: SqliteCommentStore,
}

impl Context {
    pub(super) async fn load() -> Result<Self, CliError> {
        let config = Config::load(None, None)?;
        let path = SqliteCommentStore::default_path(&config.docs_resolved.project_dir);
        let store = SqliteCommentStore::open(&path).await?;
        Ok(Self { config, store })
    }
}

/// Build an [`FsStorage`] over the project's docs directory.
fn build_storage(config: &Config) -> FsStorage {
    FsStorage::with_meta_filename(
        config.docs_resolved.source_dir.clone(),
        &config.metadata.name,
    )
}

/// Build a read-only [`Site`] over the project's docs for the comment CLI.
pub(super) fn build_site(config: &Config) -> Site {
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(build_storage(config));
    let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
    let renderer_config = PageRendererConfig {
        kroki_url: config.diagrams_resolved.kroki_url.clone(),
        include_dirs: config.diagrams_resolved.include_dirs.clone(),
        dpi: config.diagrams_resolved.dpi,
        ..PageRendererConfig::default()
    };
    Site::new(storage, cache, renderer_config)
}

/// Resolve the `--document` argument to a page URL path. A value ending in
/// `.md` is treated as a source file path and mapped through `FsStorage`
/// exactly as the scanner would (so the key matches the live site); any other
/// value is treated as a URL path, tolerating a leading slash.
///
/// Accepts the path with or without the docs-root prefix
/// (`docs/guide.md` or `guide.md`). When the input would map to two
/// different existing pages it is ambiguous and the caller must retry with
/// the URL path.
pub(super) fn document_url_path(config: &Config, document: &str) -> Result<String, CliError> {
    // Case-sensitive `.md` to match the scanner's classification
    // (`classify_relpath` uses `ext == "md"`); a `.MD` value falls through to
    // URL-path handling rather than erroring as "not a markdown page".
    let is_md = Path::new(document)
        .extension()
        .is_some_and(|ext| ext == "md");
    if !is_md {
        return Ok(document.trim_start_matches('/').to_owned());
    }

    let mut urls = build_storage(config).url_paths_for_source(Path::new(document));
    urls.sort();
    match urls.len() {
        0 => Err(CliError::Validation(format!(
            "'{document}' is not a markdown page under {}",
            config.docs_resolved.source_dir.display()
        ))),
        1 => Ok(urls.pop().unwrap()),
        _ => Err(CliError::Validation(format!(
            "'{document}' is ambiguous — it could be the page '{}'. \
             Re-run with the page's URL path to disambiguate.",
            urls.join("' or '")
        ))),
    }
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

        let storage = Arc::new(FsStorage::new(docs));
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
