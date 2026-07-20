//! Benchmarks for the `Site::render` serving path — the per-request cost on top
//! of the markdown renderer: storage read, metadata, and the page cache.
//!
//! Scope is deliberately narrow. Pure markdown->HTML render (including per-feature
//! cost — tables, code fences, alerts, task lists) is covered by `rw-renderer`'s
//! `render` bench; the one-time site scan/index path is covered by this crate's
//! `site_structure` bench. This file measures only what is unique to
//! `Site::render`: how render cost scales through the full path, and the warm
//! persistent-cache hit that is the real steady-state serving cost.
//!
//! Each timed `render` runs against a site whose scan is already primed (via
//! `navigation`, as untimed `with_inputs` setup) so it measures the per-request
//! path, not the startup scan. Priming `navigation` leaves the in-process render
//! memo empty, so every timed render is a real render (or, for `cache_hit`, a
//! real persistent-cache read) rather than a memo hit.
//!
//! Local:    cargo bench -p rw-site --bench page_rendering
//! Under CI: instrumented via CodSpeed (the `divan` dep is the compat shim).

#![allow(clippy::format_push_string)] // Benchmark setup code, performance not critical
#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and CLI examples in docs

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use divan::counter::BytesCount;
use divan::{Bencher, black_box};
use rw_cache::{Cache, FileCache, NullCache};
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

fn main() {
    divan::main();
}

fn create_site_with_config(
    project_dir: PathBuf,
    cache: Arc<dyn Cache>,
    config: PageRendererConfig,
) -> Site {
    // These fixtures keep their markdown at the project root rather than in a
    // `docs/` subdirectory, so the project dir and the source dir are the same
    // directory here.
    let storage = Arc::new(FsStorage::new(project_dir.clone(), project_dir));
    Site::new(storage, cache, config)
}

fn create_site(project_dir: PathBuf) -> Site {
    create_site_with_config(
        project_dir,
        Arc::new(NullCache),
        PageRendererConfig::default(),
    )
}

/// Build `site` and prime its scan (via `navigation`) without touching the render
/// memo, so a following timed `render` measures the per-request path only.
fn scan_primed(site: Site) -> Site {
    let _ = site.navigation(None);
    site
}

/// Generate markdown content with the specified structure.
fn generate_markdown(headings: usize, paragraphs_per_section: usize) -> String {
    let mut md = String::with_capacity(headings * 50 + headings * paragraphs_per_section * 200);
    md.push_str("# Document Title\n\n");

    for i in 0..headings {
        md.push_str(&format!("## Section {i}\n\n"));
        for j in 0..paragraphs_per_section {
            md.push_str(&format!(
                "This is paragraph {j} in section {i}. It contains **bold** and *italic* text.\n\n"
            ));
        }
    }
    md
}

/// Render a large (~100KB) document through the full `Site` path; reports
/// throughput in bytes.
#[divan::bench]
fn large_document(bencher: Bencher) {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().to_path_buf();
    let markdown = generate_markdown(100, 5);
    let bytes = markdown.len();
    fs::write(source_dir.join("large.md"), &markdown).unwrap();
    bencher
        .counter(BytesCount::new(bytes))
        .with_inputs(|| scan_primed(create_site(source_dir.clone())))
        .bench_values(|site| site.render(black_box("large")));
}

/// Cold render (`NullCache`, cache miss) vs. warm persistent `FileCache` hit for
/// the same page. `cache_hit` uses a *fresh* site over a pre-primed on-disk cache
/// so its render is served from `FileCache` (the in-process memo is empty),
/// measuring the real persistent-cache read rather than a memo hit. The delta
/// between the two arms is the render work the cache saves.
#[divan::bench(args = ["cache_miss", "cache_hit"])]
fn caching(bencher: Bencher, kind: &str) {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().to_path_buf();
    fs::write(source_dir.join("cached.md"), generate_markdown(10, 3)).unwrap();

    match kind {
        "cache_miss" => bencher
            .with_inputs(|| scan_primed(create_site(source_dir.clone())))
            .bench_values(|site| site.render(black_box("cached"))),
        "cache_hit" => {
            let cache_dir = dir.path().join("cache");
            let file_cache =
                || -> Arc<dyn Cache> { Arc::new(FileCache::new(cache_dir.clone(), "bench")) };
            // Prime the on-disk cache once (untimed).
            let _ = create_site_with_config(
                source_dir.clone(),
                file_cache(),
                PageRendererConfig::default(),
            )
            .render("cached");
            bencher
                .with_inputs(|| {
                    scan_primed(create_site_with_config(
                        source_dir.clone(),
                        file_cache(),
                        PageRendererConfig::default(),
                    ))
                })
                .bench_values(|site| site.render(black_box("cached")));
        }
        _ => unreachable!(),
    }
}
