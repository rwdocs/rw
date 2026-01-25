# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Project renamed from Docstage to RW**; all crates renamed from `docstage-*` to `rw-*`; configuration file renamed from `docstage.toml` to `rw.toml`; CLI binary renamed from `docstage` to `rw`; default OAuth consumer key changed from `"docstage"` to `"rw"`

### Added

- **Criterion benchmarks for `rw-site` crate**; measures page rendering performance (simple, ToC extraction, GFM features, code blocks, varying sizes, cache hit/miss) and site structure operations (page lookup, breadcrumbs, navigation tree building, SiteLoader reload, path resolution); run with `make bench`

- **Tabbed content blocks** using CommonMark directive syntax (`::: tab Label` / `:::`); renders as accessible HTML with ARIA attributes (`role="tablist"`, `role="tab"`, `role="tabpanel"`); interactive tab switching via click and keyboard navigation (Arrow keys, Home, End); first tab selected by default; supports markdown content inside tabs including code blocks, diagrams, and alerts; Material for MkDocs-inspired styling with clean underline indicator and code blocks pinned directly to tabs; focus indicator only shows on keyboard navigation (`:focus-visible`)
- **`TabsPreprocessor`** in `rw-renderer` for converting directive syntax to intermediate `<rw-tabs>` / `<rw-tab>` elements; 2-state machine (Normal, InTab) handles code fence skipping and warning generation
- **`TabsProcessor`** in `rw-renderer` for post-processing intermediate `<rw-tabs>` elements to accessible HTML; uses explicit `post_process(&mut String)` method instead of `CodeBlockProcessor` trait (tabs are container directives, not code blocks)
- **`TabsGroup`** and **`TabMetadata`** types for tab group metadata; exported at crate root

