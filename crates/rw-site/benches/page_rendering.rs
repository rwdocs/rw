//! Benchmarks for page rendering performance.

#![allow(clippy::format_push_string)] // Benchmark setup code, performance not critical

use std::fs;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rw_site::{PageRenderer, PageRendererConfig};

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
    let file_path = temp_dir.path().join("simple.md");
    fs::write(&file_path, "# Hello\n\nSimple content.").unwrap();

    let config = PageRendererConfig::default();
    let renderer = PageRenderer::new(config);

    c.bench_function("render_simple_markdown", |b| {
        b.iter(|| renderer.render(&file_path, "simple"));
    });
}

fn bench_render_with_toc(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("toc.md");
    let markdown = generate_markdown(10, 2);
    fs::write(&file_path, &markdown).unwrap();

    let config = PageRendererConfig {
        extract_title: true,
        ..Default::default()
    };
    let renderer = PageRenderer::new(config);

    c.bench_function("render_with_toc_10_headings", |b| {
        b.iter(|| renderer.render(&file_path, "toc"));
    });
}

fn bench_render_varying_sizes(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = PageRendererConfig::default();
    let renderer = PageRenderer::new(config);

    let mut group = c.benchmark_group("render_by_size");

    for (headings, paragraphs) in [(5, 2), (20, 3), (50, 5)] {
        let markdown = generate_markdown(headings, paragraphs);
        let file_path = temp_dir
            .path()
            .join(format!("doc_{headings}_{paragraphs}.md"));
        fs::write(&file_path, &markdown).unwrap();

        let size = markdown.len();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("markdown", format!("{headings}h_{paragraphs}p")),
            &file_path,
            |b, path| b.iter(|| renderer.render(path, "test")),
        );
    }

    group.finish();
}

fn bench_render_cached_vs_uncached(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    let file_path = temp_dir.path().join("cached.md");
    fs::write(&file_path, generate_markdown(10, 3)).unwrap();

    // Uncached renderer
    let uncached_config = PageRendererConfig::default();
    let uncached_renderer = PageRenderer::new(uncached_config);

    // Cached renderer
    let cached_config = PageRendererConfig {
        cache_dir: Some(cache_dir),
        version: "1.0.0".to_string(),
        ..Default::default()
    };
    let cached_renderer = PageRenderer::new(cached_config);

    let mut group = c.benchmark_group("caching");

    group.bench_function("render_uncached", |b| {
        b.iter(|| uncached_renderer.render(&file_path, "test"));
    });

    // Prime the cache
    let _ = cached_renderer.render(&file_path, "cached");

    group.bench_function("render_cache_hit", |b| {
        b.iter(|| cached_renderer.render(&file_path, "cached"));
    });

    group.finish();
}

fn bench_render_gfm_features(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("gfm.md");

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
    fs::write(&file_path, markdown).unwrap();

    let config = PageRendererConfig::default();
    let renderer = PageRenderer::new(config);

    c.bench_function("render_gfm_features", |b| {
        b.iter(|| renderer.render(&file_path, "gfm"));
    });
}

fn bench_render_code_blocks(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("code.md");

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
    fs::write(&file_path, markdown).unwrap();

    let config = PageRendererConfig::default();
    let renderer = PageRenderer::new(config);

    c.bench_function("render_code_blocks", |b| {
        b.iter(|| renderer.render(&file_path, "code"));
    });
}

fn bench_render_large_document(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("large.md");
    let markdown = generate_markdown(100, 5); // ~100KB document
    fs::write(&file_path, &markdown).unwrap();

    let config = PageRendererConfig::default();
    let renderer = PageRenderer::new(config);

    let mut group = c.benchmark_group("large_document");
    group.throughput(Throughput::Bytes(markdown.len() as u64));
    group.bench_function("render", |b| {
        b.iter(|| renderer.render(&file_path, "large"));
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
