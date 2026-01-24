//! Benchmarks for site structure operations.

use std::fs;
use std::path::Path;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rw_site::{SiteLoader, SiteLoaderConfig};

/// Create a site structure with specified depth and breadth.
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

fn bench_site_get_page(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 3, 5);

    let config = SiteLoaderConfig {
        source_dir,
        cache_dir: None,
    };
    let loader = SiteLoader::new(config);
    let site = loader.reload_if_needed();

    let mut group = c.benchmark_group("site_lookup");

    group.bench_function("get_page_hit", |b| {
        b.iter(|| site.get_page_by_source(Path::new("section-0/section-1/index.md")))
    });

    group.bench_function("get_page_miss", |b| {
        b.iter(|| site.get_page_by_source(Path::new("nonexistent/path.md")))
    });

    group.finish();
}

fn bench_site_breadcrumbs(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 5, 3);

    let config = SiteLoaderConfig {
        source_dir,
        cache_dir: None,
    };
    let loader = SiteLoader::new(config);
    let site = loader.reload_if_needed();

    let mut group = c.benchmark_group("breadcrumbs");

    group.bench_function("depth_2", |b| {
        b.iter(|| site.get_breadcrumbs("/section-0/section-0"))
    });

    group.bench_function("depth_5", |b| {
        b.iter(|| site.get_breadcrumbs("/section-0/section-0/section-0/section-0/section-0"))
    });

    group.finish();
}

fn bench_site_navigation(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut group = c.benchmark_group("navigation");

    for (depth, breadth) in [(2, 5), (3, 4), (4, 3)] {
        let source_dir = temp_dir.path().join(format!("docs_{depth}_{breadth}"));
        create_site_structure(&source_dir, depth, breadth);

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);
        let site = loader.reload_if_needed();

        group.bench_with_input(
            BenchmarkId::new("build_tree", format!("d{depth}_b{breadth}")),
            &site,
            |b, site| b.iter(|| site.navigation()),
        );
    }

    group.finish();
}

fn bench_siteloader_reload(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 3, 5);

    let config = SiteLoaderConfig {
        source_dir,
        cache_dir: None,
    };
    let loader = SiteLoader::new(config);

    let mut group = c.benchmark_group("siteloader");

    // Prime the cache
    let _ = loader.reload_if_needed();

    group.bench_function("reload_cached", |b| {
        b.iter(|| loader.reload_if_needed())
    });

    group.bench_function("reload_after_invalidate", |b| {
        b.iter(|| {
            loader.invalidate();
            loader.reload_if_needed()
        })
    });

    group.finish();
}

fn bench_siteloader_varying_sizes(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut group = c.benchmark_group("siteloader_size");

    // Small: ~15 pages, Medium: ~85 pages, Large: ~341 pages
    for (depth, breadth, label) in [(2, 3, "small"), (3, 4, "medium"), (4, 4, "large")] {
        let source_dir = temp_dir.path().join(format!("docs_{label}"));
        create_site_structure(&source_dir, depth, breadth);

        group.bench_function(label, |b| {
            b.iter_with_setup(
                || {
                    SiteLoader::new(SiteLoaderConfig {
                        source_dir: source_dir.clone(),
                        cache_dir: None,
                    })
                },
                |loader| loader.reload_if_needed(),
            )
        });
    }

    group.finish();
}

fn bench_resolve_source_path(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().join("docs");
    create_site_structure(&source_dir, 4, 4);

    let config = SiteLoaderConfig {
        source_dir,
        cache_dir: None,
    };
    let loader = SiteLoader::new(config);
    let site = loader.reload_if_needed();

    let mut group = c.benchmark_group("resolve_path");

    group.bench_function("shallow", |b| {
        b.iter(|| site.resolve_source_path("/section-0"))
    });

    group.bench_function("deep", |b| {
        b.iter(|| site.resolve_source_path("/section-0/section-0/section-0/section-0"))
    });

    group.bench_function("not_found", |b| {
        b.iter(|| site.resolve_source_path("/nonexistent"))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_site_get_page,
    bench_site_breadcrumbs,
    bench_site_navigation,
    bench_siteloader_reload,
    bench_siteloader_varying_sizes,
    bench_resolve_source_path,
);

criterion_main!(benches);
