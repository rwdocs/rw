//! Benchmarks for the markdown -> HTML render hot path.
//!
//! This is `rw`'s central per-request cost: the engine renders pages on demand,
//! so `MarkdownRenderer::render` runs once for every page view. The `page`
//! fixtures are `rw`'s own documentation pages, so those numbers reflect real
//! content — frontmatter, headings, tables, fenced code, and links. A separate
//! `features` bench covers constructs the real docs happen not to use (GitHub
//! alerts, task lists, strikethrough) so those render paths stay measured too.
//!
//! Local:      cargo bench -p rw-renderer --bench render
//! Under CI:   cargo codspeed run   (the `divan` dep is the `CodSpeed` compat shim,
//!             so the same file is instrumented instead of wall-timed)

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

/// `rw`'s real documentation pages, embedded at compile time — the single source
/// of both the `args` labels and the markdown looked up by [`markdown_for`].
/// Each exercises a different mix of markdown features, so a regression in any
/// one construct (tables, code fences, frontmatter) shows up in at least one row.
const PAGES: &[(&str, &str)] = &[
    ("metadata", include_str!("../../../docs/metadata.md")),
    ("diagrams", include_str!("../../../docs/diagrams.md")),
    ("confluence", include_str!("../../../docs/confluence.md")),
    ("comment-cli", include_str!("../../../docs/comment-cli.md")),
    (
        "configuration",
        include_str!("../../../docs/configuration.md"),
    ),
    ("embedding", include_str!("../../../docs/embedding.md")),
];

/// Markdown constructs the real doc corpus does not contain, kept as a dedicated
/// fixture so their render paths stay benched: GitHub alerts, task lists, and
/// strikethrough.
const FEATURES_DOC: &str = "\
# Feature coverage

> [!NOTE]
> A note alert — the real docs don't use these.

> [!WARNING]
> A second alert kind, to hit the other branch.

- [x] a completed task
- [ ] a pending task
- [ ] a ~~dropped~~ pending task

A paragraph with ~~strikethrough~~ and **bold** and `code`.
";

fn markdown_for(name: &str) -> &'static str {
    PAGES
        .iter()
        .find(|(page, _)| *page == name)
        .expect("known page name")
        .1
}

/// Build a renderer configured like the real serving path: title extraction is
/// on in production (`PageRendererConfig::extract_title` defaults true).
fn renderer() -> MarkdownRenderer<HtmlBackend> {
    MarkdownRenderer::<HtmlBackend>::new().with_title_extraction()
}

/// Render each real doc page, one bench row per page. The renderer is built once
/// (outside the timed loop, matching the server which reuses one), and only
/// `.render` is measured; a fresh `Pipeline` is required per call.
#[divan::bench(args = PAGES.iter().map(|&(name, _)| name))]
fn page(bencher: Bencher, name: &str) {
    let renderer = renderer();
    let markdown = markdown_for(name);
    bencher.bench(|| renderer.render(black_box(markdown), Pipeline::new()));
}

/// Render markdown constructs the real corpus lacks (alerts, task lists,
/// strikethrough), so those branches stay measured.
#[divan::bench]
fn features(bencher: Bencher) {
    let renderer = renderer();
    bencher.bench(|| renderer.render(black_box(FEATURES_DOC), Pipeline::new()));
}

/// Whole-corpus render: the cost of rendering every doc page back-to-back,
/// a proxy for a cold-cache walk of the site.
#[divan::bench]
fn all_pages(bencher: Bencher) {
    let renderer = renderer();
    bencher.bench(|| {
        for (_, markdown) in PAGES {
            black_box(renderer.render(black_box(markdown), Pipeline::new()));
        }
    });
}
