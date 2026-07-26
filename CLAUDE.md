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
make lint                 # Lint all code (clippy, svelte-check)

# Run the CLI
cargo build -p rw && ./target/debug/rw serve

# Frontend dev server
npm -w @rwdocs/viewer run dev
```

## Architecture

**Data flow (Confluence)**: Markdown → Rust (pulldown-cmark parsing, PlantUML
extraction, Confluence rendering, Kroki diagram rendering, API calls) → Confluence

**Data flow (HTML)**: Markdown → Rust (rw-parser tokenizing, HTML rendering
with syntax highlighting, ToC generation, HTTP serving) → Browser

**Data flow (NAPI)**: Node.js → rw-napi (napi-rs bindings) → rw-site, rw-renderer,
rw-kroki (Rust) → Node.js objects

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.96+
- **PlantUML**: Extracted from code blocks, rendered via Kroki, uploaded as attachments
- **PlantUML preprocessing**: `!include` resolution, meta-include C4 macro
  emission, and search-text stripping live in `rw-plantuml`, which has no HTTP
  client. `rw backstage publish` resolves includes through it, so
  `rw-storage-s3`'s `publish` feature needs no diagram-rendering crate. A binary
  that also renders diagrams — `rw` does, via `rw-site`/`rw-confluence` — links
  `rw-kroki` anyway.
- **Diagram id isolation**: Kroki generators emit SVG ids unique only within one
  diagram, so `rw-kroki` wraps each inlined SVG in `<rw-diagram>` and the viewer
  attaches a shadow root — one id scope per diagram. Consequence: `querySelector`
  does not reach diagram internals. Use `lib/diagram/source.ts`'s `diagramSource`
  / `diagramShadowRoots` rather than open-coding a traversal. A hand-authored
  `<figure class="diagram">` in markdown gets no wrapper and is unisolated by
  design; both shapes must keep working.
