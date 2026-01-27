//! Benchmarks for page rendering performance.

#![allow(clippy::format_push_string)] // Benchmark setup code, performance not critical

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rw_site::{PageRenderer, PageRendererConfig};
use rw_storage::FsStorage;

fn create_renderer(source_dir: PathBuf) -> PageRenderer {
    let storage = Arc::new(FsStorage::new(source_dir));
    let config = PageRendererConfig::default();
    PageRenderer::new(storage, config)
}

fn create_renderer_with_config(source_dir: PathBuf, config: PageRendererConfig) -> PageRenderer {
    let storage = Arc::new(FsStorage::new(source_dir));
    PageRenderer::new(storage, config)
}

/// Generate markdown content with specified structure.
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

fn bench_render_simple(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();
    fs::write(source_dir.join("simple.md"), "# Hello\n\nSimple content.").unwrap();

    let renderer = create_renderer(source_dir);

    c.bench_function("render_simple_markdown", |b| {
        b.iter(|| renderer.render(Path::new("simple.md"), "simple"));
    });
}

fn bench_render_with_toc(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();
    let markdown = generate_markdown(10, 2);
    fs::write(source_dir.join("toc.md"), &markdown).unwrap();

    let config = PageRendererConfig {
        extract_title: true,
        ..Default::default()
    };
    let renderer = create_renderer_with_config(source_dir, config);

    c.bench_function("render_with_toc_10_headings", |b| {
        b.iter(|| renderer.render(Path::new("toc.md"), "toc"));
    });
}

fn bench_render_varying_sizes(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();
    let renderer = create_renderer(source_dir.clone());

    let mut group = c.benchmark_group("render_by_size");

    for (headings, paragraphs) in [(5, 2), (20, 3), (50, 5)] {
        let markdown = generate_markdown(headings, paragraphs);
        let filename = format!("doc_{headings}_{paragraphs}.md");
        fs::write(source_dir.join(&filename), &markdown).unwrap();

        let size = markdown.len();
        let rel_path = PathBuf::from(&filename);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("markdown", format!("{headings}h_{paragraphs}p")),
            &rel_path,
            |b, path| b.iter(|| renderer.render(path, "test")),
        );
    }

    group.finish();
}

fn bench_render_cached_vs_uncached(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();
    let cache_dir = temp_dir.path().join("cache");
    fs::write(source_dir.join("cached.md"), generate_markdown(10, 3)).unwrap();

    // Uncached renderer
    let uncached_renderer = create_renderer(source_dir.clone());

    // Cached renderer
    let cached_config = PageRendererConfig {
        cache_dir: Some(cache_dir),
        version: "1.0.0".to_string(),
        ..Default::default()
    };
    let cached_renderer = create_renderer_with_config(source_dir, cached_config);

    let mut group = c.benchmark_group("caching");

    group.bench_function("render_uncached", |b| {
        b.iter(|| uncached_renderer.render(Path::new("cached.md"), "test"));
    });

    // Prime the cache
    let _ = cached_renderer.render(Path::new("cached.md"), "cached");

    group.bench_function("render_cache_hit", |b| {
        b.iter(|| cached_renderer.render(Path::new("cached.md"), "cached"));
    });

    group.finish();
}

fn bench_render_gfm_features(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();

    let markdown = r"# GFM Features

| Column A | Column B | Column C |
|----------|----------|----------|
| Value 1  | Value 2  | Value 3  |
| Value 4  | Value 5  | Value 6  |

- [x] Completed task
- [ ] Pending task
- [ ] Another task

This has ~~strikethrough~~ and **bold** and *italic*.
";
    fs::write(source_dir.join("gfm.md"), markdown).unwrap();

    let renderer = create_renderer(source_dir);

    c.bench_function("render_gfm_features", |b| {
        b.iter(|| renderer.render(Path::new("gfm.md"), "gfm"));
    });
}

fn bench_render_code_blocks(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();

    let markdown = r#"# Code Examples

## Rust

```rust
fn main() {
    println!("Hello, world!");
    let x = 42;
    for i in 0..10 {
        println!("{}", i * x);
    }
}
```

## Python

```python
def greet(name):
    return f"Hello, {name}!"

if __name__ == "__main__":
    print(greet("World"))
```

## JavaScript

```javascript
function fibonacci(n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

console.log(fibonacci(10));
```
"#;
    fs::write(source_dir.join("code.md"), markdown).unwrap();

    let renderer = create_renderer(source_dir);

    c.bench_function("render_code_blocks", |b| {
        b.iter(|| renderer.render(Path::new("code.md"), "code"));
    });
}

fn bench_render_large_document(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_dir = temp_dir.path().to_path_buf();
    let markdown = generate_markdown(100, 5); // ~100KB document
    fs::write(source_dir.join("large.md"), &markdown).unwrap();

    let renderer = create_renderer(source_dir);

    let mut group = c.benchmark_group("large_document");
    group.throughput(Throughput::Bytes(markdown.len() as u64));
    group.bench_function("render", |b| {
        b.iter(|| renderer.render(Path::new("large.md"), "large"));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_render_simple,
    bench_render_with_toc,
    bench_render_varying_sizes,
    bench_render_cached_vs_uncached,
    bench_render_gfm_features,
    bench_render_code_blocks,
    bench_render_large_document,
);

criterion_main!(benches);
