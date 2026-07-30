//! Benchmark for the markdown -> HTML render hot path.
//!
//! This is `rw`'s central per-request cost: the engine renders pages on demand,
//! so `MarkdownRenderer::render` runs once for every page view.
//!
//! # Why two fixtures and not one
//!
//! The two fixtures are the *same page* — identical heading count, tables, code
//! fences, list items, and directives — differing only in the script of their
//! prose. `page_latin.md` is pure ASCII; `page_mixed.md` is ~47% non-ASCII
//! (Russian-dominant, with CJK and accented-Latin sections).
//!
//! They are benched separately because script is the axis along which this
//! renderer's costs diverge most sharply, and a single blended fixture averages
//! that away. Two concrete cases, both measured on this renderer:
//!
//! * `slugify` runs on every heading, and on non-Latin headings most of its
//!   time goes to `core::unicode`'s alphabetic table lookup — which ASCII text
//!   never reaches, because `char::is_alphabetic` fast-paths ASCII before
//!   consulting it. The same heading costs several times more in Cyrillic than
//!   in Latin, and only one of these two fixtures can see that.
//! * Replacing `escape_into`'s byte loop with `memchr` measured *5.1x faster*
//!   on Russian prose in isolation while making end-to-end rendering slower.
//!   A blended fixture would have reported that change as roughly neutral.
//!
//! This is deliberately a workload axis, not the per-construct matrix the
//! renderer does not want: splitting by markdown construct (tables, code,
//! alerts, …) would mostly bench pulldown-cmark's parsing surface rather than
//! rw's own logic.
//!
//! # What the fixtures model
//!
//! Both are shaped like an ordinary documentation page — ~5 KB, 8 headings, 14
//! table rows, 2 code fences, 6 list items, one alert, one link — rather than
//! like whatever constructs were convenient to write. Prose dominates, as it
//! does on a real page, and the markup is the mix a technical page actually
//! carries.
//!
//! `page_mixed.md` varies script *within* the page instead of translating it
//! wholesale: headings and prose are mostly non-Latin, tables and code fences
//! much less so, because identifiers, defaults and commands stay in ASCII even
//! in documentation written in another language. Those regions run different
//! code, so the mix matters as much as the overall proportion.
//!
//! The measured call mirrors the production HTML serving path (`rw_site`'s page
//! renderer): a diagram provider routing the page's diagram fences.
//!
//! `StubDiagrams` stands in for `rw-kroki`, which needs the network and so
//! cannot be benched directly. It reproduces a provider's *contract* — claim the
//! fence language, then answer each request with rendered content — so the
//! request/hole/fill splice runs here exactly as it does in production.
//! Diagrams are a headline feature and a diagram-heavy page is the normal case,
//! so leaving that path unmeasured left a gate-sized hole in the gate. The
//! stub's payload is a fixed string: no network, fully deterministic.
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
use rw_renderer::{HtmlBackend, MarkdownRenderer};

fn main() {
    divan::main();
}

// Report heap traffic (allocations + bytes) alongside timing. Under CodSpeed
// instrumentation this is harmless; locally it's how you spot a change that
// allocates more even when wall time stays flat.
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

/// Pure-ASCII page (see module docs).
const PAGE_LATIN: &str = include_str!("fixtures/page_latin.md");

/// Structurally identical page whose prose is ~47% non-ASCII (see module docs).
const PAGE_MIXED: &str = include_str!("fixtures/page_mixed.md");

/// Offline stand-in for `rw-kroki`'s provider: same contract, fixed payload, no
/// network. See module docs.
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
        requests: &[DiagramRequest],
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

/// The providers the production HTML path installs: one diagram provider. Built
/// once, outside the timed loop, as the server does.
fn providers() -> Providers {
    Providers::empty().with(Arc::new(StubDiagrams) as Arc<dyn DiagramProvider>)
}

/// The renderer the production HTML path installs: title extraction on, fences
/// routed through `providers`.
fn renderer(providers: &Providers) -> MarkdownRenderer<HtmlBackend> {
    MarkdownRenderer::<HtmlBackend>::new()
        .with_title_extraction()
        .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>)
}

/// Render the ASCII fixture. The renderer and providers are built once (outside
/// the timed loop, matching the server which reuses both); only `.render` is
/// measured.
#[divan::bench]
fn render_latin(bencher: Bencher) {
    let providers = providers();
    let renderer = renderer(&providers);
    bencher.bench(|| renderer.render(black_box(PAGE_LATIN), &providers));
}

/// Render the mixed-script fixture — same page, non-ASCII prose. A gap that
/// opens up between this and `render_latin` is a script-specific regression.
#[divan::bench]
fn render_mixed(bencher: Bencher) {
    let providers = providers();
    let renderer = renderer(&providers);
    bencher.bench(|| renderer.render(black_box(PAGE_MIXED), &providers));
}
