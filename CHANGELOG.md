# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Named metadata sidecar files — place `<name>.meta.yaml` (e.g. `payments.meta.yaml`) directly in a directory to declare a page or content-less catalog entity at that path, instead of creating a `<name>/meta.yaml` subfolder. Works as a sidecar for an existing `<name>.md` (metadata from the sidecar, content from the markdown) or stand-alone to register Backstage components/systems that exist only to build relations. The suffix follows the configured metadata filename (`<name>.config.yml` for a custom `config.yml`). Named sidecars are leaf-only — they do not cascade `vars` to descendants — and a directory `meta.yaml` wins if both resolve to the same page. A file named `index.meta.yaml` is accepted as directory metadata but logs a warning suggesting it be renamed to `meta.yaml`.
- Inline comments — select text in the browser and add comments anchored to specific passages; comments persist in `.rw/comments/sqlite.db` and survive content re-renders via multi-selector anchoring (TextQuoteSelector + TextPositionSelector). Commented passages carry a yellow highlight and a **solid** underline. When the original passage no longer appears verbatim — e.g. after a typo fix or a paragraph split that drops a space — the viewer falls back to fuzzy re-anchoring (diff-match-patch) and marks the comment as **re-anchored** with a **dashed** underline (same color, so only the line style differs) and an italic label in the thread header, so reviewers can tell when a comment may have moved.
- Page comments — leave comments on a page without selecting text; page comments appear below the article content with the same threading and resolve/reopen workflow as inline comments
- Comment REST API (`/_api/comments`) for creating, listing, and updating inline annotations
- Comment authorship — every comment carries an author (`{ id, name, avatarUrl? }`); `rw serve` stamps browser-created comments with "You". Authors with id `local:human` render a person avatar; authors with id `local:ai` render a sparkles avatar (recommended for LLM agents writing via `rw comment`); others fall back to name initials.
- `rw comment` CLI (list, show, add, reply, resolve) for scripting and LLM agents — reads and writes comments in the project's `.rw/comments/sqlite.db` directly; works whether or not `rw serve` is running. Identity via `RW_COMMENT_AUTHOR_ID` / `RW_COMMENT_AUTHOR_NAME` env vars or `--author-id` / `--author-name` flags; falls back to the default `local:human` / "You" identity when neither is set. Inline anchoring via `--quote "passage text"` — the CLI renders the target page in-process and rejects ambiguous or missing matches.
- Status badges — `:status[Label]{color=NAME}` renders an inline colored pill label (`grey`, `red`, `yellow`, `green`, `blue`, `purple`). Publishing to Confluence emits the native `status` macro, so badges stay editable and on-style in Confluence. Color is case-insensitive and optional; unknown or omitted colors fall back to `grey`.
- Custom section namespaces — sections can declare a `namespace` in `meta.yaml` or frontmatter (e.g. `namespace: payments`), producing section refs of the form `kind:namespace/name` that map to Backstage catalog entities outside the `default` namespace. The field inherits down the directory tree (set it once at the site root) and a subtree can override it. Namespace values are validated against the Backstage charset; an invalid value fails the site load with an error naming the offending file. Wikilinks that omit the namespace resolve within the current page's namespace.
- `rw backstage publish` now surfaces diagram-processing warnings — broken or cyclic PlantUML `!include` paths — in yellow on stderr instead of silently discarding them. Pass `--strict` to fail the publish (non-zero exit) when any warning was emitted; bundles still upload either way, so warnings can be fixed in a follow-up commit and republished.
- `RW_DIAGRAMS_KROKI_URL` environment variable — when set, supplies `diagrams.kroki_url` for projects without an `rw.toml` (or with an `rw.toml` that omits the field). Precedence is CLI flag > `rw.toml` value > env var, so explicit project config still wins. Lets teams roll `rw` out across many repos that share a single Kroki server by exporting the variable once (dev container, CI runner, dotfiles) instead of placing a config file in each repo.
- `rw confluence render <markdown_file> --out <dir|->` — renders markdown
  to a Confluence-publishable bundle (`page.xhtml` plus one PNG per
  diagram). Title, renderer warnings, and comment markers that could not
  be re-anchored go to stderr as plain text. Stdin optionally accepts the
  current page's storage XHTML body for inline-comment-marker preservation;
  without it, the command renders as a fresh page. `--out -` writes the
  body XHTML to stdout and errors with exit 3 only if the render actually
  produced any PNG attachments (so a doc with no diagram fences pipes
  cleanly regardless of `--kroki-url`); `--strict` exits non-zero on any
  warning or unmatched comment.