- **GitHub-style alerts** (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`); renders as styled alert boxes in HTML with SVG icons (Octicons-style) and colored left borders; clean GitHub-inspired styling without rounded corners; Confluence backend maps to `info`, `tip`, `note`, and `warning` macros respectively
- **`AlertKind` enum** in `rw-renderer` for alert type classification; exported at crate root; `alert_start` and `alert_end` methods added to `RenderBackend` trait
- **Configuration validation on load** via `Config::validate()` method; validates all config fields at load time instead of during use; checks: `server.host` non-empty, `server.port` non-zero, `diagrams.kroki_url` valid HTTP(S) URL when set, `diagrams.dpi` positive and ≤1000, `confluence.*` fields non-empty and `base_url` valid HTTP(S) URL when section exists

### Changed

- **Updated `quick-xml` dependency to 0.39**; migrated from deprecated `BytesText::unescape()` API to `reader.decoder().decode()` with explicit `Event::GeneralRef` handling for XML entity references; added `CommentPreservationError::Encoding` variant for encoding errors
- **`MarkdownRenderer::extracted_code_blocks()` returns `impl Iterator`** instead of `Vec<ExtractedCodeBlock>`; callers who need a `Vec` can call `.collect()` on the result; enables lazy iteration without allocation for callers who only iterate once
- **`MarkdownRenderer::processor_warnings()` returns `impl Iterator`** instead of `Vec<String>`; same lazy evaluation benefits as `extracted_code_blocks()`
- **Preserved error sources in error types** via `#[source]` and `#[from]` attributes for proper error chain debugging; `ConfluenceError` split `Http` variant into `HttpRequest` (wraps `ureq::Error`) and `HttpResponse` (status + body), `Json` now uses `#[from] serde_json::Error`, `CommentPreservation` uses `#[from] CommentPreservationError`, `RsaKey` uses `#[from] RsaKeyError`; `DiagramErrorKind` now has `HttpRequest` (wraps `ureq::Error`), `HttpResponse` (status + body), `Io` (wraps `std::io::Error`), `InvalidUtf8` (wraps `FromUtf8Error`) variants instead of `Http(String)` and `Io(String)`

### Added

- **`RsaKeyError` enum** in `rw-confluence` for structured RSA key errors with `InvalidUtf8`, `Pkcs1`, `Pkcs8` variants using `#[from]` for automatic conversion; enables error chain debugging for key loading failures
- **`CommentPreservationError` made public** with `#[non_exhaustive]` attribute for forward compatibility; enables inspecting XML parsing errors in error chains
- **`ConfluenceError::HttpRequest` variant** for network-level errors (timeouts, DNS, TLS) with preserved `ureq::Error` source
- **`ConfluenceError::HttpResponse` variant** for HTTP-level errors (4xx, 5xx) with status code and response body
- **`DiagramRequest::error()` helper** in `rw-diagrams` for creating `DiagramError` instances with diagram index context

- **Environment variable expansion in configuration** via `shellexpand` crate; supports `${VAR}` and `${VAR:-default}` syntax in string config values; expanded fields: `server.host`, `confluence.base_url`, `confluence.access_token`, `confluence.access_secret`, `confluence.consumer_key`, `diagrams.kroki_url`
- **`ConfigError::EnvVar` variant** for environment variable expansion errors; includes field path and descriptive error message

- **`rw-site` crate** for site structure and page rendering; extracted from `rw-core` for clearer separation of concerns (site/page management vs Confluence functionality)
- **Internal `PageRenderer` in `rw-confluence`** for rendering markdown to Confluence XHTML storage format with diagram support; used by `PageUpdater`

- **`RenderResult::warnings` field** in `rw-renderer` for automatic warnings collection from processors; eliminates manual `renderer.processor_warnings()` calls
- **`MarkdownRenderer::with_gfm(bool)`** builder method to enable/disable GitHub Flavored Markdown features (tables, strikethrough, task lists)
- **`MarkdownRenderer::parser_options()`** method to get configured `pulldown_cmark::Options` based on GFM settings
- **`MarkdownRenderer::create_parser(&str)`** method to create a configured parser for markdown text
- **`MarkdownRenderer::render_markdown(&str)`** convenience method that creates parser internally and renders markdown
- **Native Rust CLI** (`crates/rw/`) replacing the Python CLI entirely; single binary with no Python runtime dependency
- **`rw serve` command** in Rust CLI; starts documentation server with live reload, identical options to Python CLI (`--config`, `--source-dir`, `--host`, `--port`, `--kroki-url`, `--verbose`, `--live-reload/--no-live-reload`, `--cache/--no-cache`)
- **`rw confluence update` command** in Rust CLI; updates Confluence pages from markdown with full feature parity (`--message`, `--kroki-url`, `--extract-title/--no-extract-title`, `--dry-run`, `--key-file`)
- **`rw confluence generate-tokens` command** in Rust CLI; interactive OAuth 1.0 three-legged flow for generating access tokens (`--private-key`, `--consumer-key`, `--base-url`)
- **Colored terminal output** via `console` crate for success, warning, and error messages in CLI
- **Rust `OAuthTokenGenerator`** in `rw-confluence` crate for OAuth 1.0 three-legged flow
- **`create_authorization_header_for_token_flow()`** in `oauth/signature.rs` for generating OAuth headers during token generation (supports optional `oauth_token`, `oauth_callback`, and `oauth_verifier` parameters)
- **Rust `PageUpdater`** in `rw-confluence` crate for updating Confluence pages from markdown; encapsulates entire workflow (convert, fetch, preserve comments, upload attachments, update) in a single Rust call
- **Rust `ConfluenceClient`** in `rw-confluence` crate; synchronous HTTP client with OAuth 1.0 RSA-SHA1 authentication for Confluence REST API operations (pages, comments, attachments)
- **Rust OAuth 1.0 RSA-SHA1 module** (`oauth/`) in `rw-confluence` crate; implements signature generation per RFC 5849 with RSA-SHA1 signing
- **RSA key loading** supporting both PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) and PKCS#8 (`-----BEGIN PRIVATE KEY-----`) PEM formats
- **Confluence API types** (`types/`) for `Page`, `Comment`, `Attachment`, and response wrappers
- **Title extraction enabled by default** for `confluence update` command; extracts title from first H1 heading and updates the Confluence page title (use `--no-extract-title` to disable)
- **`--dry-run` flag** for `confluence update` command; previews changes without updating Confluence, showing comments that would be lost
- **Rust comment preservation module** in `rw-confluence` crate; preserves inline comment markers when updating Confluence pages from markdown using tree-based comparison
- **Optional static asset embedding** via `rust-embed` crate with `embed-assets` feature flag; development builds serve from `frontend/dist`, production builds embed assets for single-binary deployment
- **`build-release` Makefile target** for building production binary with embedded assets
- **Rust `rw-server` crate** providing native axum HTTP server
- **API handlers** for `/api/config`, `/api/pages/{path}`, and `/api/navigation` endpoints
- **Static file serving** via tower-http with SPA fallback for client-side routing
- **Security middleware** for CSP, X-Content-Type-Options, and X-Frame-Options headers
- **Live reload system** using notify crate for file watching and axum WebSocket for client notifications
- **`ServerConfig`** for configuring the HTTP server (host, port, source_dir, cache_dir, kroki_url, etc.)
- **`server_config_from_rw_config()`** helper to create `ServerConfig` from `rw_config::Config`
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
- **Code block processor trait** (`CodeBlockProcessor`) in `rw-renderer` for extensible code block handling (diagrams, YAML tables, embeds, etc.)
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

