# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Simplify `rw-storage-fs` internals: extract shared watcher event logic, streamline URL path building, mtime retrieval, and event coalescing
- Simplify `rw-storage-fs` internals: extract `load_ancestor_meta` helper to reduce nesting in metadata inheritance, merge title caching into `extract_or_derive_title`, and simplify `virtual_page_title` and YAML field parsing
- Simplify `rw-storage-fs` internals: flatten `build_document` title resolution into linear if/else chain, consolidate `derive_title_from_filename` to accept `Path` directly, flatten `meta()` loop with `if let`, simplify `SourceFile::classify` by extracting kind/url_path as tuple, and use early-return in debouncer `drain_ready`
- Simplify `rw-storage-fs` internals: use `file_stem()` directly in `derive_title_from_filename`, scope mutex lock in `extract_or_derive_title` with block expression, streamline `load_ancestor_meta` with `map_err`/`ok()?` chains, use `is_some_and` in `file_path_to_url`, simplify debouncer coalesce with `if let`, and consolidate `Arc` import
- Simplify `rw-storage-fs` internals: replace `meta()` accumulation loop with `reduce`, use `find_map` in YAML field extraction, simplify `extract_title_from_content` with direct index into captures, use `as_deref` for `Option<String>` references in `build_document`, use `is_none_or` for virtual page empty check, replace `map_or` with `map_or_else` in `derive_title_from_filename` and `file_path_to_url`, use match expression in `virtual_page_title`, and avoid `Vec` allocation in `titlecase_from_slug`
- Simplify `rw-storage-fs` internals: extract `capitalize_first` helper and use `map`/`join` in `titlecase_from_slug`, bind `meta_str` once in `build_document` to avoid repeated `as_deref`, replace mutable `accumulated` in `meta()` with `Option::map` pipeline, remove unnecessary dereferences in `SourceFile::classify`, and rename shadowed `pending` variable in debouncer for clarity
- Simplify `rw-storage-fs` internals: eliminate `Vec` allocation in `titlecase_from_slug` by writing directly into `String`, use `inspect_err` instead of `map_err`/`ok()` in `load_ancestor_meta`, use `Path::extension()` instead of string matching in `SourceFile::classify`, use `as_encoded_bytes()` for hidden file detection in scanner, and use `map().unwrap_or_default()` pattern in `derive_title_from_filename` and `file_path_to_url`
- Simplify `rw-storage-fs` internals: avoid intermediate `String` allocation in `titlecase_from_slug` by using `split` instead of `replace`, use `with_extension` in `resolve_content` to avoid `format!`, flatten `meta()` inheritance clearing with `let`-chain, and use `let ... else` early return in `file_path_to_url`
- Simplify `rw-storage-fs` internals: use `flatten()` instead of `filter_map(Result::ok)` in scanner, use `starts_with` for hidden file detection, use `match` instead of `if let` in debouncer coalescing, and avoid intermediate `HashMap` clone in `merge_metadata`

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