- Delete replies from the browser. Each reply in a comment thread has a Delete button; clicking it immediately marks the reply as deleted on the server (`deletedAt` set on the row, never destructively removed) but keeps the reply visible on the page in a muted, struck-through state with a Restore button — so a misclick can be undone in the same session without confirmation prompts. Top-level comments are not deletable; use Resolve instead. Reload, navigation, or any other comment refetch hides deleted replies. The HTTP API gains `DELETE /_api/comments/{id}` (returns the soft-deleted row, idempotent on repeat), a `deletedAt` timestamp field on every comment (omitted when the row is live), and `canDelete` / `canRestore` flags, letting backends serving the viewer (e.g. a future Backstage plugin) enforce per-user permissions.

### Removed

- **Breaking:** `rw confluence update` and `rw confluence generate-tokens`
  are removed. `rw` no longer talks to the Confluence REST API; the
  `[confluence]` section in `rw.toml` is no longer recognized (stale
  sections are silently ignored, not rejected — delete them from your
  config when upgrading). Use `rw confluence render <md> --out <dir>` to
  produce a publish-ready bundle (XHTML body + diagram PNGs), then publish
  it with a tool of your choice. Comment preservation continues to work:
  pipe the current page's storage XHTML body into stdin and `rw` carries
  `<ac:inline-comment-marker>` tags through to the new XHTML.

### Changed

