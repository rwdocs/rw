# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `rw techdocs build` command for generating static documentation sites (Backstage TechDocs compatible)
- `rw techdocs publish` command for uploading sites to S3
- New `rw-techdocs` crate with `StaticSiteBuilder` and `S3Publisher`
- Relative link mode in `MarkdownRenderer` (`with_relative_links(true)`) for static site builds where links must be relative to each page's location
- `relative_path()` utility in `rw-renderer` for computing relative URL paths between pages
- `relative_links` option in `PageRendererConfig` (default `false`, opt-in for TechDocs)
- `trailing_slash` option in `MarkdownRenderer` and `PageRendererConfig` for URLs with trailing slashes (e.g., `/a/b/` instead of `/a/b`), needed for TechDocs static site output
- Meta diagram includes: PlantUML `!include` directives resolve C4 model macros from `meta.yaml` metadata (supports domain/system/service types)
- Diagram `$link` URLs now respect `relative_links` and `trailing_slash` settings via `LinkConfig` on `DiagramProcessor`

### Fixed

- `rw techdocs build` now copies font files (`.woff`/`.woff2`) to the output `assets/` directory via `rw-assets` crate, fixing 404 errors and system font fallback
- `rw techdocs build` scoped navigation now renders back link, section title, and type group labels (e.g., "SYSTEMS") matching the `rw serve` frontend
- `rw techdocs build` tabs are now interactive via CSS-only radio inputs (no JavaScript needed), matching mkdocs-material's approach for Backstage TechDocs compatibility

### Changed

- Replaced hand-rolled `push_str` HTML generation in `rw-techdocs` template with minijinja template engine for improved readability and maintainability
- Removed `DEFAULT_CSS` fallback and `css_content` option from `BuildConfig`; `rw techdocs build` always uses frontend assets via `rw-assets`
- Extracted `rw-assets` crate for shared frontend asset access (embedded + filesystem modes); `rw-server` no longer owns `rust-embed` or `mime_guess` deps
- `MarkdownRenderer::with_base_path()` now expects URL paths with leading `/` (e.g., `/a/b` instead of `a/b`); storage-to-URL conversion moved to `PageRenderer`
- Extracted `PageRenderer` from `Site` for independent page rendering testability
- Renamed `SiteConfig` to `PageRendererConfig` and moved to page module (colocated with `PageRenderer`)
- Moved `Page` and `BreadcrumbItem` from `site_state` to `page` module (removes renderer dependency on site state types)
- Renamed `PageRenderer::render_page()` to `render()` (method names shouldn't repeat the type name)
- Reordered `PageRenderer::new()` and `Site::new()` args: dependencies (`storage`, `cache`) before config
- Introduced `SiteSnapshot` to bundle `SiteState` + `TypedPageRegistry` as an atomic unit; `Site` now swaps a single `Arc<SiteSnapshot>` instead of separate state and registry
- Moved cache serialization types (`CachedSiteStateRef`, `CachedSiteState`) from `site` to `site_state` module (reduces `Site` responsibilities)
- `SiteState` now owns its cache persistence via `from_cache()`/`to_cache()` methods; cache format types are private
- Removed `TypedPageRegistry`; `SiteSnapshot` implements `MetaIncludeSource` directly using `SiteState`'s name-based section index
- Added `description` field to `Document` and `SectionInfo` (flows from `meta.yaml` through the full pipeline)
- `SiteState` now indexes sections by directory name for O(1) lookup via `find_sections_by_name()`

## [0.1.4] - 2026-02-11

### Changed

- PlantUML diagrams now use Roboto font by default (`skinparam defaultFontName Roboto`)
- Removed `diagrams.config_file` config option (font is now hardcoded)
- Cache directory moved from `.cache/` to `.rw/cache/` (`.rw/` is the new project directory)
- Removed `cache_dir` config option and `--cache-dir` CLI flag (cache location is no longer configurable)
- `.rw/.gitignore` is auto-created on first run to exclude project directory from version control
- Cache is now fully invalidated on version upgrade via `.rw/cache/VERSION` file

## [0.1.3] - 2026-02-09

### Added

- Auto-detect `README.md` as homepage when `docs/index.md` doesn't exist

## [0.1.2] - 2026-02-09

### Added

- Page metadata support via YAML sidecar files (`meta.yaml`)
- Navigation sections grouping pages by `type` in sidebar
- Scoped section navigation for hierarchical documentation sites
- Page loading progress for slow updates

### Security

- Storage errors no longer expose full filesystem paths in API responses

### Fixed

- Hash fragment navigation now properly scrolls to the target heading
- TOC now correctly highlights the clicked item instead of showing the wrong one
- TOC items now show pointer cursor on hover
- Removed animated scroll behavior
- Prevent memory leaks on frontend
- Navigation loading errors are now displayed to users

## [0.1.1]

### Fixed

- **cargo-dist builds** now embed frontend assets in binary to prevent 404 errors on installation
- **build.rs** automatically builds frontend assets when `embed-assets` feature is enabled

## [0.1.0]

Initial release of RW - a documentation engine for converting markdown to HTML and Confluence pages.

### Added

- **Documentation server** with Svelte 5 frontend
- **Markdown to HTML** conversion with syntax highlighting
- **Markdown to Confluence** conversion with XHTML output
- **Navigation sidebar** with collapsible tree structure
- **Table of contents** with scroll spy
- **Breadcrumbs** for page hierarchy
- **Mobile responsive** layout
- **Live reload** with optimized file watching (~5ms for content edits)
- **File-based caching** for fast page loads
- **Diagram rendering** via Kroki (PlantUML, Mermaid, GraphViz, and 14+ formats)
- **Tabbed content blocks** using `:::tab[Label]` syntax
- **GitHub-style alerts** (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`)
- **Confluence publishing** via REST API with OAuth 1.0 RSA-SHA1
- **Comment preservation** when updating Confluence pages
- **Configuration** via `rw.toml` with auto-discovery and environment variable expansion
- **Security headers** (CSP, X-Content-Type-Options, X-Frame-Options)
- **Path traversal protection** for secure file serving

### CLI Commands

- `rw serve` - Start documentation server
- `rw confluence update` - Update Confluence pages from markdown
- `rw confluence generate-tokens` - Generate OAuth access tokens
