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
- **Navigation sections grouping** groups pages by `type` in sidebar
  - Pages with `type` metadata are grouped under labels (e.g., "Domains", "Systems")
  - Groups appear alphabetically, ungrouped pages appear after groups
  - `section_type` field added to `/api/navigation` response
- **Virtual pages** for directories with `meta.yaml` but no `index.md`
  - Directories with metadata appear in navigation with auto-generated child index
  - Virtual pages render a list of child pages with links and descriptions
  - Supports nested virtual pages for organizing section hierarchies
- **Scoped section navigation** for hierarchical documentation sites
  - Navigation scopes to current section when viewing pages inside a section
  - Sections are leaf nodes in parent scope (no subpage expansion)
  - Back navigation to parent scope or Home from within sections
  - `navigationScope` field added to page API response
  - `scope` and `parentScope` fields added to navigation API response
  - Supports `?scope=` query parameter in `/api/navigation`
- Page loading progress for slow updates

### Security

- Storage errors no longer expose full filesystem paths in API responses

### Changed

- Metadata YAML files now ignore unknown fields instead of failing to parse

### Fixed

- Scoped navigation now preserved when navigating within a section with cached API responses
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
