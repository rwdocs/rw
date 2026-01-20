# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`std::error::Error` implementation for `DiagramError`** enabling compatibility with `?` operator, `anyhow`, and other error handling crates
- **File-based diagram output abstraction** via `DiagramOutput` enum with `Inline` (default) and `Files` modes; enables customizable tag generation via `DiagramTagGenerator` trait
- **Built-in tag generators**: `ImgTagGenerator` for static sites, `FigureTagGenerator` for figure-wrapped images
- **Diagram caching in Rust** via `FileCache` implementation; Rust owns the cache entirely via `cache_dir` path parameter, eliminating Python-to-Rust callbacks
- **Cached diagram conversion** via `convert_html_with_diagrams_cached(cache_dir)` method that creates `FileCache` internally
- **Post-processing hooks** for `CodeBlockProcessor` trait via `post_process` method; enables processors to replace placeholders after rendering
- **DiagramProcessor configuration** via builder pattern: `include_dirs()`, `config_file()`, `config_content()`, `dpi()`
- **Code block processor trait** (`CodeBlockProcessor`) in `docstage-renderer` for extensible code block handling (diagrams, YAML tables, embeds, etc.)
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

- **Extracted renderer to separate crate** (`docstage-renderer`) for reusability and smaller dependency tree
- **Extracted Confluence renderer to separate crate** (`docstage-confluence-renderer`) for cleaner separation and smaller dependency tree
- **Extracted diagram rendering to separate crate** (`docstage-diagrams`) for reusability, optional dependencies, and plugin architecture
- **Unified HTML and Confluence renderers** via trait-based `RenderBackend` abstraction
- **Config parsing moved to Rust** for better performance and type safety
- **Unified configuration** via `docstage.toml` with auto-discovery
- **Confluence commands** moved to `docstage confluence <command>` subgroup
- **Clean URLs** without `/docs` prefix (e.g., `/guide` instead of `/docs/guide`)
- **Tailwind CSS v4** with CSS-based configuration
- **ty** replaces mypy for faster Python type checking
- Update pulldown-cmark from 0.12 to 0.13
- `ConvertResult` now includes `warnings` field for API consistency
- Removed unimplemented `img` diagram format option (use `svg` or `png`)
- **Removed extract methods** (`extract_html_with_diagrams`, `extract_confluence_with_diagrams`) and related types (`ExtractResult`, `PreparedDiagram`) from public API; use `convert_html_with_diagrams_cached` for HTML rendering with caching
- **Made internal functions private** (`DEFAULT_DPI`, `load_config_file`, `create_image_tag`); DPI configuration uses `Option<u32>` throughout with defaults handled internally
- `MarkdownRenderer::render()` now takes `&mut self` instead of `self` to allow accessing extracted code blocks after rendering
- **Diagram extraction migrated to `DiagramProcessor`** implementing `CodeBlockProcessor` trait (internal refactoring, no API changes)
- **Removed reexports from `docstage-core`** crate; consumers should import directly from `docstage-renderer`, `docstage-diagrams`, and `docstage-confluence-renderer`
- **Moved diagram HTML embedding logic to `docstage-diagrams`** crate (SVG scaling, Google Fonts stripping, placeholder replacement); `docstage-core` no longer depends on `regex`
- **Simplified `convert_html_with_diagrams`** in `docstage-core` to use `DiagramProcessor` configuration
- **Unified diagram rendering path** via Rust `DiagramProcessor.post_process()` with caching; removes ~150 lines of duplicated Python diagram rendering logic
- **PageRenderer uses single Rust call** for diagram rendering with caching instead of extract+render+replace Python logic
- **Removed `diagrams` field** from `ConvertResult`; Python CLI now lists PNG files directly from output directory
- **Unified Confluence diagram rendering** via `DiagramProcessor` with `DiagramOutput::Files` mode; removes ~40 lines of duplicated orchestration code from `converter.rs`
- **Encapsulated DPI scaling in `RenderedDiagramInfo`** via `display_width(dpi)` and `display_height(dpi)` methods; removed `STANDARD_DPI` from public exports
- **Made internal APIs private** in `docstage-diagrams`: `render_all`, `DiagramRequest`, `ExtractedDiagram`, `prepare_diagram_source`, `to_extracted_diagram`, `to_extracted_diagrams`, `RenderError`
- **Simplified `MarkdownConverter::convert()`** to return `ConvertResult` directly instead of `Result<ConvertResult, RenderError>`; errors are now handled internally by replacing placeholders with error messages
- **Removed `MarkdownRenderer::finalize()`** method; `render()` now auto-finalizes by calling `post_process()` on all registered processors
- **Made `DiagramProcessor.cache` non-optional** with `NullCache` default; removes duplicate code paths and simplifies caching logic
- **Encapsulated hash calculation in `DiagramCache`** via `DiagramKey` struct; cache implementations compute hashes internally, removing `compute_diagram_hash` from public API
- **Removed unused `RenderError` variants** (`Http`, `InvalidPng`) in `docstage-diagrams`; individual diagram errors use `DiagramError`/`DiagramErrorKind`, aggregated via `RenderError::Multiple`
- **Consolidated duplicate hash implementations** in `docstage-diagrams`; filename generation in `render_all` now uses `DiagramKey::compute_hash()` (truncated to 12 hex chars) instead of separate `diagram_hash` function, ensuring DPI is included in filename hash to prevent overwrites when same diagram is rendered at different DPIs
- **Separated config from state in `DiagramProcessor`** via internal `ProcessorConfig` struct; enables borrowing config immutably while mutating warnings, eliminating unnecessary clones in `post_process()` (idiomatic Rust pattern)
- **Simplified DPI handling** by defaulting to `DEFAULT_DPI` (192) at construction time instead of using `Option<u32>` throughout; removes `unwrap_or(DEFAULT_DPI)` boilerplate from multiple functions
- **Optimized placeholder replacement** in `DiagramProcessor` via internal `Replacements` struct; collects all replacements and applies them in a single pass instead of O(N × string_length) allocations from repeated `String::replace()` calls
- **Simplified `extract_requests_and_cache_info`** in `DiagramProcessor` using iterator `unzip()` instead of manual loop
- **Made `kroki_url` required when `[diagrams]` section is present** in config; the `[diagrams]` section is optional, but if provided, `kroki_url` must be set (validates at config load time with clear error message)
- **Simplified config resolution** in `docstage-config` by using `DiagramsConfig::default()` and `iter().flatten()` pattern for cleaner optional field handling
- **Made `kroki_url` required in `DiagramProcessor` constructor**; clients decide whether to include the processor based on config; removes `Default` impl and `kroki_url()` builder method

### Fixed

- Kroki error messages now include the actual error response body (e.g., syntax errors)
- Confluence CLI commands (`convert`, `create`, `update`) now use `include_dirs`, `config_file`, and `dpi` from config
- Confluence `create` and `update` commands now upload diagram attachments to the page
- Confluence `create` and `update` commands now prepend table of contents
- Root `index.md` now correctly renders as home page
- Article links resolve correctly without `/docs/` prefix
- Comment preservation works when table content changes
- Live reload triggers page refresh correctly
- Live reload preserves scroll position when content updates
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