- Comment bodies now render as formatted markdown instead of plain text. A restricted CommonMark+GFM subset is supported — paragraphs and line breaks (so blank-line-separated paragraphs no longer collapse into one run-on line), **bold**/_italic_/~~strikethrough~~, bullet/ordered/task lists, blockquotes (GitHub `[!NOTE]`-style alerts render as plain blockquotes), inline and fenced code, and `http`/`https`/`mailto` links. Headings are demoted to paragraphs, tables are flattened to text, images are dropped, and raw HTML / unsafe link schemes are neutralized — keeping the narrow comment column tidy and safe. Rendering happens server-side in Rust (shared `rw-renderer` logic), so the same formatting applies wherever comments are served; the raw `body` is unchanged and a new additive `bodyHtml` field carries the rendered HTML.
- The page-comments block now leads with a labeled "Comments" header and a count badge instead of appearing as bare threads below a horizontal line, so the discussion zone reads as a deliberate section. The count badge shows the number of open page-level (and orphaned-inline) threads and is hidden when there are none.
- Orphaned inline comments shown in the page-comments timeline now render their original-passage quote inside the first comment message (between the author and the comment text) instead of as a detached block above the comment card, so the quote reads as part of the comment that refers to it.
- Single-page sites (only `index.md`, or a `README.md` homepage) no longer show an empty navigation sidebar. When there are no other pages to navigate to, the desktop sidebar, the mobile hamburger button, and the mobile navigation drawer are all hidden, leaving a clean centered article. Sites with a "back to home" link (a section page that simply has no child pages) keep their sidebar.
- The comments database (`.rw/comments/sqlite.db`) gains a versioned schema (`schema_versions` table). On startup, the binary applies any pending forward migrations idempotently. If the DB is at a schema version newer than the binary supports (e.g. you ran a newer `rw` and then tried to start an older one), startup fails cleanly with an "incompatible schema" error instead of corrupting reads — downgrades are not supported across schema versions.
- **Breaking (`rw-renderer` Rust API):** GitHub Flavored Markdown (tables, strikethrough, task lists, and `[!NOTE]`-style alerts) is now always enabled and the `MarkdownRenderer::with_gfm` builder method has been removed. GFM was already on in every shipped `rw` code path, so no end-user `rw` behavior changes; only downstream crates that called `with_gfm(false)` to opt out need to migrate — there is no longer a way to disable GFM.
- Minimum supported Rust version raised to 1.96 — downstream crates embedding `rw-renderer` (or any other `rw-*` crate) now need a 1.96+ toolchain to build
- Page outline (TOC) sidebar widened from 240px to 320px so that opening a comment no longer narrows the article or shifts text sideways; the sidebar now appears at viewport widths ≥ 1304px instead of ≥ 1224px (narrower viewports get the floating "On this page" popover as before)
- Page modification times (`lastModified` in API responses) now reflect the git commit time instead of the filesystem modification time — timestamps remain stable across `git checkout`, `git pull`, and branch switching
- S3-published documentation bundles now include page modification times in the manifest — previously `lastModified` was always epoch zero for Backstage-served pages
- `@rwdocs/viewer` now requires Node.js `>=22.12.0` (was `>=20`) — the previous floor was incorrect since Vite 8 needs Node 20.19+/22.12+, and Node 20 reached end-of-life on 2026-04-30; `22.12.0` is the first release where `require(esm)` works without a flag
- **Breaking:** the HTTP API served by `rw serve` moved from `/api/*` to the reserved `/_api/*` prefix (e.g. `/_api/navigation`, `/_api/pages/...`, `/_api/comments`), freeing the `/api/*` URL space for documentation pages. The bundled viewer moves in lockstep; only external callers hitting `rw serve`'s HTTP endpoints directly need to update.
- **Breaking (`rw-renderer` Rust API):** `DirectiveContext::resolve_path` now returns `Result<PathBuf, ResolveError>` and rejects inputs that would escape the base directory (absolute paths, Windows-specific prefixes such as `C:\…` / `\\?\…` / UNC, `..` segments that pop above the root, and control bytes). The previous unchecked variant and the canonicalize-based `resolve_path_safe` have both been removed. No directive shipped in `rw` itself used either method, so end-user `rw` binaries are unaffected; only downstream crates that embed `rw-renderer` and implement custom directives (e.g. an `::include` handler) need to migrate.
- **Breaking (`rw-renderer` Rust API):** code-block processors and the directive processor moved off `MarkdownRenderer` onto a new `Pipeline` type, and the render surface was consolidated to a single entry point. `MarkdownRenderer` now has one render method — `render(&self, markdown: &str, pipeline: Pipeline) -> RenderResult` (formerly `render_markdown`); it takes `&self` (was `&mut self`) and always runs the full pipeline (block-directive preprocessing, event walk with inline-directive expansion, and directive post-processing with warning collection). The old events-iterator `render` overload and the public `parser_options` / `create_parser` helpers that fed it are removed. The builder methods `with_processor` and `with_directives` moved from `MarkdownRenderer` to `Pipeline`. The `MarkdownRenderer::processor_warnings` method was removed; warnings are collected into `RenderResult.warnings` by `render` itself. `CodeBlockProcessor` and `TitleResolver` now require `Send + Sync` bounds. Migration: `MarkdownRenderer::<B>::new().with_directives(d).with_processor(p).render_markdown(md)` becomes `MarkdownRenderer::<B>::new().render(md, Pipeline::new().with_directives(d).with_processor(p))`; callers that built their own pulldown-cmark event iterator and passed it to the old `render` should pass the markdown string to `render` instead. No end-user `rw` binary behavior changes — only downstream crates embedding `rw-renderer` need to migrate.
- **Breaking (`rw-renderer` Rust API + directive syntax):** leaf (`::name`) and
  container (`:::name` … `:::`) directives are now recognized during the
  pulldown-cmark event walk instead of a line-based pre-pass, so they respect
  markdown block structure (a delimiter indented into a code block, or inside a
  fenced block, is left literal; directives work inside blockquotes and loose
  list items). **Block directives must now be blank-line separated** — each
  `:::name` / `::leaf` / `:::` on its own paragraph; a delimiter not separated
  by blank lines renders as literal text. `DirectiveProcessor::process` is
  removed (the renderer no longer pre-processes; drive `MarkdownRenderer::render`
  directly). `DirectiveOutput::Markdown` (e.g. `::include`) is still re-parsed in
  context. No end-user `rw` behavior changes for blank-line-separated `:::tab`
  blocks; only downstream crates embedding `rw-renderer` and any docs relying on
  the old no-blank-line form need to migrate.
- **Breaking:** heading anchor IDs (slug IDs in the `id="..."` attribute of `<h*>` elements) for headings containing `[[wikilink]]` syntax now include the wikilink's resolved display text. Previously `## See [[overview]]` (with resolver returning "Overview") produced `<h2 id="see">`; now it produces `<h2 id="see-overview">`. Any in-page anchor links targeting the old slugs need to be updated. The TOC entry title for such headings changes correspondingly (was "See", now "See Overview").
- Inline comments on overlapping passages now render with a visibly darker yellow at the overlap region — previously two threads on the same text looked like a single uniform highlight. Each comment now wraps its text in `<rw-annotation>` elements; nested wrappers alpha-composite via the CSS box model so 2-way, 3-way, etc. overlaps stack progressively darker.

