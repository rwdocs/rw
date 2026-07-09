//! Benchmarks for site structure operations — navigation tree building, page
//! lookup, breadcrumbs, and reload. This is `rw`'s scan-and-index path: the
//! cost of turning a directory of markdown into a navigable site.
//!
//! Local:    cargo bench -p rw-site --bench site_structure
//! Under CI: instrumented via CodSpeed (the `divan` dep is the compat shim).

#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and CLI examples in docs

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use divan::{Bencher, black_box};
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

/// Page-existence lookups at varying depths against a primed site (depth 4,
/// breadth 4): shallow hit, deep hit, and a miss. Paths carry no leading slash —
/// `path_index` is keyed on the bare page path (e.g. `"section-0/section-1"`).
#[divan::bench(args = ["hit_shallow", "hit_deep", "miss"])]
fn lookup(bencher: Bencher, kind: &str) {
    let (_dir, site) = primed_site(4, 4);
    let path = match kind {
        "hit_shallow" => "section-0",
        "hit_deep" => "section-0/section-0/section-0/section-0",
        "miss" => "nonexistent/path",
        _ => unreachable!(),
    };
    bencher.bench(|| site.has_page(black_box(path)));
}

/// Breadcrumb resolution at varying depths (depth 5, breadth 3). No leading
/// slash, so the paths resolve to real pages and walk a full trail.
#[divan::bench(args = ["depth_2", "depth_5"])]
fn breadcrumbs(bencher: Bencher, depth: &str) {
    let (_dir, site) = primed_site(5, 3);
    let path = match depth {
        "depth_2" => "section-0/section-0",
        "depth_5" => "section-0/section-0/section-0/section-0/section-0",
        _ => unreachable!(),
    };
    bencher.bench(|| site.get_breadcrumbs(black_box(path)));
}

/// Building the navigation tree from a primed (cached) site.
#[divan::bench(args = ["d2_b5", "d3_b4", "d4_b3"])]
fn navigation(bencher: Bencher, shape: &str) {
    let (depth, breadth) = match shape {
        "d2_b5" => (2, 5),
        "d3_b4" => (3, 4),
        "d4_b3" => (4, 3),
        _ => unreachable!(),
    };
    let (_dir, site) = primed_site(depth, breadth);
    bencher.bench(|| site.navigation(None));
}

/// Reload cost: cached (nothing changed) vs. forced re-scan after invalidation.
/// Depth 3, breadth 5.
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

/// Cold build: scan and index a site from scratch on each sample (setup — file
/// tree creation — is excluded from timing). Small ~15, medium ~85, large ~341
/// pages.
#[divan::bench(args = ["small", "medium", "large"])]
fn build_from_scratch(bencher: Bencher, size: &str) {
    let (depth, breadth) = match size {
        "small" => (2, 3),
        "medium" => (3, 4),
        "large" => (4, 4),
        _ => unreachable!(),
    };
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().join("docs");
    create_site_structure(&source_dir, depth, breadth);
    bencher
        .with_inputs(|| create_site(source_dir.clone()))
        .bench_values(|site| site.navigation(None));
}
