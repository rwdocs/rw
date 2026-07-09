//! Benchmarks for site structure operations — navigation tree building and
//! reload. This is `rw`'s scan-and-index path: the cost of turning a directory
//! of markdown into a navigable site.
//!
//! These run under CodSpeed's walltime instrument on metered macro runners, so
//! the set is kept to real, above-the-noise-floor operations. Sub-microsecond
//! lookups (`has_page`/`get_breadcrumbs`, a single `HashMap` get / short vec
//! walk) are deliberately not benched here — walltime can't resolve them; if
//! they ever need guarding, they belong on the free deterministic renderer-style
//! instrument, not the clock.
//!
//! Local:    cargo bench -p rw-site --bench site_structure
//! Under CI: instrumented via CodSpeed (the `divan` dep is the compat shim).

#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and CLI examples in docs

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use divan::Bencher;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

fn main() {
    divan::main();
}

fn create_site(source_dir: PathBuf) -> Site {
    let storage = Arc::new(FsStorage::new(source_dir));
    let config = PageRendererConfig::default();
    Site::new(storage, Arc::new(rw_cache::NullCache), config)
}

/// Create a site structure with the specified depth and breadth.
fn create_site_structure(root: &Path, depth: usize, breadth: usize) {
    fn create_level(dir: &Path, current_depth: usize, max_depth: usize, breadth: usize) {
        if current_depth > max_depth {
            return;
        }

        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("index.md"),
            format!("# Level {current_depth}\n\nContent at depth {current_depth}."),
        )
        .unwrap();

        for i in 0..breadth {
            let child_dir = dir.join(format!("section-{i}"));
            create_level(&child_dir, current_depth + 1, max_depth, breadth);
        }
    }

    create_level(root, 0, depth, breadth);
}

/// Build a site of the given shape and prime its navigation once. The returned
/// `TempDir` must be kept alive for the duration of the benchmark, since
/// `FsStorage` reads the files on reload.
fn primed_site(depth: usize, breadth: usize) -> (tempfile::TempDir, Site) {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().join("docs");
    create_site_structure(&source_dir, depth, breadth);
    let site = create_site(source_dir);
    let _ = site.navigation(None);
    (dir, site)
}

/// Build the navigation tree from a primed (already-scanned) site — the in-memory
/// tree materialization, on a representative site (depth 4, breadth 3).
#[divan::bench]
fn navigation(bencher: Bencher) {
    let (_dir, site) = primed_site(4, 3);
    bencher.bench(|| site.navigation(None));
}

/// Reload cost: cached (nothing changed, fast revalidation) vs. forced re-scan
/// after invalidation (the live-reload edit path). Depth 3, breadth 5.
#[divan::bench(args = ["cached", "after_invalidate"])]
fn reload(bencher: Bencher, kind: &str) {
    let (_dir, site) = primed_site(3, 5);
    match kind {
        "cached" => bencher.bench(|| site.navigation(None)),
        "after_invalidate" => bencher.bench(|| {
            site.invalidate();
            site.navigation(None)
        }),
        _ => unreachable!(),
    }
}

/// Cold build: scan and index a ~341-page site (depth 4, breadth 4) from scratch
/// on each sample. File-tree creation is untimed setup; only the scan is timed.
#[divan::bench]
fn build_from_scratch(bencher: Bencher) {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().join("docs");
    create_site_structure(&source_dir, 4, 4);
    bencher
        .with_inputs(|| create_site(source_dir.clone()))
        .bench_values(|site| site.navigation(None));
}
