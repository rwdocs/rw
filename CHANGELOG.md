# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.32] - 2026-07-12

### Added

- `@rwdocs/core`'s `RwSite.getPageMarkdown()` returns a page's Markdown source as authored, so a host (e.g. an MCP `read-page` tool feeding an AI agent) can read a page as Markdown instead of converting the rendered HTML back with turndown. Returns `null` for a virtual directory page.
- `@rwdocs/core`'s `RwSite.pagePathFor(sectionRef, subpath)` maps a page's canonical identity — the `(sectionRef, subpath)` pair `listPages()` and `PageMeta` hand out — to the path `renderPage`/`renderSearchDocument`/`getPageMarkdown` take, so a host holding an identity (a search hit, a comment) can read the page without re-deriving section scopes itself. Returns `null` when no section has that ref.

## [0.1.31] - 2026-07-10

### Fixed

- `rw backstage publish` now reports correct git-commit modification times for sites that have no `docs/` directory (e.g. a `README.md` homepage). Git repository discovery started from the (non-existent) source directory and failed silently, so every published page fell back to the filesystem checkout time — which in CI equals the latest commit's date, making unchanged pages look freshly modified. Discovery now climbs to the nearest existing directory, and `rw` warns when git modification times are requested but no repository is found.

## [0.1.30] - 2026-07-10

### Added

- `mtimeSource: "filesystem" | "git"` option on `@rwdocs/core`'s `createSite()` (default `"filesystem"`) — `"git"` reports commit-based times matching S3-served pages; `"filesystem"` uses a fast `stat`.
- `@rwdocs/core`'s `renderPage`/`getNavigation` (and the matching `rw serve` responses) now return a `sectionAncestry` map — each connected section's ref mapped to its nearest-first ancestry chain — so a host resolves a page's full section context in one call. Purely additive.
- `@rwdocs/core`'s `RwSite.listPages()` entries now carry a `lastModified` RFC-3339 timestamp, so a host can sort pages by recency without rendering each one. Legacy S3 manifests report the Unix epoch until republished.
- `rw update` self-updates the installed binary from the latest GitHub release (`--check`, `--version <x.y.z>`, `--prerelease`). Only shell/PowerShell installs self-update; Homebrew, npm, `cargo`, and source builds print upgrade guidance instead.

### Changed

- **Breaking (pre-1.0):** `@rwdocs/core`'s `renderPage`/`getNavigation` responses (and the viewer) replace the breadcrumb `section` object with flat `sectionRef`/`subpath` fields.
- `rw serve` now reports page `lastModified` from the filesystem mtime instead of the git commit time, dropping a per-request git walk. Published bundles and S3-served pages keep git-commit times.
- The `rw serve` page `ETag` now covers the whole response, not just the HTML, so a page revalidates when its section identity or ancestry changes. `If-None-Match` still returns `304` when nothing changed.

### Fixed

- Internal links no longer build doubled or wrong paths when the viewer is embedded (e.g. in Backstage): a link pointing above the current entity's scope now resolves against the nearest host-mapped ancestor instead of concatenating onto the current base. Consequently the top "Home" breadcrumb in a nested sub-entity navigates to the root entity's docs.

## [0.1.29] - 2026-07-07

### Added

- `rw serve --open` (short `-o`) opens the site in your default browser once the
  server is ready. It opens the actual bound port (so it's correct even after a
  port fallback) and uses `localhost` when listening on `0.0.0.0`. Off by
  default; if the browser can't be launched, `rw serve` warns and keeps serving.
- `rw serve` now falls back to the next free port when its default (`7979`) is
  busy — it tries `7980`, `7981`, … and prints the port it settled on — so a
  second `rw serve` just works. A port set explicitly (`--port` or
  `[server].port` in `rw.toml`) stays a hard requirement and still fails with
  "port N is already in use".
