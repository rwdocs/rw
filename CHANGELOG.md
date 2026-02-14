# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Default port changed from 8080 to 7979 (RWRW on a telephone keypad)
- Rewrote README for external audience; moved reference material to `docs/`

### Fixed

- `rw serve` and `rw techdocs` commands no longer fail when confluence environment variables are not set

## [0.1.5] - 2026-02-12

### Added

- `rw techdocs build` command for generating static documentation sites (Backstage TechDocs compatible)
- `rw techdocs publish` command for uploading sites to S3

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