### Fixed

- `rw serve` no longer fails to start when a project has a `README.md` but no `docs/` directory (`Error: failed to start file watcher: ... No path was found`). The README is served as the homepage, and live reload picks up a `docs/` directory created afterwards without a restart.
- Requesting a page that exists in the navigation tree but whose markdown source file is missing from storage (e.g. the file was deleted while `rw serve` was still serving a slightly stale snapshot) now returns `404 Not Found` instead of `500 Internal Server Error`.
- An unrecognized `:::directive` nested inside a recognized container (for example a typo'd `:::not` inside a `:::tab` block) no longer closes the *enclosing* container early. Previously the unknown directive's opening delimiter rendered as literal text but was not tracked, so its closing `:::` popped and closed the surrounding tab/note/etc. — truncating the container, leaking the rest of its content out of the panel, and emitting a bogus "stray :::" warning. The renderer now tracks every container nesting level, so an unknown container's delimiters both render literally as text and pair with each other, leaving the surrounding container intact.
- Correctly-closed multi-tab `:::tab` blocks no longer emit spurious "unclosed container directive :::tab" warnings (one per tab beyond the first). The rendered HTML was always correct, but the bogus warnings landed in the page render result's warnings — so `rw serve` logged a "Page render warning: unclosed container directive :::tab" line for every valid multi-tab page when run with verbose logging, and embedders reading `RenderResult.warnings` saw false positives.
- Pages that reference other pages — via `[[wikilinks]]` (link display text and heading-anchor IDs), cross-section links, or C4 diagram entity includes — no longer keep showing stale content after the *referenced* page changes. The rendered-page cache previously keyed only on a page's own source file, so editing page B's title (or its description / section kind / namespace) left page A serving its old cached HTML until A itself was re-saved. The cache key now also incorporates a fingerprint of the cross-page inputs, so such changes re-render the affected pages.
- **(`rw-renderer` Rust API):** a custom directive that expands to more markdown (`DirectiveOutput::Markdown`, e.g. an `::include`-style leaf) used inside an open container directive no longer breaks the surrounding container — previously the recursive expansion drained the container from the active stack, so its closing tag was dropped (producing malformed HTML with a leftover `:::`) and the renderer emitted spurious "unclosed container" / "stray :::" warnings. The container now closes correctly and no false warnings appear. No directive shipped in the `rw` binary returns `DirectiveOutput::Markdown`, so only downstream crates embedding `rw-renderer` with such a directive were affected.
- Inline comments anchored to short or common text (`-`, `,`, `TODO`, single chars) no longer silently migrate to unrelated occurrences when the original passage is edited away — they now appear in the page-comments timeline at the bottom with the original quote block, instead of getting a confident solid-underline highlight on a different character.
- Inline directive syntax (`:name[…]`) inside an inline code span (e.g., `` `:status[On Track]` ``), indented code block, or raw inline HTML is no longer expanded — documentation can now demonstrate `:status[…]` or other inline directives as code.
- Inline directives following a non-directive colon on the same line (e.g. `Note: press :kbd[Ctrl+C]`, `Status: see :status[Done]`, `See https://example.com then run :cmd[deploy]`) are no longer silently dropped — the renderer now skips past punctuation colons, URL schemes, and times and continues scanning for the real directive.
- Documentation pages whose URL begins with `/api/` (e.g. `docs/api/usage.md`) no longer return 404 when opened directly or refreshed in the browser.
- Long URLs and other unbreakable tokens (UUIDs, hash digests, file paths) in tables, paragraphs, and list items now wrap instead of forcing horizontal scrolling on narrow viewports — table cells break anywhere, body text only breaks tokens that would otherwise overflow.
- Directives with non-ASCII characters in their attribute braces (e.g. `:foo[bar]{цвет}`, `{🎉}`, `{اختبار}`) no longer panic the renderer — `rw serve` previously returned 500 and `rw confluence update` / `rw backstage publish` aborted mid-run on these inputs. Valid uses such as `{.заголовок}`, `{#заголовок}`, and `{цвет=зелёный}` were already safe and remain unchanged.
- S3 (and other remote storage) outages no longer turn into "soft outages" where every read serializes on a mutex and re-calls the unreachable backend. When a background reload fails, `rw serve` now keeps serving the stale snapshot via the fast path and only retries on the next explicit signal (file-watcher event, `reload(true)`, or successful `has_changed()` poll).
- Formatted image alt text (`![**Logo**](...)`, `` ![Press `Enter`](...) ``, `![<span>html</span>](...)`) no longer leaks empty inline tags such as `<strong></strong>` or `<code></code>` next to the `<img>`, and inline code inside alt text now contributes its content to the rendered `alt` attribute instead of disappearing — restoring accessibility for screen readers.
- An image inside a heading (e.g. `# ![](icon.png) Project Name`) now renders inside the `<h*>` element instead of escaping before it, fixing document outline, table of contents, and SEO for icon-led section titles.
- YAML frontmatter values that contain `:name[...]`-shaped text (e.g., `description: 'See :status[Done] for details'`) no longer trigger spurious "unknown inline directive" warnings or invoke registered directive handlers. Previously such patterns were scanned for inline directives during text-buffer flushing inside the metadata block, polluting `result.warnings` and firing handler side effects even though the rendered HTML correctly suppressed the directive output.
- A transient panic inside `rw serve` (most realistically inside `storage.scan()` during a reload) no longer permanently bricks the server. Axum's per-request panic catch returned 500 for that one request, but `Site`'s internal `reload_lock` stayed poisoned and every later request panicked too — recovery required restarting the process. Subsequent reads now resume on the previous snapshot and the next reload trigger is honored normally. Internal caches in `FsStorage`, `S3Storage`, and the file-watcher debouncer got the same treatment so a panic in any of them no longer turns into a permanent secondary brick on the recovery path.
- New files created under `docs/` while `rw serve` is running now reliably show up in the navigation sidebar without a manual browser refresh. The live-reload Created handler used to re-check `has_page` *after* invalidating, racing the just-triggered reload — when the post-event scan failed transiently (or hadn't yet picked up the file because of atomic-write timing on slow disks) the check would answer against the pre-event snapshot, drop the broadcast, and the sidebar would stay out of date until the next event arrived or the page was refreshed.
- Heading anchor IDs are now guaranteed unique within a page. Previously a
  heading whose slug coincided with another heading's auto-numbered suffix
  (e.g. headings `Foo`, `Foo 1`, `Foo` all on one page) could emit two elements
  with the same `id`, breaking in-page anchor links and TOC navigation; and a
  heading containing no slug characters (e.g. `## ???`) produced an empty
  `id=""`. Such headings now fall back to a `section` base and always receive a
  distinct numeric suffix.
- `rw serve` no longer keeps serving stale page content after a file change that lands while a previous reload's storage scan is still running. The lazy-reload cache tracked validity in a separate flag that a finishing scan could set back to "valid", clobbering the concurrent change signal; the change was then lost until an unrelated later edit. Validity is now derived from a monotonic generation stamp, so a change can never be swallowed by an in-flight reload.

## [0.1.24] - 2026-04-10

### Fixed

- Sites using `README.md` as homepage no longer return 500 errors when published to Backstage via S3 — `FsStorage` now auto-detects `README.md` in the parent of `source_dir`, so all code paths (serve, publish, napi) get it automatically
- Diagrams with decimal SVG dimensions from Kroki (e.g., Mermaid sequence diagrams) now scale correctly instead of rendering at full size and getting squished by the container

## [0.1.23] - 2026-04-09

### Added

- `pages` field in directory-level `meta.yaml` or `index.md` frontmatter to control navigation sidebar order — listed pages appear first in declared order, unlisted pages follow alphabetically

### Changed

- **Breaking:** `getNavigation()` on `RwSite` is now async (returns a Promise) — previously it blocked the Node.js event loop during S3 operations on cold cache or reload
- S3-backed `RwSite` instances now share a single tokio runtime instead of each creating its own thread pool, reducing resource usage when serving multiple documentation entities

### Fixed

- Relative links from README.md homepage (e.g., `[Guide](docs/guide.md)`) now resolve correctly to `/guide` instead of the non-existent `/docs/guide`

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