- Diagram code fences accept a `{#id}` attribute block to set a stable
  `data-diagram-id` on the rendered `<figure class="diagram">`
  (e.g. ` ```mermaid {#architecture} `). Diagrams without one get an auto
  `data-diagram-id="diagram-<n>"`. An explicit id survives diagram reordering;
  auto ids do not. `format` is also set inside the block (`{format=png}`).
- Diagrams can now be opened in a fullscreen zoom popup. Every rendered diagram
  shows an "Expand diagram" button on hover (always visible on touch) that opens
  a modal where you can zoom (wheel, pinch, or on-screen controls) and drag to
  pan, making large diagrams legible. It opens at natural size, scaled down only
  to fit the screen, and keeps its proportions; Escape or the close button
  dismisses it. While it's open, editing the diagram's source re-renders it live
  on the next reload without losing your zoom or pan; a momentarily-broken edit
  keeps the last good render until you fix it.

### Changed

- Fenced code blocks now match the page theme instead of always rendering as a
  dark box: a faint light-tinted surface in light theme, a subtly darker panel in
  dark theme. Inline `code` and syntax highlighting are unchanged.
- **Breaking (pre-1.0):** the bare `format=png` diagram fence form (outside the
  braces) is removed. Set the format inside the attribute block: ` ```mermaid
{format=png} `.
- The embedded-preview shell (`rw serve --embedded`, a Backstage-like host
  wrapper for testing embedded rendering) is now compiled into every build,
  instead of needing the `embedded-preview` Cargo feature. The flag stays hidden
  from `rw --help`.

### Fixed

- Inline comments now re-anchor to their passage in more cases after it's edited,
  instead of dropping to the page timeline. The fuzzy re-anchor now uses a Myers
  bit-vector matcher with no length limit (previously quotes over 32 characters
  couldn't be matched at all) and ranks candidates by edit distance and
  surrounding context, so edited quotes and moderate word substitutions
  re-highlight (with the dashed "re-anchored" underline). Passages that are
  genuinely gone or too ambiguous still drop to the timeline.
- An inline comment on a short passage (e.g. a section heading) no longer drops
  to the page timeline when the text *next to* it is edited — for instance
  inserting a paragraph between a heading and its list. As long as the passage is
  still unique on the page and one side of its context still matches, the comment
  stays anchored; previously a short passage needed both sides unchanged.
- Mermaid diagrams no longer render as solid black shapes with unreadable labels
  in the fullscreen zoom popup. Mermaid scopes its embedded stylesheet under the
  SVG's root id; the popup renames the clone's ids but left the stylesheet
  selectors behind, orphaning every rule. Selectors and
  `aria-labelledby`/`aria-describedby` references are now renamed in lockstep.
  PlantUML/C4 diagrams were never affected.
- Inline comments no longer attach to rendered diagrams. A diagram is inlined as
  an SVG whose labels are real text, so selecting one used to offer "Add comment"
  and create a comment that couldn't be reliably re-found. Selecting text inside
  a diagram (or across one) now shows no "Add comment" button, and a prose
  comment can no longer jump onto a same-worded diagram label on re-render. On
  the CLI, `rw comment add --quote` for text that appears only inside a diagram
  now reports the quote as not found.
- Live-reloading the homepage (editing `docs/index.md` or the `README.md`
  homepage) no longer sometimes jumps your scroll position to the top. It
  refreshed non-silently, so a reload slower than ~300ms briefly showed the
  loading skeleton and collapsed the page height; the homepage now refreshes
  silently like every other page.
- Comment keyboard navigation (`n`/`p`/`r`) now works on non-Latin keyboard
  layouts (Cyrillic, Greek, and similar). The shortcuts matched the typed
  character, so on a Russian layout the physical keys produced letters that never
  matched; they now fall back to the physical key position while still honoring
  the labeled key on Dvorak/AZERTY. As a side effect they're now case-insensitive.
- Resolving an inline comment no longer makes the thread's position counter jump
  to the end (e.g. "1 / 6" → "6 / 6"). The resolved comment stays in its slot with
  its passage highlighted; the counter updates (to "1 / 5") only when you step to
  the next comment.
- `rw serve` no longer keeps showing broken diagrams from cache after you fix a
  Kroki problem. A change to `kroki_url`, `dpi`, or the PlantUML include
  directories now invalidates cached pages, and a page whose diagrams failed to
  reach Kroki (network error or 5xx) isn't cached at all, so it recovers on the
  next request. A genuinely broken diagram (Kroki returns 400) still caches, so it
  doesn't re-hit Kroki on every request.

## [0.1.28] - 2026-06-24

### Added

- `@rwdocs/core` exposes `RwSite.listSections()`, which returns every documentation section in one call — flat, each with its canonical ref (`kind:namespace/name`), scope path, and full nearest-first ancestry (root last) — so a host no longer needs N+1 `getNavigation()` calls to walk nested sections (which deliberately hide sub-sections as childless leaves).
- `@rwdocs/core` exposes `RwSite.listPages()`, which enumerates every page in a site in one pass — each with its title and its `(sectionRef, subpath)` key (the same pair comments use as a page's `document_id`) — so a host can cache human-readable page titles (e.g. for a comment inbox) without an N+1 of per-page `renderPage()` calls. The site's root page and virtual directory pages are included.

### Changed

- Opening an inline-comment deep link (`#comment-<id>`), or stepping to an inline comment with `n`/`p`, now lands the highlighted passage about a third of the way down the viewport instead of dead-center — so the passage sits where the eye rests on arrival, with room above for the comment thread.

### Fixed

- Relative `.md` links from a leaf page (e.g. `[sibling](./other.md)` in `docs/specs/notif.md`) now resolve to the sibling page (`/specs/other`) instead of a non-existent path nested under the current page (`/specs/notif/other`). Links now follow standard CommonMark semantics — resolved relative to the source file's directory — for both leaf pages and `index.md` directory pages. Links from README/`index.md` homepages (including the `docs/` source-dir prefix case) are unchanged.
- Opening an inline-comment deep link (`#comment-<id>`) no longer leaves the comment thread pinned in the wrong vertical position. The thread in the right-margin column (and the narrow-screen comment popover) could land hundreds of pixels above its highlighted passage and stay there when content above the passage reflowed _after_ the thread was positioned — e.g. a web-font swap on first load, or a late-loading image or diagram. Threads now re-align whenever their highlighted passage moves, not only when the article is resized, so they track the highlight through any late layout shift. A normal click was never affected (it happens after the page has settled).
- The "Add comment" popover now appears when you select a line's first word by dragging right-to-left and release the mouse to the left of the article — and likewise for any text selection released outside the article body (past its right edge or below it). Previously the popover only showed when the mouse was released inside the article, so a right-to-left first-word selection silently produced nothing.

## [0.1.27] - 2026-06-22

### Added

- Inline-comment threads are now reachable on narrow windows and phones. Below the width where the right-margin comment column is hidden, tapping a highlighted passage opens its thread in a popover anchored to the highlight (with replies, resolve, and delete), and selecting text → "Add comment" opens the draft box there too — previously both silently did nothing because the only thread surface was the hidden margin column. Escape or tapping away dismisses it; `n`/`p` navigation and tapping another highlight move the popover to that comment.
- Press `r` while a comment thread is active (highlighted with `n`/`p`) to move keyboard focus straight into that thread's reply box — no mouse needed. Works for both the inline margin thread and page-timeline/orphaned threads, scrolls the reply box into view on long threads, and announces the move to screen readers. The existing reply shortcuts still apply: Cmd/Ctrl+Enter submits and Escape releases the box so `n`/`p` resume. `r` is ignored while you're already typing, when no thread is active, or on a resolved thread.

### Changed

- The `rw comment` CLI now stamps comments it creates with the AI identity (`{ id: "local:ai", name: "AI" }`, a sparkles avatar in the viewer) by default, instead of the human `{ id: "local:human", name: "You" }`. The CLI's primary user is an LLM agent, so unattributed agent comments are now visually distinct from a human reviewer's own comments in the browser. Set `RW_COMMENT_AUTHOR_ID`/`RW_COMMENT_AUTHOR_NAME` (or `--author-id`/`--author-name`) to override. Browser-authored comments via `rw serve` are unchanged (still `local:human`).
- The "Add comment" button that appears when you select text in a doc is now icon-only (a speech-bubble icon) instead of icon + "Add comment" text, for a more compact popover. The button keeps its "Add comment" accessible name, so screen readers and keyboard users are unaffected.

### Fixed

- Comment highlighting no longer re-walks and re-wraps the entire article on every comment action (resolve, reply, reopen) or background live-refresh — only the changed highlight is updated. On long pages with many comments this removes a visible hitch, and a background refresh no longer wipes an in-progress text selection unless the change overlaps the selected text.
- Replying to a comment thread (or posting a new comment) no longer traps keyboard focus in the composer. After you submit, focus moves to the thread you just acted on; pressing Escape releases the composer field. Either way, `n`/`p` comment navigation works again without reaching for the mouse.
- Comment deep-linking now works when the viewer is embedded (e.g. in Backstage), matching standalone mode: every comment thread shows a "Copy link" button, stepping through comments with `n`/`p` (and opening a thread) writes a shareable `#comment-<id>` to the host page's URL, and Back/Forward or a manual hash edit re-focuses the linked comment. Previously these were silently disabled in embedded mode. Path-based host routing is unchanged — only the URL hash is touched.
- A transient failure during a live-reload background refresh no longer blanks the page. A silent refresh that fails (server restart mid-edit, a flaky proxy, a momentarily-unreachable host) now keeps the last-rendered page on screen and recovers on the next successful reload, instead of replacing it with a full-height error.
- A transient failure during a live comment refresh no longer wipes the rendered comments or pops an error toast the user never triggered. Silent comment refreshes now keep the current comments quietly and recover on the next successful reload.
- A documentation page or the navigation sidebar no longer briefly shows a transient error (or blanks) when you navigate quickly between pages and a now-superseded request fails with a non-abort error. The superseded request's failure is dropped instead of overwriting the page you actually landed on.
- The mobile navigation drawer is now a proper modal dialog for assistive tech and keyboard users: it exposes `role="dialog"`/`aria-modal`, moves focus into the drawer on open and restores it to the menu button on close, traps Tab within the drawer, and marks the page behind it `inert` so screen readers no longer read the obscured content.
- Stepping through comments with `n`/`p` is now re-announced to screen readers even when the move lands on the same position — e.g. wrapping around on a page with a single open comment, where the position text is identical each press. Previously the polite live region's text was unchanged, so NVDA/JAWS/VoiceOver stayed silent despite the move.
- A reply draft typed into one comment thread no longer appears in every other thread. Drafts are now scoped to the thread they were written in: switching threads (with `n`/`p` or the prev/next buttons) shows each thread's own draft, an untouched thread stays empty, and returning to a thread restores the draft you left there. Drafts clear on submit and when you navigate to another page.
- Comments from the previous page no longer briefly reappear after navigating to a page that shows none (e.g. Home): a now-superseded comment fetch that resolves after you've navigated away is dropped instead of repopulating the just-cleared list. Resolving or reopening a comment while a navigation is in flight likewise no longer writes the change into the next page's comment view.
- Accessibility: the active navigation link and the active "On this page" outline entry now expose `aria-current`, so screen readers announce which page and heading you're on (previously conveyed by color alone); copying a comment's share link is announced via a polite live region rather than only swapping the button icon; and the desktop navigation landmark now carries an accessible name ("Documentation"), distinct from the breadcrumb, table-of-contents, and mobile-navigation landmarks.

## [0.1.26] - 2026-06-21

### Added

- Embedding hosts can supply their own comment client via `mountRw({ comments })` — an injected `CommentApiClient` (`list`/`create`/`update`/`delete`, plus optional `subscribe` for live refresh). When present, comments are enabled implicitly (no `/config` round-trip) and fully decoupled from the docs `apiBaseUrl`, so a host can serve comments at any URL shape and transport. When omitted, the viewer builds its existing HTTP client against `{apiBaseUrl}/comments` and reads `commentsEnabled` from `/config` — unchanged. `CommentApiClient`, `Comment`, `CreateCommentRequest`, and `UpdateCommentRequest` are now exported from `@rwdocs/viewer`.
- Embedding hosts can route transient notifications (e.g. a failed comment save) through their own toast/alert system by passing `mountRw({ onNotify })`. Standalone `rw serve` falls back to a built-in toaster.
- Comments carry a `canResolve` capability flag (alongside `canDelete`/`canRestore`) so a host's permission model decides whether the Resolve/Reopen affordance appears. `rw serve` sets it true for top-level comments and false for replies, matching current behavior.
- `PageMeta` now exposes a section-relative `subpath` field (HTTP page API, `@rwdocs/core`, and `@rwdocs/viewer` types) alongside `sectionRef`. Embedding hosts that store their own comments can key them on the stable `(sectionRef, subpath)` pair instead of the URL `path`. See [Embedding](docs/embedding.md).

### Changed

- **Breaking:** `rw serve` and the `rw comment` CLI now both key inline and page comments on the stable `(sectionRef, subpath)` pair instead of the page's URL path, so CLI-created and browser-created comments land on the same key. Relocating or remounting a whole section (its `sectionRef` unchanged) no longer orphans the comments on its pages; a section _rename_ or moving a single page _within_ a section still changes the key for the affected comments. The `--document` flag on `rw comment add` / `rw comment list` still accepts the URL path and resolves it to the composite key internally. It now also accepts the markdown source file path, with or without the docs-root prefix (e.g. `docs/guide.md` or `guide.md`), mapping it to the page's URL path the same way the server does — so a script or agent can pass the file it just edited directly. Comments created in 0.1.25 (keyed by the old path) are not migrated: they remain in the database but are no longer queried, so they effectively disappear from the UI and CLI — negligible impact given 0.1.25's age.

### Fixed

- When a site's root page (`docs/index.md` or the `README.md` homepage) declares a section `kind` in its metadata, the navigation API now reports that kind for the root scope (and for the back-navigation parent of top-level sections) instead of the generic `section` kind, so a host mapping sections to catalog entities by kind now resolves the root consistently. Previously the page API's `section_ref` and the navigation API disagreed about the root's identity.
- Wide tables no longer collapse their short-content columns to a sliver (a single word breaking one or two characters per line). Columns now take their natural widths, and a table that is genuinely wider than the page scrolls horizontally inside its own box — keyboard-focusable and announced to screen readers — instead of being clipped.
- A comment whose save failed (e.g. `rw serve` was down or unreachable) no longer loses the text you typed. The composer keeps your draft, its button changes to **Retry**, and a toast explains the save failed and that your draft is kept — instead of silently clearing the box.
- `n`/`p` comment navigation no longer gets stuck on orphaned comments (inline comments whose anchored text was later edited away); they now highlight when selected, and navigation continues to the next comment.
- `n`/`p` comment navigation no longer centers page-comment threads with many replies in a way that pushed the first comment off the top of the screen. Long page-comment threads now scroll so their first comment is visible, matching how the shareable `#comment-<id>` deep links already behaved.
- Markdown blockquotes are restyled for readability — upright (no italics), normal-weight body text with no decorative quotation marks, set off by the left border alone. Long multi-line quotes are much easier to read.

## [0.1.25] - 2026-06-19

### Added

- **Inline & page comments** — review documentation directly in the browser: select text to anchor a comment to a passage, or comment on the page as a whole. Threads support replies, resolve/reopen, and a "Show resolved" disclosure with a count badge. Comments persist in `.rw/comments/sqlite.db` (versioned schema with idempotent forward migrations) and survive re-renders via multi-selector anchoring (TextQuoteSelector + TextPositionSelector); when the original passage changes, the viewer falls back to fuzzy re-anchoring (diff-match-patch) and marks the comment "re-anchored" with a dashed underline. Anchors that would land on short or ambiguous text drop to the page timeline with their original quote instead of jumping to an unrelated occurrence, and highlights stack progressively darker where comments overlap.
  - **Authorship** — every comment carries an author (`{ id, name, avatarUrl? }`); `local:human` renders a person avatar, `local:ai` a sparkles avatar (recommended for LLM agents), others fall back to name initials. `rw serve` stamps browser-created comments as "You".
  - **Markdown bodies** — comment bodies render a safe, restricted CommonMark+GFM subset (paragraphs, **bold**/_italic_/~~strikethrough~~, lists, blockquotes, inline and fenced code, `http`/`https`/`mailto` links). Raw HTML, images, tables, headings, and unsafe link schemes are neutralized; the rendered HTML ships in an additive `bodyHtml` field alongside the raw `body`.
  - **Deep links** — every thread has a shareable `#comment-<id>` URL with a "Copy link" button; opening one scrolls to and reveals the thread (auto-expanding the resolved disclosure when needed). Inbound deep-linking works when embedded; the copy button is hidden there since the host owns the URL.
  - **Keyboard navigation** — press `n`/`p` to step between comments in document order, wrapping at the ends; each jump scrolls into view, opens inline threads in the margin, and is announced to screen readers. Keys are ignored while typing, and modifier combos pass through to the browser.
  - **Live refresh** — comments added, edited, or resolved by the `rw comment` CLI or another browser tab appear without a manual reload, via a best-effort token-authenticated notify (using `.rw/server.json`) re-broadcast over the existing live-reload WebSocket. In-progress drafts are preserved, and the CLI stays decoupled — if no server is running, the comment is still written.
  - **Delete replies** — replies can be soft-deleted (kept visible, muted and struck-through, with a Restore button so a misclick is reversible in-session); top-level comments use Resolve instead. Deletes are non-destructive (`deletedAt` on the row) and hidden on the next refetch.
  - **REST API & CLI** — `/_api/comments` (create, list, update, and `DELETE /_api/comments/{id}` with a `deletedAt` field plus `canDelete`/`canRestore` flags) and an `rw comment` CLI (list, show, add, reply, resolve) for scripting and LLM agents. The CLI reads and writes the SQLite store directly whether or not `rw serve` is running, takes identity from `RW_COMMENT_AUTHOR_ID`/`RW_COMMENT_AUTHOR_NAME` or `--author-*` flags, and anchors inline comments with `--quote` (rejecting ambiguous or missing matches).
  - `@rwdocs/core` exports `renderCommentBody(markdown)` so hosts that store their own comments (e.g. a Backstage backend plugin) can render bodies to the same safe `bodyHtml`. Returns a `Promise<string>`.
- Status badges — `:status[Label]{color=NAME}` renders an inline colored pill label (`grey`, `red`, `yellow`, `green`, `blue`, `purple`). Publishing to Confluence emits the native `status` macro, so badges stay editable and on-style. Color is case-insensitive and optional; unknown or omitted colors fall back to `grey`.
- Custom section namespaces — sections can declare a `namespace` in `meta.yaml` or frontmatter (e.g. `namespace: payments`), producing section refs of the form `kind:namespace/name` that map to Backstage catalog entities outside the `default` namespace. The field inherits down the directory tree and a subtree can override it; invalid values fail the site load with an error naming the offending file. Wikilinks that omit the namespace resolve within the current page's namespace.
- Named metadata sidecar files — place `<name>.meta.yaml` (e.g. `payments.meta.yaml`) directly in a directory to declare a page or content-less catalog entity at that path, instead of creating a `<name>/meta.yaml` subfolder. Works as a sidecar for an existing `<name>.md` or stand-alone to register Backstage components/systems that exist only to build relations. The suffix follows the configured metadata filename. Named sidecars are leaf-only (no `vars` cascade), and a directory `meta.yaml` wins if both resolve to the same page.
- `rw confluence render <markdown_file> --out <dir|->` — renders markdown to a Confluence-publishable bundle (`page.xhtml` plus one PNG per diagram). Stdin optionally accepts the current page's storage XHTML body to preserve inline-comment markers; without it, the command renders as a fresh page. `--out -` writes the body XHTML to stdout (erroring with exit 3 only if the render produced PNG attachments); `--strict` exits non-zero on any warning or unmatched comment.
- `rw backstage publish` now surfaces diagram-processing warnings — broken or cyclic PlantUML `!include` paths — in yellow on stderr instead of silently discarding them. Pass `--strict` to fail the publish when any warning was emitted; bundles still upload either way, so warnings can be fixed in a follow-up commit and republished.
- `RW_DIAGRAMS_KROKI_URL` environment variable — supplies `diagrams.kroki_url` for projects without an `rw.toml` (or one that omits the field). Precedence is CLI flag > `rw.toml` > env var, so explicit project config still wins. Lets teams roll `rw` out across many repos that share a single Kroki server by exporting the variable once.
- `rw serve` writes a `.rw/server.json` runtime-info file on startup (host, port, pid, version, start time, and a reserved secret token) and removes it on graceful shutdown — including on SIGTERM (`docker stop`, systemd), not just Ctrl-C. The file is written atomically with `0600` permissions in the gitignored `.rw/` directory, so the token never lands in version control. It lets other tooling discover a running server for the project.

### Removed

- **Breaking:** `rw confluence update` and `rw confluence generate-tokens` are removed. `rw` no longer talks to the Confluence REST API; the `[confluence]` section in `rw.toml` is no longer recognized (stale sections are silently ignored, not rejected). Use `rw confluence render <md> --out <dir>` to produce a publish-ready bundle (XHTML body + diagram PNGs), then publish it with a tool of your choice. Comment preservation continues to work: pipe the current page's storage XHTML body into stdin and `rw` carries `<ac:inline-comment-marker>` tags through to the new XHTML.

### Changed

- **Breaking:** the HTTP API served by `rw serve` moved from `/api/*` to the reserved `/_api/*` prefix (e.g. `/_api/navigation`, `/_api/pages/...`, `/_api/comments`), freeing the `/api/*` URL space for documentation pages. The bundled viewer moves in lockstep; only external callers hitting `rw serve`'s HTTP endpoints directly need to update.
- **Breaking:** heading anchor IDs for headings containing `[[wikilink]]` syntax now include the wikilink's resolved display text — `## See [[overview]]` (resolver returning "Overview") now produces `<h2 id="see-overview">` instead of `<h2 id="see">`. The TOC entry title changes correspondingly. In-page anchor links targeting the old slugs need updating.
- Block directives (`:::tab`/`:::note` containers and `::leaf` directives) are now parsed with awareness of markdown structure: they must be blank-line separated (each delimiter on its own paragraph), delimiters inside code or fenced blocks stay literal so directive syntax can be shown as an example, and directives now work inside blockquotes and loose list items. Standard blank-line-separated `:::tab` blocks are unaffected; any that relied on the old no-blank-line form now render as literal text until separated.
- Single-page sites (only `index.md`, or a `README.md` homepage) no longer show an empty navigation sidebar — the desktop sidebar, mobile hamburger, and mobile drawer are all hidden, leaving a clean centered article. Sites with a "back to home" link keep their sidebar.
- Page modification times (`lastModified`) now reflect the git commit time instead of the filesystem mtime, so timestamps stay stable across `git checkout`, `pull`, and branch switching. S3-published bundles now carry these times in the manifest too (previously always epoch zero for Backstage-served pages).
- Page outline (TOC) sidebar widened from 240px to 320px so opening a comment no longer narrows the article; it now appears at viewport widths ≥ 1304px (was ≥ 1224px), with the floating "On this page" popover below that.
- `@rwdocs/viewer` now requires Node.js `>=22.12.0` (was `>=20`) — Vite 8 needs Node 20.19+/22.12+, and Node 20 reached end-of-life on 2026-04-30; `22.12.0` is the first release where `require(esm)` works without a flag.
- Minimum supported Rust version raised to 1.96 — building `rw` from source now needs a 1.96+ toolchain.

### Fixed

- `rw serve` no longer fails to start when a project has a `README.md` but no `docs/` directory; the README is served as the homepage and live reload picks up a `docs/` directory created afterwards without a restart.
- Requesting a page that exists in the navigation tree but whose markdown source is missing from storage now returns `404 Not Found` instead of `500 Internal Server Error`.
- Documentation pages whose URL begins with `/api/` (e.g. `docs/api/usage.md`) no longer return 404 when opened directly or refreshed.
- Pages that reference other pages — via `[[wikilinks]]`, cross-section links, or C4 diagram entity includes — no longer keep showing stale content after the _referenced_ page changes. The rendered-page cache key now incorporates a fingerprint of cross-page inputs.
- A transient panic inside `rw serve` (most realistically inside `storage.scan()` during a reload) no longer permanently bricks the server by poisoning the internal reload lock. Reads resume on the previous snapshot and the next reload trigger is honored; the storage layers and file-watcher debouncer got the same hardening.
- `rw serve` no longer keeps serving stale page content after a file change that lands while a previous reload's storage scan is still running — validity is now derived from a monotonic generation stamp, so a change can't be swallowed by an in-flight reload.
- New files created under `docs/` while `rw serve` is running now reliably appear in the navigation sidebar without a manual refresh (the live-reload Created handler no longer races its own invalidation).
- S3 (and other remote storage) outages no longer become "soft outages" where every read serializes on a mutex and re-calls the unreachable backend; a failed background reload keeps serving the stale snapshot and retries only on the next explicit signal.
- Heading anchor IDs are now guaranteed unique within a page (previously a slug colliding with another heading's auto-numbered suffix could emit duplicate `id`s, and a heading with no slug characters produced an empty `id=""`).
- An image inside a heading (e.g. `# ![](icon.png) Project Name`) now renders inside the `<h*>` element instead of escaping before it, fixing the document outline, TOC, and SEO for icon-led titles.
- Formatted image alt text (`![**Logo**](...)`, ``![Press `Enter`](...)``) no longer leaks empty inline tags next to the `<img>`, and inline code inside alt text now contributes to the rendered `alt` attribute instead of disappearing.
- Long URLs and other unbreakable tokens (UUIDs, hash digests, file paths) in tables, paragraphs, and list items now wrap instead of forcing horizontal scrolling on narrow viewports.
- Directives with non-ASCII characters in their attribute braces (e.g. `:foo[bar]{цвет}`, `{🎉}`) no longer panic the renderer; valid uses such as `{.заголовок}`, `{#заголовок}`, and `{цвет=зелёный}` were already safe and remain unchanged.
- Inline directive syntax (`:name[…]`) inside an inline code span, indented code block, or raw inline HTML is no longer expanded, so documentation can demonstrate `:status[…]` and other directives as code.
- Inline directives following a non-directive colon on the same line (e.g. `Note: press :kbd[Ctrl+C]`, `See https://example.com then run :cmd[deploy]`) are no longer silently dropped — the renderer skips past punctuation colons, URL schemes, and times and continues scanning for the real directive.
- YAML frontmatter values containing `:name[...]`-shaped text (e.g. `description: 'See :status[Done] for details'`) no longer trigger spurious "unknown inline directive" warnings or invoke directive handlers.

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
