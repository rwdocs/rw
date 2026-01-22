# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Rust `OAuthTokenGenerator`** in `docstage-confluence` crate for OAuth 1.0 three-legged flow; replaces `authlib` Python dependency with native Rust implementation
- **PyO3 bindings for OAuth token generation** via `OAuthTokenGenerator`, `RequestToken`, and `AccessToken` classes; enables synchronous token generation from Python CLI
- **`create_authorization_header_for_token_flow()`** in `oauth/signature.rs` for generating OAuth headers during token generation (supports optional `oauth_token`, `oauth_callback`, and `oauth_verifier` parameters)
- **Rust `PageUpdater`** in `docstage-core` crate for updating Confluence pages from markdown; encapsulates entire workflow (convert, fetch, preserve comments, upload attachments, update) in a single Rust call
- **PyO3 bindings for page updater** via `update_page_from_markdown()` and `dry_run_update()` methods on `ConfluenceClient`; `UpdateResult` and `DryRunResult` classes for result types
- **Rust `ConfluenceClient`** in `docstage-confluence` crate; synchronous HTTP client with OAuth 1.0 RSA-SHA1 authentication for Confluence REST API operations (pages, comments, attachments)
- **Rust OAuth 1.0 RSA-SHA1 module** (`oauth/`) in `docstage-confluence` crate; implements signature generation per RFC 5849 with RSA-SHA1 signing
- **RSA key loading** supporting both PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) and PKCS#8 (`-----BEGIN PRIVATE KEY-----`) PEM formats
- **Confluence API types** (`types/`) for `Page`, `Comment`, `Attachment`, and response wrappers
- **PyO3 bindings for Confluence client** via `ConfluenceClient`, `ConfluencePage`, `ConfluenceComment`, `ConfluenceCommentsResponse`, and `read_private_key()` function
- **Title extraction enabled by default** for `confluence update` command; extracts title from first H1 heading and updates the Confluence page title (use `--no-extract-title` to disable)
- **`--dry-run` flag** for `confluence update` command; previews changes without updating Confluence, showing comments that would be lost
- **Rust comment preservation module** in `docstage-confluence` crate; preserves inline comment markers when updating Confluence pages from markdown using tree-based comparison
- **PyO3 bindings for comment preservation** via `preserve_comments()` function, `PreserveResult`, and `UnmatchedComment` classes
- **Optional static asset embedding** via `rust-embed` crate with `embed-assets` feature flag; development builds serve from `frontend/dist`, production builds embed assets for single-binary deployment
- **`build-release` Makefile target** for building production binary with embedded assets
- **Rust `docstage-server` crate** providing native axum HTTP server; replaces Python aiohttp server with direct Rust calls, eliminating FFI overhead
- **API handlers** for `/api/config`, `/api/pages/{path}`, and `/api/navigation` endpoints
- **Static file serving** via tower-http with SPA fallback for client-side routing
- **Security middleware** for CSP, X-Content-Type-Options, and X-Frame-Options headers
- **Live reload system** using notify crate for file watching and axum WebSocket for client notifications
- **`ServerConfig`** for configuring the HTTP server (host, port, source_dir, cache_dir, kroki_url, etc.)
- **`server_config_from_docstage_config()`** helper to create `ServerConfig` from `docstage_config::Config`
- **PyO3 bindings for HTTP server** via `HttpServerConfig` class and `run_http_server()` function; Python CLI now delegates to native Rust server
- **Rust `Site` module** (`Site`, `SiteBuilder`, `SiteLoader`, `Page`, `BreadcrumbItem`) for site structure management; provides O(1) path lookups and O(d) breadcrumb building
- **Rust `SiteCache` trait** with `FileSiteCache` (JSON-based) and `NullSiteCache` implementations for site caching
- **Rust `NavItem` and `build_navigation()`** for navigation tree construction from site structure
- **PyO3 bindings** for `Site`, `SiteLoader`, `SiteLoaderConfig`, `Page`, `BreadcrumbItem`, `NavItem`, and `build_navigation()` function
- **Rust `PageRenderer`** for page rendering with file-based caching; replaces Python `PageRenderer`, unifying page rendering with the existing markdown conversion pipeline
- **`PageRendererConfig`** for configuring the page renderer (cache_dir, version, extract_title, kroki_url, include_dirs, config_file, dpi)
- **PyO3 bindings** for `PageRenderer`, `PageRendererConfig`, and `PageRenderResult` classes
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

