//! A `_` file is no page, so a link to it is as dead as a link to a missing
//! file.

use std::fs;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{PageRendererConfig, RenderError, Site};
use rw_storage_fs::FsStorage;

#[test]
fn a_link_to_an_underscore_file_is_a_dead_link() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    fs::create_dir_all(docs.join("_partials")).unwrap();
    fs::write(docs.join("_partials/w.md"), "# Warning\n").unwrap();
    fs::write(
        docs.join("guide.md"),
        "[w](_partials/w.md) [m](_partials/missing.md)\n",
    )
    .unwrap();
    let storage = Arc::new(FsStorage::new(temp.path().to_path_buf(), docs));
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());

    let html = site.render("guide").unwrap().html;

    assert!(html.contains(r#"href="/_partials/w""#), "{html}");
    assert!(html.contains(r#"href="/_partials/missing""#), "{html}");
    assert!(matches!(
        site.render("_partials/w"),
        Err(RenderError::PageNotFound(_))
    ));
}
