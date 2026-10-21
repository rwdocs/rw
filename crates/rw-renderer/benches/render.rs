//! Benchmark for the markdown -> HTML render hot path: one
//! walk-resolve-finish round trip per page view.
//!
//! The two fixtures are the same documentation page, differing only in the
//! script of their prose: `page_latin.md` is pure ASCII, `page_mixed.md` is
//! ~47% non-ASCII. Script is where this renderer's costs diverge most, and a
//! blended fixture averages that away. Keep `page_mixed.md`'s mix: prose
//! mostly non-Latin, tables and code mostly ASCII, since those regions run
//! different code.
//!
//! `StubDiagrams` stands in for a network-bound diagram provider, so the
//! request/hole/fill splice is measured too.
//!
//! Local:      cargo bench -p rw-renderer --bench render
//! Under CI:   cargo codspeed run   (the `divan` dep is the CodSpeed compat shim,
//!             so the same file is instrumented instead of wall-timed)

#![allow(clippy::doc_markdown)] // Product names (CodSpeed) and GitHub-flavored terms

use std::sync::Arc;

use divan::{Bencher, black_box};
use rw_diagrams::{
    Asset, DiagramContent, DiagramError, DiagramProvider, DiagramRequest, DiagramRouter, Providers,
    ResolveContext, Resolved,
};
use rw_renderer::{HtmlBackend, MarkdownRenderer, RenderResult};

fn main() {
    divan::main();
}

// Reports allocations alongside timing, to catch a change that allocates
// more at flat wall time.
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

/// Pure-ASCII page (see module docs).
const PAGE_LATIN: &str = include_str!("fixtures/page_latin.md");

/// Structurally identical page whose prose is ~47% non-ASCII (see module docs).
const PAGE_MIXED: &str = include_str!("fixtures/page_mixed.md");

/// Offline diagram provider with a fixed payload.
struct StubDiagrams;

/// Roughly the size of a rendered PlantUML sequence diagram, so the fill splice
/// moves a realistic number of bytes.
const STUB_SVG: &str = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 320">"#,
    r#"<g class="node"><rect x="8" y="8" width="120" height="40" rx="4"/>"#,
    r#"<text x="68" y="32" text-anchor="middle">Client</text></g>"#,
    r#"<g class="node"><rect x="176" y="8" width="120" height="40" rx="4"/>"#,
    r#"<text x="236" y="32" text-anchor="middle">Gateway</text></g>"#,
    r#"<path d="M128 28 L176 28" marker-end="url(#arrow)"/>"#,
    r#"</svg>"#,
);

impl DiagramProvider for StubDiagrams {
    fn handles(&self, language: &str) -> bool {
        language == "plantuml" || language == "mermaid"
    }

    fn resolve(
        &self,
        requests: &[&DiagramRequest],
        _ctx: &ResolveContext<'_>,
    ) -> Vec<Result<Resolved, DiagramError>> {
        requests
            .iter()
            .map(|_| {
                Ok(Resolved {
                    asset: Asset::Inline(DiagramContent::Svg(STUB_SVG.to_owned())),
                    size: None,
                    digest: "0".to_owned(),
                    warnings: Vec::new(),
                })
            })
            .collect()
    }
}

/// The providers the production HTML path installs: one diagram provider.
fn providers() -> Providers {
    Providers::empty().with(Arc::new(StubDiagrams) as Arc<dyn DiagramProvider>)
}

/// The renderer the production HTML path installs: fences routed through
/// `providers`.
fn renderer(providers: &Providers) -> MarkdownRenderer<HtmlBackend> {
    MarkdownRenderer::<HtmlBackend>::new()
        .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>)
}

/// The production round trip: walk, resolve the diagram requests, finish.
fn render(
    renderer: &MarkdownRenderer<HtmlBackend>,
    providers: &Providers,
    page: &str,
) -> RenderResult {
    let pass = renderer.begin(page);
    let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
    pass.finish(&resolutions)
}

/// Render the ASCII fixture. The renderer and providers are built outside the
/// timed loop, as the server reuses both.
#[divan::bench]
fn render_latin(bencher: Bencher) {
    let providers = providers();
    let renderer = renderer(&providers);
    bencher.bench(|| render(&renderer, &providers, black_box(PAGE_LATIN)));
}

/// Render the mixed-script fixture — same page, non-ASCII prose. A gap that
/// opens up between this and `render_latin` is a script-specific regression.
#[divan::bench]
fn render_mixed(bencher: Bencher) {
    let providers = providers();
    let renderer = renderer(&providers);
    bencher.bench(|| render(&renderer, &providers, black_box(PAGE_MIXED)));
}
