# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`docstage-site` crate** for site structure and page rendering; extracted from `docstage-core` for clearer separation of concerns (site/page management vs Confluence functionality)
- **Internal `PageRenderer` in `docstage-confluence`** for rendering markdown to Confluence XHTML storage format with diagram support; used by `PageUpdater`

- **`RenderResult::warnings` field** in `docstage-renderer` for automatic warnings collection from processors; eliminates manual `renderer.processor_warnings()` calls
- **`MarkdownRenderer::with_gfm(bool)`** builder method to enable/disable GitHub Flavored Markdown features (tables, strikethrough, task lists)
- **`MarkdownRenderer::parser_options()`** method to get configured `pulldown_cmark::Options` based on GFM settings
- **`MarkdownRenderer::create_parser(&str)`** method to create a configured parser for markdown text
- **`MarkdownRenderer::render_markdown(&str)`** convenience method that creates parser internally and renders markdown
- **Native Rust CLI** (`crates/docstage/`) replacing the Python CLI entirely; single binary with no Python runtime dependency
- **`docstage serve` command** in Rust CLI; starts documentation server with live reload, identical options to Python CLI (`--config`, `--source-dir`, `--host`, `--port`, `--kroki-url`, `--verbose`, `--live-reload/--no-live-reload`, `--cache/--no-cache`)
- **`docstage confluence update` command** in Rust CLI; updates Confluence pages from markdown with full feature parity (`--message`, `--kroki-url`, `--extract-title/--no-extract-title`, `--dry-run`, `--key-file`)
- **`docstage confluence generate-tokens` command** in Rust CLI; interactive OAuth 1.0 three-legged flow for generating access tokens (`--private-key`, `--consumer-key`, `--base-url`)
- **Colored terminal output** via `console` crate for success, warning, and error messages in CLI
- **Rust `OAuthTokenGenerator`** in `docstage-confluence` crate for OAuth 1.0 three-legged flow
- **`create_authorization_header_for_token_flow()`** in `oauth/signature.rs` for generating OAuth headers during token generation (supports optional `oauth_token`, `oauth_callback`, and `oauth_verifier` parameters)
- **Rust `PageUpdater`** in `docstage-confluence` crate for updating Confluence pages from markdown; encapsulates entire workflow (convert, fetch, preserve comments, upload attachments, update) in a single Rust call
- **Rust `ConfluenceClient`** in `docstage-confluence` crate; synchronous HTTP client with OAuth 1.0 RSA-SHA1 authentication for Confluence REST API operations (pages, comments, attachments)
- **Rust OAuth 1.0 RSA-SHA1 module** (`oauth/`) in `docstage-confluence` crate; implements signature generation per RFC 5849 with RSA-SHA1 signing
- **RSA key loading** supporting both PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) and PKCS#8 (`-----BEGIN PRIVATE KEY-----`) PEM formats
- **Confluence API types** (`types/`) for `Page`, `Comment`, `Attachment`, and response wrappers
- **Title extraction enabled by default** for `confluence update` command; extracts title from first H1 heading and updates the Confluence page title (use `--no-extract-title` to disable)
- **`--dry-run` flag** for `confluence update` command; previews changes without updating Confluence, showing comments that would be lost
- **Rust comment preservation module** in `docstage-confluence` crate; preserves inline comment markers when updating Confluence pages from markdown using tree-based comparison
- **Optional static asset embedding** via `rust-embed` crate with `embed-assets` feature flag; development builds serve from `frontend/dist`, production builds embed assets for single-binary deployment
- **`build-release` Makefile target** for building production binary with embedded assets
- **Rust `docstage-server` crate** providing native axum HTTP server
- **API handlers** for `/api/config`, `/api/pages/{path}`, and `/api/navigation` endpoints
- **Static file serving** via tower-http with SPA fallback for client-side routing
- **Security middleware** for CSP, X-Content-Type-Options, and X-Frame-Options headers
- **Live reload system** using notify crate for file watching and axum WebSocket for client notifications
- **`ServerConfig`** for configuring the HTTP server (host, port, source_dir, cache_dir, kroki_url, etc.)
- **`server_config_from_docstage_config()`** helper to create `ServerConfig` from `docstage_config::Config`
- **Rust `Site` module** (`Site`, `SiteBuilder`, `SiteLoader`, `Page`, `BreadcrumbItem`) for site structure management; provides O(1) path lookups and O(d) breadcrumb building
- **Rust `SiteCache` trait** with `FileSiteCache` (JSON-based) and `NullSiteCache` implementations for site caching
- **`Site::navigation()` method** for building navigation tree from site structure; returns `Vec<NavItem>` for UI presentation
- **Rust `PageRenderer`** for page rendering with file-based caching
- **`PageRendererConfig`** for configuring the page renderer (cache_dir, version, extract_title, kroki_url, include_dirs, config_file, dpi)
- **`std::error::Error` implementation for `DiagramError`** enabling compatibility with `?` operator, `anyhow`, and other error handling crates
- **`Debug` implementation for `DiagramOutput`** enabling easier debugging of diagram output configuration
- **File-based diagram output abstraction** via `DiagramOutput` enum with `Inline` (default) and `Files` modes; enables customizable tag generation via `DiagramTagGenerator` trait
- **Built-in tag generators**: `ImgTagGenerator` for static sites, `FigureTagGenerator` for figure-wrapped images
- **Diagram caching in Rust** via `FileCache` implementation; Rust owns the cache entirely via `cache_dir` path parameter, eliminating Python-to-Rust callbacks
- **Cached diagram conversion** via `convert_html_with_diagrams_cached(cache_dir)` method that creates `FileCache` internally
- **Post-processing hooks** for `CodeBlockProcessor` trait via `post_process` method; enables processors to replace placeholders after rendering
- **DiagramProcessor configuration** via builder pattern: `include_dirs()`, `config_file()`, `config_content()`, `dpi()`, `timeout()`
- **Configurable HTTP timeout** for Kroki requests via `DiagramProcessor::timeout()`; default is 30 seconds
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

