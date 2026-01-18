# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Live reload** for development mode with WebSocket-based file watching
- **Diagram rendering** via Kroki (PlantUML, Mermaid, GraphViz, and 14+ other formats)
- **Confluence diagram support** for all 17 diagram types (previously PlantUML only)
- **GitHub Actions CI** with lint, test, and E2E jobs
- **Security headers** (CSP, X-Content-Type-Options, X-Frame-Options)
- **Cache version invalidation** to prevent stale content after upgrades
- **Configurable caching** with `--cache/--no-cache` CLI flag
- **Verbose mode** (`--verbose`) for diagram rendering warnings
- Support for `kroki-` prefixed diagram languages (MkDocs compatibility)
- Support for superscript and subscript in markdown

### Changed

- **Config parsing moved to Rust** for better performance and type safety
- **Unified configuration** via `docstage.toml` with auto-discovery
- **Confluence commands** moved to `docstage confluence <command>` subgroup
- **Clean URLs** without `/docs` prefix (e.g., `/guide` instead of `/docs/guide`)
- **Tailwind CSS v4** with CSS-based configuration
- **ty** replaces mypy for faster Python type checking
- Update pulldown-cmark from 0.12 to 0.13
- `ConvertResult` now includes `warnings` field for API consistency

### Fixed

- Root `index.md` now correctly renders as home page
- Article links resolve correctly without `/docs/` prefix
- Comment preservation works when table content changes
- Live reload triggers page refresh correctly
- Navigation updates on consecutive clicks
- Breadcrumbs exclude non-navigable paths and current page
- Diagram sizing displays at correct physical size
- Indented `!include` directives resolve in PlantUML

## [0.1.0] - 2025-12-05

Initial release as Docstage (renamed from md2conf).

### Added

- **Documentation server** with Svelte 5 frontend (Stripe-inspired design)
- **Markdown to HTML** conversion with syntax highlighting
- **Markdown to Confluence** conversion with XHTML output
- **Navigation sidebar** with collapsible tree structure
- **Table of contents** with scroll spy
- **Breadcrumbs** for page hierarchy
- **Mobile responsive** layout with hamburger menu
- **File-based caching** with mtime invalidation
- **PlantUML support** with `!include` resolution
- **Confluence publishing** via REST API with OAuth 1.0 RSA-SHA1
- **Comment preservation** when updating Confluence pages
- CLI commands: `serve`, `confluence convert|create|update|get-page|test-auth|generate-tokens`

### Technical

- Rust core library (`docstage-core`) for markdown conversion
- Python CLI package (`docstage`) with aiohttp server
- PyO3 bindings for Rust/Python interop
- Parallel diagram rendering via rayon
