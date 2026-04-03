# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.22] - 2026-04-03

### Added

- `renderSearchDocument()` method on `RwSite` — renders markdown pages to plain text for search indexing, stripping HTML formatting and replacing diagrams with meaningful text descriptions

## [0.1.21] - 2026-04-02

### Added

- Wikilink syntax for section-stable internal links — `[[domain:billing::overview]]` resolves via section registry instead of filesystem paths, surviving directory reorganization. Supports explicit display text (`[[target|text]]`), current-section links (`[[::page]]`), and fragment links. Unresolved wikilinks render with a visual broken-link indicator.
- Frontmatter support — page metadata can now be defined in YAML frontmatter (`---` delimited) at the top of markdown files, in addition to meta.yaml sidecar files. Frontmatter values override meta.yaml when both exist.
- `reload(force?)` method on `RwSite` — when called without `force` (or `force=false`), checks whether S3 content has changed before reloading, using S3 ETags to skip unnecessary reloads. `reload(true)` forces an unconditional reload like before.

### Fixed

- Directory renames under `docs/` are now detected by live reload — previously, renaming a directory required manually deleting `.rw/cache` and restarting the server
- Page metadata no longer extracts `#` comments inside fenced code blocks as H1 titles
- Page metadata now correctly extracts plain text from H1 titles with inline formatting (bold, italic, code, links)
- Editing a page title inside a section no longer resets the sidebar to root navigation
- Navigation sidebar no longer flashes "Loading..." text during live reload when editing markdown files

## [0.1.20] - 2026-03-24

### Fixed

- S3 storage errors now include the full error chain (e.g., TLS, DNS, or connection details) instead of just "dispatch failure"

## [0.1.19] - 2026-03-24

### Fixed

- S3 storage errors now propagate instead of silently returning empty site — misconfigured or unreachable S3 returns proper error messages to the Backstage plugin and 503 responses from the HTTP server

## [0.1.18] - 2026-03-23

### Added

- Cross-section link annotation — all internal links now include `data-section-ref` and `data-section-path` attributes on the rendered `<a>` element, enabling host applications to resolve entity page URLs at runtime. Works for both markdown links and diagram links (PlantUML `$link` URLs rendered via Kroki)
- `resolveSectionRefs` option for `mountRw()` — host applications can provide a resolver that maps section refs to base URLs, enabling cross-entity link navigation in Backstage and other embedded contexts
- `sectionRef` field on navigation items, scope info, and breadcrumbs in both server API and `@rwdocs/core` responses

### Changed

- Section metadata field renamed from `type` to `kind` to align with Backstage and Kubernetes conventions — `type` is still accepted in YAML for backward compatibility
- API responses now use a nested `section: { kind, name }` object (was flat `sectionType`/`sectionKind` fields) and `sectionRef` string (e.g., `domain:default/billing`) on navigation items, scope info, and breadcrumbs
- Embedded viewer (`mountRw()`) now uses flow layout — content takes its natural height and the parent page controls scrolling, instead of filling a fixed container with internal scroll. Hash fragment scrolling now works in embedded mode.
- `mountRw()` API simplified — `basePath` and `scopePath` options replaced by a single `sectionRef` string; the viewer derives path mappings at runtime using `resolveSectionRefs` and the navigation API
- Navigation API and `@rwdocs/core` `getNavigation()` now accept `sectionRef` (e.g., `"domain:default/billing"`) instead of a filesystem `scope` path — page responses return `sectionRef` instead of `navigationScope`

### Removed