- **Merged `docstage-core` into `docstage-confluence`**; all Confluence-related functionality is now in a single crate
- **Renamed `MarkdownConverter` to `PageRenderer`** in `docstage-confluence`; `convert()` method renamed to `render()`
- **Import path changes** for Confluence types: `docstage_core::{MarkdownConverter, updater::*}` → `docstage_confluence::{PageRenderer, updater::*}`
- **Simplified `docstage-confluence` public API**; all types now exported at crate root (`PageUpdater`, `UpdateConfig`, `UpdateResult`, `DryRunResult`, `UpdateError`, `OAuthTokenGenerator`, `AccessToken`, `RequestToken`); `updater` and `oauth` submodules are now private
- **Tightened `docstage-confluence` internal visibility**; internal types (`ConfluenceBackend`, `PageRenderer`, `ConfluenceTagGenerator`, `OAuth1Auth`, `PreserveResult`, `CommentPreservationError`) and client methods (`get_page`, `update_page`, `get_page_url`, `get_comments`, `upload_attachment`, `get_attachments`) changed from `pub` to `pub(crate)`; only truly external API remains public
- **Extracted site-related code from `docstage-core` to `docstage-site`**; `docstage-server` now depends on `docstage-site` instead of `docstage-core`; cleaner separation between site management (HTML rendering) and Confluence functionality
- **Moved `build_navigation()` to `Site::navigation()` method**; call `site.navigation()` instead of `build_navigation(&site)`
- **`PageRenderer` now uses `MarkdownRenderer` directly** instead of going through `MarkdownConverter`; eliminates unnecessary abstraction layer (`PageRenderer` → `MarkdownConverter` → `MarkdownRenderer` reduced to `PageRenderer` → `MarkdownRenderer`)
- **`MarkdownConverter` now uses renderer's GFM config** via `MarkdownRenderer::with_gfm()` instead of internal `get_parser_options()` method; removes duplicated parser configuration
- **`MarkdownConverter` methods now use `RenderResult`** directly from renderer instead of creating separate result types; `convert()`, `convert_html()`, `convert_html_with_diagrams()`, and `convert_html_with_diagrams_cached()` all return `RenderResult`
- **`MarkdownRenderer::render()` now includes warnings** in the returned `RenderResult`; consumers no longer need to manually call `renderer.processor_warnings()`
- **Removed `MarkdownConverter::get_parser_options()`** method; parser configuration moved to `MarkdownRenderer`
- **Merged `convert_html_with_diagrams_cached` into `convert_html_with_diagrams`**; the method now takes an optional `cache_dir` parameter
- **`MarkdownConverter::convert()` now takes optional `kroki_url` and `output_dir`**; when `None`, diagram blocks are rendered as syntax-highlighted code instead of images (consistent with `convert_html()` behavior)
- **CLI replaced with native Rust binary**; no longer requires Python runtime or `uv run` command; run directly via `./target/debug/docstage` or `cargo install --path crates/docstage`
- **Removed Python CLI package** (`packages/docstage/`); all CLI functionality is now in `crates/docstage/`
- **Removed PyO3 bindings package** (`packages/docstage-core/`); no longer needed since CLI is pure Rust
- **Removed Python tooling**: `pyproject.toml`, `uv.lock`, `.python-version`, Python dependencies (Click, aiohttp, watchfiles, authlib, httpx, cryptography)
- **Updated CI workflow**: removed Python setup steps (uv, Python 3.14, dependencies), removed Python linting (ruff, ty), removed Python tests (pytest); added `cargo build -p docstage` before E2E tests
- **Updated Playwright config**: E2E tests now use `./target/debug/docstage serve` instead of `uv run docstage serve`
- **Updated Makefile**: removed Python-related targets (`uv sync`, `uv run pytest`, `uv run ruff`, `uv run ty`); simplified to Rust and frontend only
- **`CodeBlockProcessor` trait methods return slices** (`extracted()` returns `&[ExtractedCodeBlock]` and `warnings()` returns `&[String]` instead of `Vec`); implementations no longer need to clone, improving performance
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
- **Simplified parallel rendering** in `docstage-diagrams` by using rayon's global thread pool instead of per-call thread pool creation; all render functions now return `PartialRenderResult` for consistent partial-success handling; removed `RenderError` type; extracted `create_agent()` and `partition_results()` helpers
- **HTTP agent reuse for connection pooling** in `docstage-diagrams`; the `ureq::Agent` is now stored in `ProcessorConfig` and reused across render calls instead of creating a new agent per call, enabling HTTP connection pooling for improved performance
- **Optimized cache lookup in `post_process_inline`** by consuming the prepared diagrams iterator and constructing `DiagramKey` directly for cache hits; eliminates unnecessary `CacheInfo` allocation and string clone for cached diagrams
- **Added capacity hint for `Replacements` HashMap** in `DiagramProcessor`; pre-allocates based on diagram count to reduce rehashing
- **Reduced `docstage-confluence` public API**; internal types now private: `ConfluenceBackend`, `PageRenderer`, `TreeNode`, `PreserveResult`, `preserve_comments()`, `OAuth1Auth`, `types::*` re-exports; consumers use `ConfluenceClient::from_config()` and `PageUpdater` instead of internal implementation details
- **`ConfluenceClient::from_config()` and `OAuthTokenGenerator::new()` now take key file path** instead of key bytes; file I/O moved from CLI to library; removes `oauth::read_private_key` from public API