- **Moved OAuth token generation from Python to Rust**; `confluence generate-tokens` command now uses synchronous Rust `OAuthTokenGenerator` via PyO3 bindings instead of async Python `authlib` client
- **Simplified `confluence generate-tokens` command**; removed `asyncio.run()` wrapper and `--port` option (unused with oob callback); three-step flow now uses Rust implementation
- **Removed `authlib`, `httpx`, and `cryptography` dependencies** from Python package; OAuth token generation is now handled entirely in Rust
- **Moved Confluence update workflow from Python to Rust**; `confluence update` command now uses single `update_page_from_markdown()` or `dry_run_update()` call instead of multiple PyO3 boundary crossings
- **Simplified Python CLI `confluence update` command**; removed tempfile management, converter creation, and attachment collection from Python; all handled in Rust
- **Moved Confluence client from Python to Rust**; all Confluence CLI commands now use synchronous Rust `ConfluenceClient` via PyO3 bindings instead of async Python `ConfluenceClient` with httpx
- **Removed Python `ConfluenceClient` class** (`docstage.confluence.client`); use `docstage_core.ConfluenceClient` instead
- **Removed Python `oauth` module** (`docstage.oauth`); OAuth 1.0 RSA-SHA1 authentication is now handled entirely in Rust; use `docstage_core.read_private_key()` for key loading
- **Confluence CLI commands are now synchronous**; removed `asyncio.run()` wrappers from `test_auth`, `get_page`, `test_create`, `create`, `update`, and `comments` commands
- **Extracted CLI helper functions** for diagram attachment handling and dry-run output; `_collect_diagram_attachments()`, `_upload_attachments()`, `_print_dry_run_summary()`, and `_print_unmatched_comments_warning()` reduce code duplication across `_create`, `_update`, and `_upload_mkdocs` commands
- **Moved comment preservation from Python to Rust**; Confluence `update` command now uses Rust `preserve_comments()` via PyO3 bindings instead of Python `CommentPreserver` class
- **Removed Python `CommentPreserver` class** (`docstage.confluence.comment_preservation`); use `docstage_core.preserve_comments()` instead
- **Removed Python `docstage.confluence` module**; import `MarkdownConverter` directly from `docstage_core` instead
- **Moved `Site` module from Python to Rust**; Python server now uses Rust `SiteLoader` via PyO3 bindings
- **Removed Python `Site`, `SiteBuilder`, `SiteLoader`** (`docstage.core.site`); use `docstage_core.Site`, `docstage_core.SiteLoader` instead
- **Removed Python `NavItem`, `build_navigation`** (`docstage.core.navigation`); use `docstage_core.NavItem`, `docstage_core.build_navigation` instead
- **Removed Python `URLPath` type alias** (`docstage.core.types`); paths are now plain `str`
- **Removed site cache methods from `FileCache`** (`get_site`, `set_site`, `invalidate_site`); site caching is now handled entirely in Rust
- **Removed Python `docstage.core.cache` module** (`PageCache`, `FileCache`, `NullCache`, `CacheEntry`); page caching is now handled entirely in Rust via `PageRenderer`
- **Moved `PageRenderer` from Python to Rust**; Python server now uses Rust `PageRenderer` via PyO3 bindings, reducing Python orchestration to a thin aiohttp layer
- **Removed Python `PageRenderer` class** (`docstage.core.renderer.PageRenderer`); use `docstage_core.PageRenderer` instead
- **Server initialization uses `PageRendererConfig`** instead of passing cache and keyword arguments; Rust `PageRenderer` manages its own page cache internally
- **Replaced Python aiohttp server with native Rust server** via PyO3; `docstage serve` now delegates to `run_http_server()` instead of `aiohttp.web.run_app()`
- **Removed Python server modules** (`docstage.server.create_app`, `docstage.api`, `docstage.live`, `docstage.app_keys`); all HTTP handling now in Rust `docstage-server` crate
- **Removed `static_dir` from server configuration**; Rust server now handles static files internally via `frontend/dist` (dev) or embedded assets (prod)
- **Removed Python `docstage.assets` module** (`get_static_dir()`); static asset discovery is no longer needed as Rust handles static files internally
- **Removed bundled static assets from Python package** (`packages/docstage/src/docstage/static/`); assets are now served from `frontend/dist` during development
- **Removed `npm run build:bundle` script** from frontend; use `npm run build` instead
- **Removed `aiohttp` and `watchfiles` dependencies** from Python package; no longer needed as Rust handles HTTP serving and file watching
- **Removed `pytest-aiohttp` dev dependency**; no longer needed as HTTP server is now in Rust
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

### Removed

- **Unused PyO3 exports from Python interface**; removed `Site`, `SiteLoader`, `SiteLoaderConfig`, `Page`, `BreadcrumbItem`, `NavItem`, `build_navigation`, `PageRenderer`, `PageRendererConfig`, `PageRenderResult`, `TocEntry`, `MarkdownConverter`, `ConvertResult`, `HtmlConvertResult`, `ConfluenceAttachment`, `ConfluenceAttachmentsResponse`, `ConfluenceComment`, `ConfluenceCommentsResponse`, `preserve_comments`, `PreserveResult`, `ConfluenceTestConfig` from `docstage_core` Python package; these are now handled entirely in Rust
- **`confluence upload-mkdocs` command**; use `confluence update` instead with appropriate `include_dirs` and `config_file` in `docstage.toml`
- **`confluence comments` command**; comment information is available in the Confluence UI
- **`confluence convert` command**; use `confluence update --dry-run` to preview conversion
- **`confluence create` command**; create pages manually in Confluence, then use `confluence update` to sync content
- **`confluence get-page` command**; use the Confluence REST API directly or the web UI
- **`confluence test-auth` command**; use `confluence update --dry-run` to verify authentication
- **`confluence test-create` command**; use `confluence update --dry-run` to verify permissions

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
