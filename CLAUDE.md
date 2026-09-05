# CLAUDE.md

Before committing:
- Run `make format` to format all code

After code changes:
- Update CHANGELOG.md — only add user-facing changes (new features, behavior changes, bug fixes users would notice). Skip internal refactors, code quality fixes, and clippy cleanups.
- Check CLAUDE.md and README.md for outdated or missing information and fix

## Project Overview

RW is a documentation engine with no build step. It renders CommonMark documents
on demand — pages are rendered when requested, not ahead of time. Also publishes
to Confluence. Also supports embedding in Backstage via native plugins.

## Development Commands

```bash
make build                # Build frontend and CLI
make test                 # Run all tests with coverage including doctests (Rust, Frontend)
make format               # Format all code (Rust, Frontend)
make lint                 # Lint all code (clippy, svelte-check, eslint, knip, publint/attw)
make audit                # Check the lockfile against the license and source policy (cargo-deny)

# Run the CLI
cargo build -p rw && ./target/debug/rw serve

# Frontend dev server
npm -w @rwdocs/viewer run dev
```

## Architecture

**Data flow (Confluence)**: Markdown → Rust (rw-parser tokenizing, walk
reserving a hole per diagram fence, Kroki resolving those diagrams, Confluence
rendering, attachment upload, API calls) → Confluence

**Data flow (HTML)**: Markdown → Rust (rw-parser tokenizing, walk reserving a
hole per diagram fence, Kroki resolving those diagrams, HTML rendering with
syntax highlighting, ToC generation, HTTP serving) → Browser

**Two-phase diagram rendering**: `rw-renderer` walks a document and hands back a
`RenderPass` listing every diagram fence as a `DiagramRequest`; the caller
resolves those through `rw-diagrams`' `Providers` (`rw-kroki` is the only
provider today) and `RenderPass::finish` turns each resolution into markup
through the `RenderBackend`. Providers produce *content* — bytes, a size, a
digest — never markup, so `rw-kroki` does not depend on `rw-renderer`. A backend
decides the markup for every construct the renderer knows. A caller supplies
inputs alongside it — `Sections`, a `TitleResolver`, and the `DiagramRouter`
that names which fence languages are diagrams — but none of them produce markup.

**Data flow (NAPI)**: Node.js → rw-napi (napi-rs bindings) → rw-site, rw-renderer,
rw-kroki (Rust) → Node.js objects

**Metadata flow**: Markdown + sidecar → `rw-meta::Meta` → shared `Arc<Meta>`
in `rw-storage::Document` → `rw-site::SiteState` → `PageRenderResult`. The site
stores each `Document` directly and returns the same metadata allocation through
fresh renders and cache hits; boundary adapters project their existing public
shapes. Storage serialization keeps the existing flattened wire format.

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.97+. `rust-toolchain.toml` pins the
  exact channel; `rust-version` in the workspace manifest tracks it, so bump both
  together
- **Node requirements**: `^22.22.2 || >=24.15.0`, declared identically in the
  root `package.json` and in `packages/viewer`. It tracks the dev toolchain, not
  the shipped artifact — this branch keeps `eslint-plugin-regexp` 3.1.1 and
  `jsdoc-type-pratt-parser` 7.3.0, while the synchronized declarations prepare
  for the separate pending PR #840 where `eslint-plugin-regexp` 3.2 resolves
  `jsdoc-type-pratt-parser` 9.1.2. Those parser-set patch floors also exclude
  the 23 line, while the viewer's runtime dependencies declare no `engines` at
  all. The root `package.json`, `packages/viewer/package.json`, and the two
  corresponding `package-lock.json` workspace records must move together. Node
  20 remains excluded deliberately because it reached end of life on
  2026-04-30.
- **PlantUML**: A `plantuml` fence becomes a diagram request resolved by
  `rw-kroki` — inline SVG by default. `rw-confluence` appends `format=png`,
  writes the bytes into the bundle directory, and uploads them as attachments
- **PlantUML preprocessing**: `!include` resolution, meta-include C4 macro
  emission, and search-text stripping live in `rw-plantuml`, which has no HTTP
  client. `rw backstage publish` resolves includes through it, so
  `rw-storage-s3`'s `publish` feature needs no diagram-rendering crate. A binary
  that also renders diagrams — `rw` does, via `rw-site`/`rw-confluence` — links
  `rw-kroki` anyway.
- **Diagram id isolation**: Kroki generators emit SVG ids unique only within one
  diagram, so `HtmlBackend` wraps each inlined SVG in `<rw-diagram>` and the
  viewer attaches a shadow root — one id scope per diagram. Consequence: `querySelector`
  does not reach diagram internals. Use `lib/diagram/source.ts`'s `diagramSource`
  / `diagramShadowRoots` rather than open-coding a traversal. A hand-authored
  `<figure class="diagram">` in markdown gets no wrapper and is unisolated by
  design; both shapes must keep working.