- `rw techdocs build` and `rw techdocs publish` commands — use native Backstage plugins ([rwdocs/backstage-plugins](https://github.com/rwdocs/backstage-plugins)) instead
- `linkPrefix` option from `@rwdocs/core` `createSite()` config — use `resolveSectionRefs` in `mountRw()` for link URL construction in embedded mode

## [0.1.17] - 2026-03-10

### Fixed

- Fixed `@rwdocs/core` linux-x64-gnu binary segfault on Debian 12 by building on Ubuntu 22.04 (glibc 2.35)

## [0.1.16] - 2026-03-10

### Fixed

- `@rwdocs/core` linux-x64-gnu binary now targets glibc 2.17, fixing "GLIBC_2.38 not found" errors on Debian 12 and other older Linux distributions

## [0.1.15] - 2026-03-10

### Added

- `accessKeyId` and `secretAccessKey` options for `@rwdocs/core` S3 config to pass AWS credentials explicitly instead of relying on environment variables

### Fixed

- Clicking links in diagrams (e.g., C4 `$link` URLs) no longer triggers a full page reload — links now use SPA routing

## [0.1.14] - 2026-03-10

### Fixed

- Embed CSS no longer uses `@layer`, fixing viewer styles being overridden by host app resets (e.g., MUI's CssBaseline)
- Embedded viewer now sets `font-size: 16px` on its root element, preventing host font-size from breaking em-based typography sizing
- C4 diagram `$link` URLs now include `linkPrefix` when serving from S3 bundles

## [0.1.13] - 2026-03-09

### Added

- `diagrams` option for `@rwdocs/core` `createSite()` to configure `krokiUrl` and `dpi` without `rw.toml`
- `setColorScheme()` method on `RwInstance` to update the color scheme without re-mounting the viewer

## [0.1.12] - 2026-03-09

### Added

- `--embedded` flag for `rw serve` to preview docs inside a Backstage-like shell during development
- "On this page" popover button for accessing table of contents when the sidebar is hidden on narrow screens
- S3-backed diagram cache for embedded mode — diagrams rendered via Kroki are cached in S3, avoiding re-rendering on every page request in Backstage deployments

### Changed

- Viewer layout now uses container queries instead of viewport breakpoints, adapting to actual available space when embedded in host applications
- Mobile header now shows breadcrumbs and table of contents button instead of the logo

### Fixed

- Fixed hash fragment navigation not scrolling to headings with non-Latin characters (e.g., Cyrillic) when opening a URL directly
- Fixed sidebar, table of contents, mobile drawer, and loading bar overflowing container bounds when viewer is embedded in a smaller host element
- Long breadcrumb trails progressively collapse middle items into a "..." dropdown, showing as many items as fit
- Embed library CSS is now scoped under `[data-rw-viewer]` to prevent style leaks into host pages
- Embed library no longer bundles font files, reducing CSS bundle size by 96%
- Clicking a heading in the mobile "On this page" menu no longer scrolls the heading behind the sticky header
- Scrolling back to top after using mobile table of contents no longer hides the page title behind the mobile header
- Fixed page content flickering (shifting left and back) when navigating between pages on wide viewports
- Fixed flash of unstyled text (FOUT) by preloading critical fonts
- Fixed page not scrolling to top when navigating between pages
- Fixed page title vertical position misaligned between home page and inner pages

## [0.1.11] - 2026-03-04

### Added

- Dark theme support — automatically follows OS dark mode preference
- `colorScheme` option for embedded mode (`mountRw`) to set 'light', 'dark', or 'auto'

### Fixed

- Fixed navigation sidebar collapsing after page refresh on inner pages

## [0.1.10] - 2026-03-01

### Fixed

- `@rwdocs/core` npm package now includes JavaScript bindings and TypeScript declarations
- `@rwdocs/viewer` npm package now ships generated `.d.ts` type declarations

## [0.1.9] - 2026-03-01

### Added

- `rw backstage publish` command for publishing documentation bundles to S3 for the Backstage plugin
- S3 storage backend for serving docs in deployed Backstage instances without local files
- Frontend can now be embedded in external host applications (e.g., Backstage plugins) with configurable API base URL, memory-based routing, and no browser side effects
- Node.js native bindings (`rw-napi`) for embedding RW in Node.js applications via napi-rs
- Published `@rwdocs/core` and `@rwdocs/viewer` to npm (macOS arm64, Linux x64, Linux x64 musl/Alpine)

## [0.1.8] - 2026-02-26

### Changed

- `rw techdocs build` now renders pages in parallel, significantly speeding up diagram-heavy sites
- Startup scan is now ~3x faster on large sites (parallel directory walking and document building)

### Fixed

- Fixed unnecessary full site rescan on every file save in editors that use atomic writes (vim, neovim)
- Fixed heading anchors for non-Latin characters (Cyrillic, CJK, etc.) producing empty IDs
- Fixed navigation sidebar blinking on every file save when only page content changed
- Fixed navigation not updating when page title is changed (H1 heading or meta.yaml)

## [0.1.7] - 2026-02-16

### Added

- Mobile navigation toggle for TechDocs output (CSS-only hamburger menu for narrow viewports)

### Changed

- Removed bundled Roboto font from `rw techdocs build` output (Backstage already provides it)

### Fixed

- Fixed table of contents not staying sticky when scrolling in TechDocs output
- Fixed long table of contents being cut off when it exceeds viewport height (now scrollable)

## [0.1.6] - 2026-02-14

### Changed

- Default port changed from 8080 to 7979 (RWRW on a telephone keypad)
- Rewrote README for external audience; moved reference material to `docs/`

### Fixed

- `rw serve` and `rw techdocs` commands no longer fail when confluence environment variables are not set

## [0.1.5] - 2026-02-14

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
