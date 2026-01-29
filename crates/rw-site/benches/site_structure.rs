//! Benchmarks for site structure operations.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rw_site::{Site, SiteConfig};
use rw_storage::FsStorage;

fn create_site(source_dir: PathBuf) -> Site {
    let storage = Arc::new(FsStorage::new(source_dir));
    let config = SiteConfig::default();
    Site::new(storage, config)
}

/// Create a site structure with specified depth and breadth.
fn create_site_structure(root: &std::path::Path, depth: usize, breadth: usize) {
    fn create_level(dir: &std::path::Path, current_depth: usize, max_depth: usize, breadth: usize) {
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

fn bench_site_get_page(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 3, 5);

    let site = create_site(source_dir);
    let _ = site.navigation();

    let mut group = c.benchmark_group("site_lookup");

    group.bench_function("get_page_hit", |b| {
        b.iter(|| site.get_page_by_source(std::path::Path::new("section-0/section-1/index.md")));
    });

    group.bench_function("get_page_miss", |b| {
        b.iter(|| site.get_page_by_source(std::path::Path::new("nonexistent/path.md")));
    });

    group.finish();
}

fn bench_site_breadcrumbs(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 5, 3);

    let site = create_site(source_dir);
    let _ = site.navigation();

    let mut group = c.benchmark_group("breadcrumbs");

    group.bench_function("depth_2", |b| {
        b.iter(|| site.get_breadcrumbs("/section-0/section-0"));
    });

    group.bench_function("depth_5", |b| {
        b.iter(|| site.get_breadcrumbs("/section-0/section-0/section-0/section-0/section-0"));
    });

    group.finish();
}

fn bench_site_navigation(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut group = c.benchmark_group("navigation");

    for (depth, breadth) in [(2, 5), (3, 4), (4, 3)] {
        let source_dir = temp_dir.path().join(format!("docs_{depth}_{breadth}"));
        create_site_structure(&source_dir, depth, breadth);

        let site = create_site(source_dir);
        let _ = site.navigation();

        group.bench_with_input(
            BenchmarkId::new("build_tree", format!("d{depth}_b{breadth}")),
            &site,
            |b, site| b.iter(|| site.navigation()),
        );
    }

    group.finish();
}

fn bench_site_reload(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 3, 5);

    let site = create_site(source_dir);

    let mut group = c.benchmark_group("site");

    // Prime the cache
    let _ = site.navigation();

    group.bench_function("reload_cached", |b| b.iter(|| site.navigation()));

    group.bench_function("reload_after_invalidate", |b| {
        b.iter(|| {
            site.invalidate();
            let _ = site.navigation();
        });
    });

    group.finish();
}

fn bench_site_varying_sizes(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut group = c.benchmark_group("site_size");

    // Small: ~15 pages, Medium: ~85 pages, Large: ~341 pages
    for (depth, breadth, label) in [(2, 3, "small"), (3, 4, "medium"), (4, 4, "large")] {
        let source_dir = temp_dir.path().join(format!("docs_{label}"));
        create_site_structure(&source_dir, depth, breadth);

        group.bench_function(label, |b| {
            b.iter_with_setup(|| create_site(source_dir.clone()), |site| site.navigation());
        });
    }

    group.finish();
}

fn bench_get_page(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 4, 4);

    let site = create_site(source_dir);
    let _ = site.navigation();

    let mut group = c.benchmark_group("get_page");

    group.bench_function("shallow", |b| {
        b.iter(|| site.get_page("/section-0"));
    });

    group.bench_function("deep", |b| {
        b.iter(|| site.get_page("/section-0/section-0/section-0/section-0"));
    });

    group.bench_function("not_found", |b| {
        b.iter(|| site.get_page("/nonexistent"));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_site_get_page,
    bench_site_breadcrumbs,
    bench_site_navigation,
    bench_site_reload,
    bench_site_varying_sizes,
    bench_get_page,
);

criterion_main!(benches);