### Removed

- **`docstage-core` crate**; merged into `docstage-confluence`; all Confluence integration functionality is now in a single crate
- **`MarkdownConverter` type**; replaced by `PageRenderer` in `docstage-confluence` with `convert()` renamed to `render()`
- **`ConvertResult` type alias**; use `RenderResult` from `docstage-renderer` (re-exported by `docstage-confluence`) instead
- **Python CLI package** (`packages/docstage/`); replaced by native Rust CLI in `crates/docstage/`
- **PyO3 bindings package** (`packages/docstage-core/`); no longer needed
- **Python tooling**: `pyproject.toml`, `uv.lock`, `.python-version`
- **`confluence upload-mkdocs` command**; use `confluence update` instead with appropriate `include_dirs` and `config_file` in `docstage.toml`
- **`confluence comments` command**; comment information is available in the Confluence UI
- **`confluence convert` command**; use `confluence update --dry-run` to preview conversion
- **`confluence create` command**; create pages manually in Confluence, then use `confluence update` to sync content
- **`confluence get-page` command**; use the Confluence REST API directly or the web UI
- **`confluence test-auth` command**; use `confluence update --dry-run` to verify authentication
- **`confluence test-create` command**; use `confluence update --dry-run` to verify permissions
- **Unused `ConfluenceClient` methods** (`create_page`, `base_url`, `get_inline_comments`, `get_footer_comments`); removed to reduce API surface and eliminate dead code
- **Unused Confluence API types** (`Comment`, `Extensions`, `InlineProperties`, `Resolution`); `CommentsResponse` simplified to only include `size` field; `Attachment` simplified to only `id` and `title`; serde ignores unknown fields by default so unused API response fields are skipped

### Fixed

- **OAuth 1.0 signature now includes query parameters** per RFC 5849 Section 3.4.1.3; fixes `signature_invalid` errors for Confluence API requests with query strings (e.g., `get_page` with `expand` parameter)
- **Ctrl-C signal handling** now works for `docstage serve`; uses tokio graceful shutdown instead of relying on Python signal handlers
- Kroki error messages now include the actual error response body (e.g., syntax errors)
- Confluence `update` command now uses `include_dirs`, `config_file`, and `dpi` from config
- Confluence `update` command now uploads diagram attachments to the page
- Confluence `update` command now prepends table of contents
- Root `index.md` now correctly renders as home page
- Article links resolve correctly without `/docs/` prefix
- Comment preservation works when table content changes
- Live reload triggers page refresh correctly
- Live reload preserves scroll position when content updates
- Navigation updates on consecutive clicks
- Breadcrumbs exclude non-navigable paths and current page
- Diagram sizing displays at correct physical size
- Indented `!include` directives resolve in PlantUML
- Cache mtime validation on CI due to f64 precision loss in JSON serialization

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
