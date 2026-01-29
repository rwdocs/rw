# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [unreleased]

### Added

- **Page metadata support** via YAML sidecar files (`meta.yaml`)
  - Custom page titles, descriptions, and types
  - Sub-site/section definitions (directories with `type` set)
  - Custom variables with inheritance from parent directories
  - New `/api/sections` endpoint for listing all sections
  - New `[metadata]` config section with `name` option (default: `meta.yaml`)
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
