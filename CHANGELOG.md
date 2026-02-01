# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [unreleased]

### Added

- Page metadata support via YAML sidecar files (`meta.yaml`)
- Navigation sections grouping pages by `type` in sidebar
- Scoped section navigation for hierarchical documentation sites
- Page loading progress for slow updates

### Changed

- **Storage API redesign**: `Document` now includes `has_content` and `page_type` fields for unified document model. Virtual pages (directories with metadata but no `index.md`) are now discovered by Storage instead of Site
- `Metadata` moved from rw-site to rw-storage for reuse by future storage backends
- `Storage.meta()` now returns `Option<Metadata>` with inheritance applied (vars are inherited, title/description/page_type are not)
- Site uses lazy metadata loading during render instead of eager loading during scan
- Metadata file naming convention is now encapsulated in Storage via `meta()` method
- **Storage crate split**: `rw-storage` now contains only the core `Storage` trait, error types, event types, `Metadata` struct, and `MockStorage`. Filesystem implementation moved to new `rw-storage-fs` crate with `FsStorage`, enabling future backends like `rw-storage-s3` or `rw-storage-redis`
- `Storage::scan()` now returns `Vec<Document>` directly instead of `ScanResult` wrapper
- `FsStorage::scan()` no longer sorts results or filters `node_modules`/`target`/`_` prefixed paths - sorting is presentation logic handled by Site, and directory filtering is user responsibility via `source_dir` configuration
- **Scanner extraction**: `FsStorage` now uses a separate `Scanner` struct for document discovery, separating filesystem walking (Phase 1) from document building (Phase 2). This improves testability and enables future partial scanning optimizations
- `DocumentRef` now uses explicit `content_path` and `meta_path` fields instead of `sources: Vec<PathBuf>`, making file type identification the Scanner's responsibility
- `Scanner::new()` now accepts `&Path` instead of `PathBuf` to avoid unnecessary cloning
- **Scanner refactoring**: Replaced recursive `scan_directory()` with stack-based iteration and HashMap-based grouping. Extracted file classification logic into `SourceFile` struct (`source.rs`), separating concerns between file type detection and document grouping
- Extracted `titlecase_from_slug()` utility to eliminate duplicated title generation logic
- Use explicit `Arc::clone()` in `Site` for clarity
- Remove unnecessary `alignments.clone()` in table rendering (already owned)

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