- **Structured logging** across all crates; converted `tracing` log calls from string interpolation (e.g., `tracing::info!("Found {} items", count)`) to structured fields (e.g., `tracing::info!(count, "Found items")`); enables better log analysis and filtering
- **Reduced lock contention in `SiteLoader`** (RD-027); replaced external `Arc<RwLock<SiteLoader>>` pattern with internal thread-safe design using `RwLock<Arc<Site>>` for site snapshot, `Mutex<()>` for serializing reloads, and `AtomicBool` for lock-free cache validity; new API: `get()` returns `Arc<Site>` with minimal locking, `reload_if_needed()` uses double-checked locking pattern, `invalidate()` is lock-free; removed `load(&mut self, use_cache: bool) -> &Site` method
- **Standardized error handling on `thiserror`** across all crates; `ConfigError` (`rw-config`), `RenderError` (`rw-site`), `DiagramError` and `DiagramErrorKind` (`rw-diagrams`) now use `thiserror` derive macros instead of manual `Display` and `Error` trait implementations; reduces boilerplate and ensures consistent error chain preservation
- **Standardized error message format** across crates; "IO error" changed to "I/O error" in `ConfigError` (`rw-config`) and `DiagramErrorKind` (`rw-diagrams`) for consistency with `RenderError` (`rw-site`)
- **Tightened `rw-site` internal visibility**; `SiteBuilder`, `SiteCache` trait, `FileSiteCache`, `NullSiteCache` changed from `pub` to `pub(crate)`; `Site` methods (`new()`, `get_page()`, `get_children()`, `get_root_pages()`, `pages()`, `children_indices()`, `parent_indices()`, `root_indices()`) changed from `pub` to `pub(crate)`; `site` and `site_loader` modules changed from `pub mod` to `pub(crate) mod`; only types used by other crates remain public (`PageRenderer`, `PageRendererConfig`, `PageRenderResult`, `RenderError`, `SiteLoader`, `SiteLoaderConfig`, `Site`, `Page`, `BreadcrumbItem`, `NavItem`)
- **Tightened `rw-config` internal visibility**; `CONFIG_FILENAME` const, `Config::discover_config()`, `Config::default_with_cwd()`, `Config::default_with_base()` methods changed from `pub` to private; removed unused `CliSettings::is_empty()` method, `ConfluenceTestConfig` struct, `ConfluenceConfig::test` field, and `Config::confluence_test()` method
- **Tightened `rw` CLI internal visibility**; all types and functions changed from `pub` to `pub(crate)` since the crate is a binary with no external consumers; removed unused `Default` impl for `Output`
- **Merged `rw-core` into `rw-confluence`**; all Confluence-related functionality is now in a single crate
- **Renamed `MarkdownConverter` to `PageRenderer`** in `rw-confluence`; `convert()` method renamed to `render()`
- **Import path changes** for Confluence types: `rw_core::{MarkdownConverter, updater::*}` → `rw_confluence::{PageRenderer, updater::*}`
- **Simplified `rw-confluence` public API**; all types now exported at crate root (`PageUpdater`, `UpdateConfig`, `UpdateResult`, `DryRunResult`, `UpdateError`, `OAuthTokenGenerator`, `AccessToken`, `RequestToken`); `updater` and `oauth` submodules are now private
- **Tightened `rw-confluence` internal visibility**; internal types (`ConfluenceBackend`, `PageRenderer`, `ConfluenceTagGenerator`, `OAuth1Auth`, `PreserveResult`, `CommentPreservationError`) and client methods (`get_page`, `update_page`, `get_page_url`, `get_comments`, `upload_attachment`, `get_attachments`) changed from `pub` to `pub(crate)`; only truly external API remains public
- **Tightened `rw-diagrams` internal visibility**; `DiagramKey`, `NullCache`, `ImgTagGenerator`, `FigureTagGenerator` changed from `pub` to `pub(crate)`; only types used by other crates remain public (`DiagramProcessor`, `DiagramCache`, `FileCache`, `DiagramOutput`, `DiagramTagGenerator`, `RenderedDiagramInfo`)
- **Tightened `rw-renderer` internal visibility**; `parse_fence_info` function changed from `pub` to `pub(crate)`, `slugify` function changed from `pub` to private, `heading_level_to_num` function changed from `pub` to `pub(crate)`, internal state structs (`CodeBlockState`, `TableState`, `ImageState`, `HeadingState`) changed from `pub` to `pub(crate)`; only types used by other crates remain public (`RenderBackend`, `CodeBlockProcessor`, `ExtractedCodeBlock`, `ProcessResult`, `HtmlBackend`, `MarkdownRenderer`, `RenderResult`, `TocEntry`, `escape_html`)
- **Tightened `rw-server` internal visibility**; `ServerError`, `AppState`, handlers (`get_config`, `get_page`, `get_root_page`, `get_navigation`), live reload types (`LiveReloadManager`, `ReloadEvent`, `ws_handler`), middleware functions (`csp_layer`, `content_type_options_layer`, `frame_options_layer`), static file functions (`static_router`, `spa_fallback`), and `create_router` changed from `pub` to `pub(crate)`; removed unused `ServerError::FileNotFound` variant; only types used by CLI remain public (`ServerConfig`, `run_server`, `server_config_from_rw_config`)
- **Extracted site-related code from `rw-core` to `rw-site`**; `rw-server` now depends on `rw-site` instead of `rw-core`; cleaner separation between site management (HTML rendering) and Confluence functionality
- **Moved `build_navigation()` to `Site::navigation()` method**; call `site.navigation()` instead of `build_navigation(&site)`
- **`PageRenderer` now uses `MarkdownRenderer` directly** instead of going through `MarkdownConverter`; eliminates unnecessary abstraction layer (`PageRenderer` → `MarkdownConverter` → `MarkdownRenderer` reduced to `PageRenderer` → `MarkdownRenderer`)
- **`MarkdownConverter` now uses renderer's GFM config** via `MarkdownRenderer::with_gfm()` instead of internal `get_parser_options()` method; removes duplicated parser configuration
- **`MarkdownConverter` methods now use `RenderResult`** directly from renderer instead of creating separate result types; `convert()`, `convert_html()`, `convert_html_with_diagrams()`, and `convert_html_with_diagrams_cached()` all return `RenderResult`
- **`MarkdownRenderer::render()` now includes warnings** in the returned `RenderResult`; consumers no longer need to manually call `renderer.processor_warnings()`
- **Removed `MarkdownConverter::get_parser_options()`** method; parser configuration moved to `MarkdownRenderer`
- **Merged `convert_html_with_diagrams_cached` into `convert_html_with_diagrams`**; the method now takes an optional `cache_dir` parameter
- **`MarkdownConverter::convert()` now takes optional `kroki_url` and `output_dir`**; when `None`, diagram blocks are rendered as syntax-highlighted code instead of images (consistent with `convert_html()` behavior)
- **CLI replaced with native Rust binary**; no longer requires Python runtime or `uv run` command; run directly via `./target/debug/rw` or `cargo install --path crates/rw`
- **Removed Python CLI package** (`packages/rw/`); all CLI functionality is now in `crates/rw/`
- **Removed PyO3 bindings package** (`packages/rw-core/`); no longer needed since CLI is pure Rust
- **Removed Python tooling**: `pyproject.toml`, `uv.lock`, `.python-version`, Python dependencies (Click, aiohttp, watchfiles, authlib, httpx, cryptography)
- **Updated CI workflow**: removed Python setup steps (uv, Python 3.14, dependencies), removed Python linting (ruff, ty), removed Python tests (pytest); added `cargo build -p rw` before E2E tests
- **Updated Playwright config**: E2E tests now use `./target/debug/rw serve` instead of `uv run docstage serve`
- **Updated Makefile**: removed Python-related targets (`uv sync`, `uv run pytest`, `uv run ruff`, `uv run ty`); simplified to Rust and frontend only
- **`CodeBlockProcessor` trait methods return slices** (`extracted()` returns `&[ExtractedCodeBlock]` and `warnings()` returns `&[String]` instead of `Vec`); implementations no longer need to clone, improving performance
- **Extracted renderer to separate crate** (`rw-renderer`) for reusability and smaller dependency tree
- **Extracted Confluence renderer to separate crate** (`rw-confluence-renderer`) for cleaner separation and smaller dependency tree
- **Extracted diagram rendering to separate crate** (`rw-diagrams`) for reusability, optional dependencies, and plugin architecture
- **Unified HTML and Confluence renderers** via trait-based `RenderBackend` abstraction
- **Config parsing moved to Rust** for better performance and type safety
- **Unified configuration** via `rw.toml` with auto-discovery
- **Confluence commands** moved to `rw confluence <command>` subgroup
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
- **Removed reexports from `rw-core`** crate; consumers should import directly from `rw-renderer`, `rw-diagrams`, and `rw-confluence-renderer`
- **Moved diagram HTML embedding logic to `rw-diagrams`** crate (SVG scaling, Google Fonts stripping, placeholder replacement); `rw-core` no longer depends on `regex`
- **Simplified `convert_html_with_diagrams`** in `rw-core` to use `DiagramProcessor` configuration
- **Unified diagram rendering path** via Rust `DiagramProcessor.post_process()` with caching; removes ~150 lines of duplicated Python diagram rendering logic
- **PageRenderer uses single Rust call** for diagram rendering with caching instead of extract+render+replace Python logic
- **Removed `diagrams` field** from `ConvertResult`; Python CLI now lists PNG files directly from output directory
- **Unified Confluence diagram rendering** via `DiagramProcessor` with `DiagramOutput::Files` mode; removes ~40 lines of duplicated orchestration code from `converter.rs`
- **Encapsulated DPI scaling in `RenderedDiagramInfo`** via `display_width(dpi)` and `display_height(dpi)` methods; removed `STANDARD_DPI` from public exports
- **Made internal APIs private** in `rw-diagrams`: `render_all`, `DiagramRequest`, `ExtractedDiagram`, `prepare_diagram_source`, `to_extracted_diagram`, `to_extracted_diagrams`, `RenderError`
- **Simplified `MarkdownConverter::convert()`** to return `ConvertResult` directly instead of `Result<ConvertResult, RenderError>`; errors are now handled internally by replacing placeholders with error messages
- **Removed `MarkdownRenderer::finalize()`** method; `render()` now auto-finalizes by calling `post_process()` on all registered processors
- **Made `DiagramProcessor.cache` non-optional** with `NullCache` default; removes duplicate code paths and simplifies caching logic
- **Encapsulated hash calculation in `DiagramCache`** via `DiagramKey` struct; cache implementations compute hashes internally, removing `compute_diagram_hash` from public API
- **Removed unused `RenderError` variants** (`Http`, `InvalidPng`) in `rw-diagrams`; individual diagram errors use `DiagramError`/`DiagramErrorKind`, aggregated via `RenderError::Multiple`
- **Consolidated duplicate hash implementations** in `rw-diagrams`; filename generation in `render_all` now uses `DiagramKey::compute_hash()` (truncated to 12 hex chars) instead of separate `diagram_hash` function, ensuring DPI is included in filename hash to prevent overwrites when same diagram is rendered at different DPIs
- **Separated config from state in `DiagramProcessor`** via internal `ProcessorConfig` struct; enables borrowing config immutably while mutating warnings, eliminating unnecessary clones in `post_process()` (idiomatic Rust pattern)
- **Simplified DPI handling** by defaulting to `DEFAULT_DPI` (192) at construction time instead of using `Option<u32>` throughout; removes `unwrap_or(DEFAULT_DPI)` boilerplate from multiple functions
- **Optimized placeholder replacement** in `DiagramProcessor` via internal `Replacements` struct; collects all replacements and applies them in a single pass instead of O(N × string_length) allocations from repeated `String::replace()` calls
- **Simplified `extract_requests_and_cache_info`** in `DiagramProcessor` using iterator `unzip()` instead of manual loop
- **Made `kroki_url` required when `[diagrams]` section is present** in config; the `[diagrams]` section is optional, but if provided, `kroki_url` must be set (validates at config load time with clear error message)
- **Simplified config resolution** in `rw-config` by using `DiagramsConfig::default()` and `iter().flatten()` pattern for cleaner optional field handling
- **Made `kroki_url` required in `DiagramProcessor` constructor**; clients decide whether to include the processor based on config; removes `Default` impl and `kroki_url()` builder method
- **Simplified parallel rendering** in `rw-diagrams` by using rayon's global thread pool instead of per-call thread pool creation; all render functions now return `PartialRenderResult` for consistent partial-success handling; removed `RenderError` type; extracted `create_agent()` and `partition_results()` helpers
- **HTTP agent reuse for connection pooling** in `rw-diagrams`; the `ureq::Agent` is now stored in `ProcessorConfig` and reused across render calls instead of creating a new agent per call, enabling HTTP connection pooling for improved performance
- **Optimized cache lookup in `post_process_inline`** by consuming the prepared diagrams iterator and constructing `DiagramKey` directly for cache hits; eliminates unnecessary `CacheInfo` allocation and string clone for cached diagrams
- **Added capacity hint for `Replacements` HashMap** in `DiagramProcessor`; pre-allocates based on diagram count to reduce rehashing
- **Reduced `rw-confluence` public API**; internal types now private: `ConfluenceBackend`, `PageRenderer`, `TreeNode`, `PreserveResult`, `preserve_comments()`, `OAuth1Auth`, `types::*` re-exports; consumers use `ConfluenceClient::from_config()` and `PageUpdater` instead of internal implementation details
- **`ConfluenceClient::from_config()` and `OAuthTokenGenerator::new()` now take key file path** instead of key bytes; file I/O moved from CLI to library; removes `oauth::read_private_key` from public API

