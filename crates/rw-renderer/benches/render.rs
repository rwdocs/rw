//! Benchmarks for the markdown -> HTML render hot path.
//!
//! This is `rw`'s central per-request cost: the engine renders pages on demand,
//! so `MarkdownRenderer::render` runs once for every page view.
//!
//! Fixtures live in `benches/fixtures/` and are dedicated, frozen bench inputs —
//! deliberately NOT the project's live `docs/`, so a documentation edit can't
//! shift the benchmark baseline (a gate must move only when the *code* changes).
//! Together they cover the render feature surface: `article` (frontmatter,
//! headings, prose, lists, links, blockquotes), `reference` (tables, multi-language
//! code fences, images, nested lists), and `gfm` (GitHub alerts, task lists,
//! strikethrough).
//!
//! Local:      cargo bench -p rw-renderer --bench render
//! Under CI:   cargo codspeed run   (the `divan` dep is the CodSpeed compat shim,
//!             so the same file is instrumented instead of wall-timed)

#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and GitHub-flavored terms

use divan::{Bencher, black_box};
use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};

fn main() {
    divan::main();
}

// Report heap traffic (allocations + bytes) alongside timing for every bench.
// Under CodSpeed instrumentation this is harmless; locally it's how you spot a
// change that allocates more even when wall time stays flat.
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

/// Dedicated, frozen bench fixtures (see module docs) — the single source of
/// both the `args` labels and the markdown looked up by [`markdown_for`].
const FIXTURES: &[(&str, &str)] = &[
    ("article", include_str!("fixtures/article.md")),
    ("reference", include_str!("fixtures/reference.md")),
    ("gfm", include_str!("fixtures/gfm.md")),
];

fn markdown_for(name: &str) -> &'static str {
    FIXTURES
        .iter()
        .find(|(fixture, _)| *fixture == name)
        .expect("known fixture name")
        .1
}

/// Build a renderer configured like the real serving path: title extraction is
/// on in production (`PageRendererConfig::extract_title` defaults true).
fn renderer() -> MarkdownRenderer<HtmlBackend> {
    MarkdownRenderer::<HtmlBackend>::new().with_title_extraction()
}

/// Render each fixture, one bench row per fixture. The renderer is built once
/// (outside the timed loop, matching the server which reuses one), and only
/// `.render` is measured; a fresh `Pipeline` is required per call.
#[divan::bench(args = FIXTURES.iter().map(|&(name, _)| name))]
fn page(bencher: Bencher, name: &str) {
    let renderer = renderer();
    let markdown = markdown_for(name);
    bencher.bench(|| renderer.render(black_box(markdown), Pipeline::new()));
}
