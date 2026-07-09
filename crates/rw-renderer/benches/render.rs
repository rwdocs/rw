//! Benchmark for the markdown -> HTML render hot path.
//!
//! This is `rw`'s central per-request cost: the engine renders pages on demand,
//! so `MarkdownRenderer::render` runs once for every page view.
//!
//! One coarse gate on the whole render, deliberately — not a per-construct
//! matrix. Splitting by markdown construct (tables, code, alerts, …) would
//! mostly bench pulldown-cmark's parsing surface, not rw's own logic. What is
//! rw's own is exercised here: syntax highlighting, heading anchors + ToC, and
//! the directive processor. The pipeline mirrors the production HTML serving
//! path (`rw_site`'s page renderer): a `DirectiveProcessor` with the `:::tab`
//! container and inline `:status` badge registered. (The production path also
//! adds a Kroki diagram processor, omitted here since it needs the network.)
//! When a specific rw path is identified as hot or gets optimized, add a
//! targeted bench for it then, rather than partitioning speculatively now.
//!
//! The fixture is a frozen, realistic doc page (`benches/fixtures/page.md`) — not
//! the project's live `docs/`, so a documentation edit can't shift the baseline.
//!
//! Local:      cargo bench -p rw-renderer --bench render
//! Under CI:   cargo codspeed run   (the `divan` dep is the CodSpeed compat shim,
//!             so the same file is instrumented instead of wall-timed)

#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and GitHub-flavored terms

use divan::{Bencher, black_box};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline, StatusDirective, TabsDirective};

fn main() {
    divan::main();
}

// Report heap traffic (allocations + bytes) alongside timing. Under CodSpeed
// instrumentation this is harmless; locally it's how you spot a change that
// allocates more even when wall time stays flat.
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

/// A frozen, realistic doc page (see module docs) — the render input.
const PAGE: &str = include_str!("fixtures/page.md");

/// The directive processor the production HTML path installs (`rw_site`
/// `create_directives_pipeline`): `:::tab` container + inline `:status` badge.
/// Built fresh per render, as production does — the render consumes the pipeline.
fn pipeline() -> Pipeline {
    let directives = DirectiveProcessor::new()
        .with_container(TabsDirective::new())
        .with_inline(StatusDirective::new());
    Pipeline::new().with_directives(directives)
}

/// Render the fixture through the full pipeline. The renderer is built once
/// (outside the timed loop, matching the server which reuses one) and configured
/// like the real serving path (title extraction is on in production); only
/// `.render` is measured.
#[divan::bench]
fn render(bencher: Bencher) {
    let renderer = MarkdownRenderer::<HtmlBackend>::new().with_title_extraction();
    bencher.bench(|| renderer.render(black_box(PAGE), pipeline()));
}