### Removed

- **`rw-core` crate**; merged into `rw-confluence`; all Confluence integration functionality is now in a single crate
- **`MarkdownConverter` type**; replaced by `PageRenderer` in `rw-confluence` with `convert()` renamed to `render()`
- **`ConvertResult` type alias**; use `RenderResult` from `rw-renderer` (re-exported by `rw-confluence`) instead
- **Python CLI package** (`packages/rw/`); replaced by native Rust CLI in `crates/rw/`
- **PyO3 bindings package** (`packages/rw-core/`); no longer needed
- **Python tooling**: `pyproject.toml`, `uv.lock`, `.python-version`
- **`confluence upload-mkdocs` command**; use `confluence update` instead with appropriate `include_dirs` and `config_file` in `rw.toml`
- **`confluence comments` command**; comment information is available in the Confluence UI
- **`confluence convert` command**; use `confluence update --dry-run` to preview conversion
- **`confluence create` command**; create pages manually in Confluence, then use `confluence update` to sync content
- **`confluence get-page` command**; use the Confluence REST API directly or the web UI
- **`confluence test-auth` command**; use `confluence update --dry-run` to verify authentication
- **`confluence test-create` command**; use `confluence update --dry-run` to verify permissions
- **Unused `ConfluenceClient` methods** (`create_page`, `base_url`, `get_inline_comments`, `get_footer_comments`); removed to reduce API surface and eliminate dead code
- **Unused Confluence API types** (`Comment`, `Extensions`, `InlineProperties`, `Resolution`); `CommentsResponse` simplified to only include `size` field; `Attachment` simplified to only `id` and `title`; serde ignores unknown fields by default so unused API response fields are skipped

### Fixed

- **Path traversal protection in `Site::resolve_source_path`**; canonicalizes resolved paths and validates they stay within `source_dir` to prevent directory traversal attacks
- **OAuth 1.0 signature now includes query parameters** per RFC 5849 Section 3.4.1.3; fixes `signature_invalid` errors for Confluence API requests with query strings (e.g., `get_page` with `expand` parameter)
- **Ctrl-C signal handling** now works for `rw serve`; uses tokio graceful shutdown instead of relying on Python signal handlers
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

- Rust core library (`rw-core`) for markdown conversion
- Python CLI package (`rw`) with aiohttp server
- PyO3 bindings for Rust/Python interop
- Parallel diagram rendering via rayon
