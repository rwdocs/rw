use std::path::Path;

use rw_comments::SqliteCommentStore;
use rw_config::Config;
use rw_site::Site;

use crate::commands::{ProjectArgs, ensure_data_dir, source_paths};
use crate::error::CliError;

/// Per-invocation context shared across subcommands.
pub(super) struct Context {
    pub config: Config,
    pub store: SqliteCommentStore,
}

impl Context {
    pub(super) async fn load(project: &ProjectArgs, read_only: bool) -> Result<Self, CliError> {
        let config = project.load(None)?;
        let path = SqliteCommentStore::default_path(&config.docs.data_dir);
        // A read finds no comments in a store that does not exist yet.
        let store = if read_only && !path.exists() {
            SqliteCommentStore::open_memory().await?
        } else {
            ensure_data_dir(&config.docs.data_dir)?;
            SqliteCommentStore::open(&path).await?
        };
        Ok(Self { config, store })
    }
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

/// Resolve a page's URL path to its stable comment key — `sectionRef#subpath` —
/// matching the page response's `documentId`. Keying comments on
/// `(sectionRef, subpath)` keeps them anchored when a whole section is relocated,
/// and keeps CLI-created and browser-created comments on the same key.
pub(super) fn document_key(site: &Site, page_path: &str) -> Result<String, CliError> {
    Ok(site.document_id(page_path)?)
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
