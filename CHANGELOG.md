# Changelog

## [Unreleased]

### 2025-12-08
- Improve ToC sidebar styling
  - Remove left border
  - Position ToC at page H1 level instead of top
  - Article takes full width when ToC is empty, centered when ToC present
- Fix HTML renderer to preserve H1 title and heading levels
  - Title extraction now extracts first H1 without removing it from output
  - Header levels are no longer shifted (H2 stays H2, not H1)
  - ToC excludes page title (first H1) but includes all other headings
  - This differs from Confluence renderer which removes H1 and shifts headers
- Address PR review feedback for bundled assets
  - Fix error message to reference correct build command (`npm run build:bundle`)
  - Add `requires_bundled_assets` skip marker for tests depending on bundled assets

### 2025-12-07
- Bundle frontend assets into Python package (RD-003)
  - Add `docstage.assets` module with `get_static_dir()` for bundled asset discovery
  - Remove `--static-dir` CLI option from `serve` command
  - Server always uses bundled assets from `docstage/static/`
  - Add `npm run build:bundle` script to build and copy frontend to backend
  - Configure hatchling `force-include` to bundle assets in wheel
- Create RD-003: Bundled Frontend Assets requirements document
- Create RD-002: Docstage Frontend requirements document
- Implement Frontend Phase 1: Project Setup
  - Initialize Vite + Svelte 5 project with TypeScript
  - Implement native SPA router using History API (no external library)
  - Configure Tailwind CSS with Typography plugin
  - Create base layout structure (navigation sidebar, content area, ToC sidebar)
  - Set up API client with TypeScript interfaces
  - Add page and navigation Svelte stores
  - Design inspired by Stripe documentation
- Complete Frontend Phase 2: Navigation
  - Add mobile responsive drawer with hamburger menu
  - Auto-close drawer on route change and Escape key
- Complete Frontend Phase 4: Table of Contents
  - Add scroll spy with IntersectionObserver to highlight active heading
- Complete Backend Phase 5: Static File Serving
  - Implement SPA fallback route serving index.html for client-side routing
  - Serve static assets from `/assets` directory
  - Add favicon route at `/favicon.png`
  - API routes take precedence over SPA fallback

### 2025-12-06
- Implement Phase 4: Python Backend - HTTP API
  - Add `docstage.server` module with aiohttp application factory
  - Add `docstage.api.pages` module with `/api/pages/{path}` endpoint
  - Add `docstage.api.navigation` module with `/api/navigation` and `/api/navigation/{path}` endpoints
  - Add `serve` CLI command to start documentation server
  - Add aiohttp and pytest-aiohttp dependencies
  - Implement cache headers (ETag, Last-Modified, Cache-Control) for page responses
  - Add 17 tests for HTTP API endpoints
- Implement Phase 3: Python Backend - Core Library
  - Add `docstage.core.cache` module with file-based caching and mtime invalidation
  - Add `docstage.core.renderer` module wrapping Rust converter with caching
  - Add `docstage.core.navigation` module for building navigation trees from directories
  - Export `HtmlConvertResult` and `TocEntry` from `docstage_core` Python bindings
  - Add 26 tests for core modules (cache, renderer, navigation)
- Pre-Phase 3 code review and improvements
  - Add Python tests for config and CLI (16 tests)
  - Extract `HtmlRenderer` state into dedicated structs (`CodeBlockState`, `TableState`, `ImageState`, `HeadingState`)
  - Collect all Kroki diagram errors via `RenderError::Multiple` instead of failing on first
  - Make DPI configurable via `MarkdownConverter::dpi()` builder method
  - Add comprehensive module-level rustdoc comments
  - Add pytest as dev dependency group
- Implement Phase 2: Rust Core - HTML Renderer
  - Add `HtmlRenderer` module producing semantic HTML5
  - Generate heading IDs for anchor links
  - Create `TocEntry` struct for table of contents
  - Add `MarkdownConverter::convert_html()` method
  - Update Python bindings with `HtmlConvertResult` and `TocEntry` classes
  - Preserve inline formatting in headings (code, emphasis, strong, links)
  - Add table column alignment support via inline styles
  - Code blocks output `language-*` class for client-side highlighting
- Create RD-001: Docstage Backend requirements document
- Define API design for page rendering and navigation endpoints
- Plan implementation phases for HTML renderer, core library, HTTP API, and live reload

### 2025-12-05
- Rename project from md2conf to Docstage ("Where documentation takes the stage")
- Rename packages: md2conf → docstage, md2conf-core → docstage-core
- Update CLI entrypoint: `md2conf` → `docstage`
- Restructure Rust code into Cargo workspace:
  - `crates/docstage-core`: Pure Rust library (no PyO3)
  - `packages/docstage-core`: Python package with PyO3 bindings (maturin)
- Move conversion logic from PyO3 bindings to `docstage-core::MarkdownConverter`

### 2025-12-04
- Merge `convert_with_diagrams` into `convert` method (kroki_url/output_dir now required)
- Move Kroki diagram rendering from Python to Rust with parallel requests (rayon + ureq)

### 2025-12-01
- Comment preservation for inline comments on page updates
- OAuth signature fix: `force_include_body=True` for POST/PUT
- Markdown to Confluence converter via Rust core
- Confluence REST API client with OAuth 1.0 RSA-SHA1
- CLI: `convert`, `create`, `update`, `get-page`, `generate-tokens`, `test-auth`
