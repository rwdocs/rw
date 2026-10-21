# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `@rwdocs/core` exposes `__napiBindingTarget` to identify the loaded backend (`"native"` for the distributed binaries).
- `@rwdocs/core`'s `renderPage()`, `renderSearchDocument()` and `getPageMarkdown()` also take a path with one leading slash, such as a page response's `meta.path` (`/guide`, or `/` for the home page).
- `rw comment` subcommands and `rw confluence render` take `--project-dir <dir>`, and `rw comment` subcommands take `-c, --config <path>`, as `rw serve` and `rw backstage publish` do. See [Configuration](docs/configuration.md).
- Ambiguous C4 metadata-include lookups now log a WARN when the same kind/name exists across namespaces, listing matching refs and document paths. Use distinct explicit names. Warnings follow logging filters and actual lookups (including search), so cached HTML stays quiet. See [ambiguous include names](docs/metadata.md#ambiguous-include-names).
- **Page-local attrs for integrations** — JSON-compatible `attrs` in frontmatter and selected YAML sidecars survive S3 publication and appear as optional `meta.attrs` in NAPI/core `renderPage()` responses. Frontmatter overlays top-level keys; nested values replace whole, null remains data, and attrs never inherit. Empty attrs are omitted. Authored nesting is bounded for manifest/cache transport; Rust JSON byte roundtrips preserve supported finite `f64` values. No built-in HTTP/viewer or rendering semantics change. See [Page Metadata](docs/metadata.md#attrs-page-local-integration-data).
- Tabs and other `:::` directives work without blank lines around their `:::` lines, the compact form remark-directive and Docusaurus use. See [Tabs](docs/tabs.md).
- rw warns when a directive line renders as text: one that needs a blank line (after a list, quote, table or HTML block, in a tight list item, or above a `---` heading underline), one with text after its name, or one with markup in its label. See [Tabs](docs/tabs.md#mistakes-rw-warns-about).
- Unknown directive names, such as Docusaurus's `:::note` or `::youtube[id]`, now warn once per page, so `rw confluence render --strict` catches them.
- An image naming a diagram file alone in its paragraph, such as `![Payment flow](diagrams/payment.puml)`, renders as that diagram in `rw serve`, `@rwdocs/core`, `rw confluence render` and Backstage, with its alt text as the diagram's. PlantUML, Mermaid, Graphviz, ditaa, erd (`.er`), Vega and Vega-Lite (`.vg.json`, `.vl.json`) files work everywhere, svgbob and nomnoml files everywhere but `rw confluence render`, and `rw serve` refreshes open pages when one changes. See [Diagrams from files](docs/diagrams.md#diagrams-from-files).
- `rw serve` shows images stored in the project, such as `![Flow](img/flow.png)` or `![Logo](/.github/assets/logo.svg)`, with paths resolved as on GitHub. Backstage doesn't show them yet. See [Images](docs/images.md).
- `@rwdocs/core` can serve those images: `getFile(path)` returns a project file's bytes, content type and ETag. `@rwdocs/viewer` loads them from `{apiBaseUrl}/files/{path}`, a route the host serves with `getFile`. See [Embedding](docs/embedding.md#serve-page-images-with-getfile).
- Page responses from `rw serve` and `@rwdocs/core`'s `renderPage()` carry `meta.documentId`, the key the page's comments are stored under. `@rwdocs/viewer` now reads it, so upgrade `@rwdocs/core` along with the viewer. See [Embedding](docs/embedding.md#key-on-sectionref-subpath-instead).
- Diagram fences take alt text, such as ```` ```plantuml {alt="Payment flow"} ````, which screen readers announce and `rw confluence render` publishes as the image's alt text. Fence attribute values in quotes may now contain spaces. See [Diagrams](docs/diagrams.md#attributes).

### Changed

- An unknown inline directive such as `:foo[x]` now warns once per page instead of once per use.
- Attributes in a directive's braces, such as `:status[Done]{color=green}`, now follow the quoting rules of diagram fence attributes. See [Diagrams](docs/diagrams.md#attributes).
- **Breaking (pre-1.0):** `rw serve`, `rw backstage publish`, and `rw comment` now fail (exit 3) when the project root has neither a docs directory nor a `README.md` homepage, as in a subdirectory of a project without `rw.toml`; so does `rw confluence render` unless you name the root. Before, `rw serve` served an empty site there and `rw comment list` found nothing. Run from the project root or pass `--project-dir`. See [Configuration](docs/configuration.md).
- **Breaking (pre-1.0):** `@rwdocs/core`'s `createSite({ projectDir })` no longer reads `RW_DIAGRAMS_KROKI_URL`, matching `createSite({ s3 })`. To render diagrams, pass `diagrams: { krokiUrl }` or set `[diagrams] kroki_url` in the project's `rw.toml`.
- **Breaking (pre-1.0):** `@rwdocs/core` types `createSite()`'s `mtimeSource` as `"filesystem" | "git"`, so TypeScript rejects a plain `string`. `createSite()` now throws for any other value with an `s3` site too, which used to ignore it.
- **Breaking (pre-1.0):** a `--project-dir` directory or `--config` file that does not exist now exits 4, not 1.
- **Breaking (pre-1.0):** rw, and `@rwdocs/core`'s `createSite({ projectDir })`, refuse an `rw.toml` whose `[diagrams] include_dirs` names a directory outside the project, such as `"/"` or `"../shared"`, symlinks resolved. To share includes, move `rw.toml` up and set `[docs] source_dir`, or keep one config per site at the root and pass `--config`. `rw confluence render -I` still takes any directory. See [Diagrams](docs/diagrams.md#configuration).
- **Breaking (pre-1.0):** rw, and `@rwdocs/core`'s `createSite({ projectDir })`, refuse a `[docs] source_dir` outside the project, symlinks resolved, such as `"../docs"` or a `docs` folder symlinked elsewhere. Move `rw.toml` up to a folder that holds the docs and set `source_dir`. See [Configuration](docs/configuration.md).
- **Breaking (pre-1.0):** PlantUML `!include` now inlines only PlantUML files (`.puml`, `.iuml`, `.plantuml`, `.pu`, `.wsd`), from include directories too, so a `.env` or `.git/config` never reaches Kroki; rw warns about any other file instead. Rename such an include to a PlantUML extension. See [PlantUML Includes](docs/diagrams.md#plantuml-includes).
- Building from source now requires Rust 1.99.1 or newer (previously 1.97).
- A `:::` opener with text after its name or label, such as `:::tab macOS` or `:::note Title`, is now text. Before, rw opened the container and dropped that text. Put the label in brackets: `:::tab[macOS]`.
- **Breaking (pre-1.0, Rust source):** Public `rw_meta::Meta` literals must add `attrs: Default::default()` (or use `Meta::resolve`). S3 manifests remain version 1; upgrade readers before integrations rely on attrs, since older readers ignore and lose them on reserialization. Populated attrs increase manifest/cache payloads and selected-page conversion work. JavaScript numeric identifiers requiring exact large integers should be strings.
- **Breaking (pre-1.0):** `@rwdocs/viewer` now declares Node `^22.22.2 || ^24.15.0 || >=26.0.0`, excluding Node 25 to match the Vitest 5 development toolchain. npm warns on engine mismatches or rejects installation with engine enforcement enabled; the shipped viewer code is unchanged.
- In light mode, the viewer's sidebar, table of contents, breadcrumbs, and comments use neutral instead of blue-tinted grays. Dark mode is unchanged.
- `rw confluence render` with the current page's XHTML piped on stdin no longer takes seconds on long pages: a 108 KB page now takes under a tenth of a second instead of about 4 seconds, including when it was edited since its last render and has inline comments.
- `@rwdocs/core` sites created with `mtimeSource: "git"` now walk a page's git history once per checked-out commit instead of on every render, so rendering a page again no longer slows down as history grows.
- `rw backstage publish` no longer uploads pages the bucket already holds unchanged (given `s3:ListBucket`), so republishing a few edits takes a few uploads. Its summary reads "Published: uploaded N objects" instead of "Published N files". See [Embedding](docs/embedding.md#publish-from-ci-with-full-history).
- **Breaking (pre-1.0):** `rw comment` now exits 4, not 2, when a comment or document isn't found. Exit 2 is left to argument errors, as clap reports them. See [Exit codes](docs/comment-cli.md#exit-codes).
- Open pages now refresh their comments after `rw comment` changes them even when the CLI can't reach `rw serve` over loopback, as with a sandboxed agent; before, the page silently stayed stale. The refresh can lag up to about half a second, and `rw serve` no longer writes `.rw/server.json`.
- **Breaking (pre-1.0):** a directory's `README.md` is now its page when it has no `index.md`: `docs/guides/README.md` is served at `/guides` instead of `/guides/README`, and `docs/README.md` at `/`. Links and wikilinks to the README follow it. Where `index.md` sits beside it, rw leaves the `README.md` out of the site and logs a warning. Old `/…/README` URLs answer 404, and inline comments made on a `/x/README` page do not move to `/x`. S3 manifests remain version 1; upgrade `@rwdocs/core` readers before publishing with this version, since older readers ignore the new `readme` field and their links to folder READMEs answer 404. See [README.md as Homepage](docs/configuration.md#readmemd-as-homepage).
- A link that leaves the docs directory with `..`, such as `../guide.md` from `docs/guide.md`, now resolves from the file's real location: it stays as written instead of opening `/guide`. Links in a project-root `README.md` homepage now resolve from the project root, so `guide.md` there no longer opens `/guide`. A link to a missing `index.md` is broken. See [README.md as Homepage](docs/configuration.md#readmemd-as-homepage).
- **Breaking (pre-1.0):** frontmatter must be the first thing in a markdown file. rw no longer reads it below an HTML comment or a blank line; move such a comment below the frontmatter. See [Page Metadata](docs/metadata.md).
- The `rw` binary is about 0.4 MB smaller: it no longer carries the viewer's JavaScript source maps, so browser DevTools shows the viewer's code minified.
- **Breaking (pre-1.0):** a wikilink target is a file name, so a `%` in it is literal: write `[[::my page]]`, not `[[::my%20page]]`, which now opens a missing page.
- The viewer's sidebar and breadcrumb links now spell a page URL as page content links do: `/a+b`, not `/a%2Bb`, for `a+b.md`. Old bookmarks keep working.
- `rw serve` always serves a preview of the embedded viewer, inside a Backstage-like host page, at `/_embedded/`. The hidden `--embedded` flag, which put that page at `/` in place of the standalone viewer, is gone. See [Embedding](docs/embedding.md).
- rw now logs warnings to stderr by default: `rw serve` shows problems such as invalid page metadata, conflicting source files and page render warnings without `--verbose`, which now only adds info messages. `RUST_LOG`, when set, now takes precedence over `--verbose`. Log lines carry color codes only when stderr is a terminal.
- **Breaking (pre-1.0):** `@rwdocs/viewer`'s `mountRw()` replaces `initialPath` with `basePath`, where your app shows the section's root page, and `location`, the browser's still-encoded `pathname + search + hash`. `navigateTo()` takes that location too; call it on every location change, hash-only ones included. It no longer calls `onNavigate`, which now reports only navigation the viewer starts, such as a link click, so update your URL yourself. The `embedded` option is gone. See [Embedding](docs/embedding.md#migrating-from-initialpath).
- **Breaking (pre-1.0):** `rw confluence render` copies images stored in the project into its bundle as `image_<hash>.<ext>` attachments, which `page.xhtml` references, instead of leaving them for you to upload. Publishers must upload every bundle file except `page.xhtml`, with the MIME type its extension gives, not a hard-coded `image/png`. See [Confluence Rendering](docs/confluence.md#bundle-format).
- `rw confluence render` no longer deletes every `*.png` in the bundle directory, only the `diagram_*.png` and `image_<hash>.<ext>` attachments an earlier render left there.
- `rw confluence render` publishes an image whose source has any scheme, such as `ftp://`, as an external image, as `rw serve` does, instead of as an attachment named after the file.
- A C4 `!include` such as `systems/sys_payments_api.iuml` now includes a real file of that name wherever rw renders it; page metadata is used only when no such file exists. Before, `rw serve`, `rw confluence render` and `@rwdocs/core` sites created with `projectDir` preferred the metadata, while published S3 sites used the file. Rename or remove such a file to use the metadata. See [Diagram Includes](docs/metadata.md#diagram-includes).
- `rw backstage publish` uploads the images pages show, skipping those already in the bucket (given `s3:ListBucket`), and `--strict` now also fails when one can't be published. Its warnings print under "Warnings" instead of "Diagram warnings".

### Removed

- **Breaking (pre-1.0):** `rw serve --cache <BOOL>` and `--live-reload <BOOL>` are gone. Use `--no-cache` or `--no-live-reload` to turn either off; both stay on by default, and `rw.toml` settings work as before.
- **Breaking (pre-1.0):** `[metadata] name` in `rw.toml` is gone: sidecar files are always `meta.yaml` and `<name>.meta.yaml`. rw ignores the key, so a site that set another name must rename its sidecars, or rw stops reading them. See [Page Metadata](docs/metadata.md).
- `rw serve` no longer answers `GET /_api/comments/{id}`; it returns 405. List a page's comments with `GET /_api/comments?documentId=…`, or read one with [`rw comment show <id>`](docs/comment-cli.md). The viewer never used it.

### Fixed

- After a reload, an S3-backed `@rwdocs/core` site could pick up a stale page tree that another process sharing the bucket had cached. rw no longer caches the page tree, which also saves one S3 read and write per reload.
- When the navigation fails to load, the viewer's sidebar and mobile menu each show one "Failed to load navigation" alert instead of two stacked alerts.
- A page under a top-level `assets/` folder, such as `docs/assets/guide.md`, now opens in the viewer. Before, the sidebar listed it, but opening it showed "404 Page not found".
- The embedded viewer no longer spins forever when a host's custom `fetchFn` aborts a page or navigation request on its own, such as on its own timeout; the viewer shows its usual error state instead.
- `rw confluence render` with the current page's XHTML piped on stdin no longer shuffles element attributes between runs, so rendering unchanged Markdown twice produces an identical `page.xhtml`.
- `rw confluence render` now names the file or directory it could not read, create, or write, instead of printing a bare `Error: I/O error`.
- `rw confluence render` now warns that Kroki renders `nomnoml`, `svgbob` and `wavedrom` diagrams only as SVG, which it can't attach, instead of passing on Kroki's 400 error. In `rw serve` and `@rwdocs/core`, a `{format=png}` fence of those types gets the same error. See [Confluence Rendering](docs/confluence.md#bundle-format).
- `rw comment add --help` and `rw comment reply --help` now name the real default author, `local:ai` / `AI`, instead of `local:human` / `You`. Only the help text changed; comments were already stamped as AI.
- When `rw confluence render` cannot parse the current page's XHTML piped on stdin, its "comment preservation skipped" warning now gives the parser's reason instead of a bare "XML parse error".
- A `README.md` homepage now shows when `docs/` has a `meta.yaml` but no `index.md`. rw used to render an empty "Home" page there; `docs/meta.yaml` now applies to the README homepage.
- In `rw serve`, a page under a top-level `_api/` folder, such as `docs/_api/guide.md`, now opens by its URL. Before, the sidebar opened it, but reloading or sharing the link gave a bare 404.
- An `index.md` without an H1 is titled after its directory (`setup-guide/index.md` becomes "Setup Guide", the homepage "Home") instead of "Index". See [Title resolution](docs/metadata.md#title-resolution).
- With the current page's XHTML piped on stdin, `rw confluence render` writes code block bodies, empty ones included, as CDATA, as a fresh render does, instead of escaping them. Confluence displays both the same.
- A link to the homepage's `index.md` from the homepage or a top-level page now opens `/` instead of a missing `/index` page.
- On macOS and Windows, only a file named exactly `index.md` or `README.md` serves a directory's page or the homepage, as on Linux. Before, `a/Index.md` could replace the body of the page `a.md`, and a lowercase `readme.md` became the homepage.
- A code block containing `]]>` no longer makes `rw confluence render` write malformed XHTML.
- With the current page's XHTML piped on stdin, `rw confluence render` no longer silently drops an inline comment marker in edited text because another marker with the same `ac:ref`, or another with none, was already carried over.
- An unrecognized container directive opened with four or more colons, such as `::::note`, now renders with all of its colons instead of three.
- In the viewer's page comments list, the quoted passage on an orphaned or resolved comment now updates when the comments API re-anchors it while the page is open, instead of showing the old passage until reload.
- `rw serve` live reload now picks up page edits when the docs directory is given as a relative path (such as `--project-dir docs-site`) or, on macOS, through a symlink such as `/tmp`. Before, rw ignored every change inside it.
- If `rw serve` was killed or ran out of disk space while caching a rendered page or diagram, it could keep serving that truncated copy from `.rw/cache/`; a diagram stayed broken until the next rw upgrade. rw now replaces a cache entry only once it is fully written.
- Screen readers now announce the viewer's page comment count as "3 comments" instead of a bare "3", and the "fuzzy" badge on a re-anchored comment as "Re-anchored to the closest matching passage".
- When storage fails, the errors that `@rwdocs/core`'s `renderPage`, `renderSearchDocument`, and `getPageMarkdown` reject with now name each cause once, instead of repeating the storage error and its cause.
- A storage error for a file rw lacks permission to read no longer says "Permission denied" twice in `rw serve`'s 503 responses and `@rwdocs/core` rejections: `[Fs] Error (path: guide): Permission denied (os error 13)`.
- Storage error messages from the `rw` CLI and in the `rw serve` log now name every underlying cause, not just the first. There, in `rw serve`'s 503 responses, and in `@rwdocs/core` rejections, a storage error's `(path: …)` now comes before its causes.
- Other `rw` CLI errors, such as a network failure in `rw update`, now also name every underlying cause instead of stopping at the first.
- An invalid `rw.toml` no longer reports `TOML parse error: TOML parse error at line …` in the `rw` CLI and `@rwdocs/core`'s `createSite()`; the prefix appears once.
- An invalid `RW_DIAGRAMS_KROKI_URL` no longer fails `rw backstage publish` or the `rw comment` commands that render nothing. Only `rw serve`, `rw confluence render`, and `rw comment add --quote` read it. See [Configuration](docs/configuration.md#rw_diagrams_kroki_url-fallback).
- A diagram that fails because rw cannot reach Kroki now says why, such as `HTTP request failed: io: Connection refused (os error 61)` instead of a bare `HTTP request failed`, in `rw serve`, `rw confluence render`, and `@rwdocs/core`.
- On macOS, `rw serve` live reload now treats deleting a page created shortly before as a deletion. Before, the sidebar could keep listing the deleted page, and an open viewer reloaded it into a not-found error.
- After `rw serve` restarts, the first edit to the page an open viewer shows now reloads that page. Before, rw refreshed only the navigation.
- On narrow screens, the viewer's new-comment popover now opens below the first selected line instead of covering its lower half, matching where an existing thread's popover opens.
- An S3 manifest page with an invalid `namespace` or `name`, such as one edited by hand, no longer crashes an S3-backed `@rwdocs/core` site. rw drops that field with a warning, as it does for a local page. See [Fields](docs/metadata.md#fields).
- `rw backstage publish` now lists problems found while scanning the docs, such as invalid page metadata or a file left out because it conflicts with another, so `--strict` fails on them. `rw serve --verbose` logs each one once instead of on every rescan. See [Diagnostics](docs/metadata.md#diagnostics).
- `rw backstage publish` now lists `pages` entries that rw skips, such as a slug with no matching page, so `--strict` fails on them. `rw serve --verbose` logs each one once instead of on every reload. See [Navigation ordering](docs/metadata.md#navigation-ordering).
- `rw backstage publish` now lists sections that share a full ref, such as two `domain:default/billing` directories, so `--strict` fails on them. `rw comment` and `rw confluence render` no longer print that warning. See [`name`](docs/metadata.md#name-identity-independent-of-the-documentation-path).
- When `rw backstage publish` republishes a change to a PlantUML `!include`d file alone, an S3-backed `@rwdocs/core` site, such as Backstage, now shows the new diagram instead of the page it cached. Both the publisher and `@rwdocs/core` need this version.
- When `rw backstage publish` republishes unchanged docs, an S3-backed `@rwdocs/core` site, such as Backstage, no longer reloads them. Only the publisher needs this version.
- In dark mode, the embedded viewer (`mountRw()`, as in Backstage, and `rw serve --embedded`) now draws its buttons, menus, badges, alerts, and loading placeholders in dark colors. They stayed light, such as a white diagram expand button on a dark page.
- `rw serve` live reload now applies edits to a page's `kind`, `name`, `namespace`, `description`, or `attrs`. Before, the sidebar, sections, breadcrumbs, and the page's metadata kept the old values until an unrelated change, such as adding a page, refreshed the site.
- `rw backstage publish` and `@rwdocs/core`'s `createSite({ mtimeSource: "git" })` now report git commit times when the project directory is relative, such as with `-c rw.toml` or `projectDir: "."`. Before, rw fell back to filesystem times, which in CI is the checkout time.
- `rw backstage publish` and `@rwdocs/core`'s `mtimeSource: "git"` now report git commit times for files checked out with CRLF line endings, the Git for Windows default. Before, rw gave them the checkout time, a new one on every CI run.
- `rw comment reply` and `rw serve`'s comments API now refuse a reply to a reply (exit 3, HTTP 400) instead of saving one that no list or viewer shows; the API also refuses a reply to a deleted reply. `rw comment resolve` and the API also refuse to resolve a reply. Existing comments still load. See [`rw comment`](docs/comment-cli.md).
- `rw comment add --quote ""` now fails with "quote is empty" (exit 3) instead of calling the quote ambiguous or, on a page with no text, anchoring the comment to nothing. `rw serve`'s comments API answers an empty `quote` with 400.
- A homepage made of only a `docs/meta.yaml`, with no `index.md` and no `title`, is now titled "Home" instead of "Untitled", as an untitled `README.md` homepage already was.
- When `rw confluence render --out -` cannot parse the current page's XHTML piped on stdin, it no longer prints an error log line ahead of the XHTML on stdout, where it ended up in the published page. rw now writes all logs, including `rw serve --verbose`, to stderr.
- A PlantUML `!include` now expands only on its own line. Before, rw also expanded its path inside a longer one (`a.iuml` within `!include a.iuml.bak`) or a commented-out `' !include`, and could drop a repeated include's indentation.
- A `[[target\|text]]` wikilink, the form a GFM table needs, no longer leaves a `\` on the target, which broke the link.
- A list item holding only a link reference definition, such as `- [r]: http://x`, followed by an indented blank line no longer crashes rendering its page in `rw serve` and `@rwdocs/core`, or aborts `rw backstage publish` and `rw confluence render`. rw still prints a harmless `pulldown-cmark` panic message for such a page.
- In the viewer, a relative link to something other than a `.md` file, such as `[x](sibling)` or `[x](../api/)`, now opens the page relative to the current one, as the address bar shows. Before, the viewer loaded that path from the site root, and the embedded viewer passed the unresolved link to the host.
- An embedded viewer mounted without `resolveSectionRefs` now opens a page's links into the mounted section under its `basePath`. Before, it handed the host's `onNavigate` the bare site path, such as `/api`, outside the docs.
- An embedded viewer based at `/x/docs` now hands a link to `/x/docs-v2/page` to the host's `onNavigate` instead of opening a broken `/-v2/page` itself.
- A page whose file name contains `..`, such as `a..b.md`, now renders in `rw serve` and `@rwdocs/core` instead of failing, and no longer aborts `rw backstage publish`. Its `a..b.meta.yaml` sidecar now applies too.
- `rw serve` live reload now drops a directory's pages from the sidebar when the directory is moved out of the docs directory, such as with `mv` or Finder's Move to Trash. Before, the sidebar kept them until another change refreshed the site.
- `rw serve` live reload now drops deleted pages from the sidebar when the same deletion removes a folder with no pages, such as `rm -r` on a folder with an `img/` subfolder. Before, the sidebar could keep listing them until another change refreshed the site.
- `rw serve` live reload no longer collapses the sidebar sections you expanded each time it refreshes the sidebar, such as when a page is added or a JetBrains IDE saves one. A new section still starts collapsed.
- `rw confluence render` now warns about each diagram it could not render or write as an attachment, so `--strict` fails. Before, it put an error figure in `page.xhtml` with nothing on stderr and exit code 0.
- PlantUML and C4 diagrams no longer have every `width: …px` and `height: …px` inside them halved, such as in a label reading `width: 300px`; rw scales only the diagram's own size.
- A PlantUML diagram whose C4 metadata include (`!include systems/sys_….iuml`) names a section with a double quote in its title or description now renders, showing the quote. Before, Kroki rejected the whole diagram.
- `rw confluence render` of a file in the docs directory now expands C4 metadata includes (`!include systems/sys_….iuml`), as `rw serve` does, and warns about one that names no section. Before, it sent them to Kroki unexpanded, so the page published an error figure. See [Diagram Includes](docs/metadata.md#diagram-includes).
- An unrecognized block directive now renders exactly as written. Before, rw dropped trailing text (`:::note This is important` became `:::note`) and reordered and requoted attributes (`::foo[x]{b=2 a=1}`).
- A `:status` badge with a bare word in its braces, such as `:status[Done]{draft color=green}`, now takes its color instead of rendering grey.
- A `:::` inside a blockquote or list item no longer closes a tab or other container opened outside it, which produced broken HTML. rw renders it as text and warns about a stray `:::`.
- Hovering a disabled or busy button in the viewer, such as "Comment" on an empty comment form, no longer makes it transparent. In dark mode, hovering the red "Retry" button no longer clears its fill.
- `rw comment` now writes `.rw/.gitignore` too. In a project where `rw serve` had never run, any `rw comment` command, even `list`, created the comments database in `.rw/` without it, so `git add .` could commit the database.
- `rw comment list` and `rw comment show` no longer create `.rw/` and an empty comments database; before the first comment is written they find nothing.
- `rw comment --document` now accepts the absolute path of a project `README.md` homepage reached through a symlink, such as `/tmp` on macOS. A source file rw leaves out of the site, such as a hidden or symlinked one, now exits 3 as not a page, where `add` exited 4 and `list` printed nothing.
- In an embedded viewer, a protocol-relative link such as `//example.com/a`, in a page or a diagram, now opens that site instead of a broken path under the docs base URL.
- A wikilink into a section whose folder name contains `#`, `?` or `%`, such as `[[system:sys1::api]]` for `sys#1/`, now opens the page instead of a broken URL.
- A wikilink with a query, such as `[[::api?tab=x]]`, now shows the page's title instead of `api?tab=x`.
- When `rw serve` cannot load the site from storage, creating a comment with a `quote` through its comments API now returns a 500 naming the storage error, instead of a 404 saying the document does not exist.
- `rw serve --host ::1` now starts instead of failing with "invalid bind address"; before, only the bracketed `[::1]` worked.
- `rw comment add`, `reply`, and `resolve` now refresh open browsers when `rw serve` is bound to an IPv6 address. Before, the notification silently failed and viewers showed the change only after a reload.
- A `kind` that is empty or contains `:`, such as `kind: "a:b"`, no longer makes a section whose ref navigation, comments, and section lookups cannot resolve. rw drops that `kind` with a warning, as it does an invalid `namespace`, including from an S3 manifest. See [Fields](docs/metadata.md#fields).
- `rw backstage publish --bucket-root-path` and `@rwdocs/core`'s `s3.bucketRootPath` now ignore leading and trailing slashes, so `/techdocs/` and `techdocs` find the same bundles. Republish bundles that were published with such slashes; rw leaves the old copies in the bucket.
- A wikilink to a folder's `index.md`, such as `[[::guide/index]]`, now opens `/guide` and shows that page's title, as a link to `guide/index.md` does.
- A link between pages in a folder whose name contains `#`, `?` or `%`, such as `c#/`, now opens the page instead of a URL cut short at that character.
- A link to a page whose file name contains `\`, such as `x\y.md`, now opens that page, from page content or a C4 diagram box. Before, the browser read the `\` as `/` and opened a missing page.
- On Linux and macOS, a folder whose name contains `\`, such as `a\b/`, now appears in the site with its pages. Before, rw dropped the folder and everything in it.
- On Linux and macOS, `rw comment --document` now accepts a page in a folder whose name contains `\`, such as `docs/a\b/child.md`. Before, it said the file was not a markdown page.
- The wikilink `[[::#heading]]` now links to that heading on the current section's root page instead of rendering as a broken link.
- An ASCII control character (U+0000–U+001F) other than tab or a line break, such as NUL or a pasted terminal escape, in page content now renders as `�` (U+FFFD) in `rw serve`, `@rwdocs/core`, and `rw confluence render`; a form feed renders as a space. Before, `page.xhtml` kept it, and Confluence rejects that as malformed XML. Raw HTML is unaffected.
- In an embedded viewer, a link to a section's root page with a query string, such as `/domains/billing?tab=x`, now opens that section instead of resolving through the parent section.
- The navigation sidebar and `@rwdocs/core`'s `getNavigation()` now sort a directory without an `index.md`, and a page inside a directory with no page of its own (by the rest of its path, such as `old/faq`), alphabetically among its sibling pages, as documented, instead of after them. Parents that set `pages` already did this for unlisted children. See [Navigation ordering](docs/metadata.md#navigation-ordering).
- With the current page's XHTML piped on stdin, `rw confluence render` no longer turns `&copy;` or `&nbsp;` in a code block into the characters they name, nor an entity it does not know in raw HTML, such as `&hearts;`, into the text `&hearts;`. Quotes and apostrophes now come out as in a fresh render.
- With the current page's XHTML piped on stdin, `rw confluence render` no longer adds a space on each side of a commented passage whose inline comment marker included surrounding spaces.
- `rw confluence render` now self-closes `<br>`, `<hr>`, `<img …>`, and other void elements in raw HTML. Before, Confluence rejected the page as malformed. See [Images and raw HTML](docs/confluence.md#images-and-raw-html).
- `rw confluence render` now publishes `data:` and protocol-relative `//host/…` images as URLs instead of attachments, and names a local image's attachment without percent-escapes (`a%20b.png` becomes `a b.png`). It warns about each local image, which you upload yourself, so `--strict` fails on pages that have one.
- `rw confluence render` now publishes an image's alt text, such as `Flow` in `![Flow](img/flow.png)`. Before, it dropped it.
- When `rw confluence render` cannot write a diagram's PNG, it still prints that diagram's other warnings, such as an unknown attribute.
- With the current page's XHTML piped on stdin, `rw confluence render` no longer moves an inline comment onto the same words earlier in the paragraph when they sit inside formatting, such as bold or a link.
- With the current page's XHTML piped on stdin, `rw confluence render` no longer drops an attribute written without a value, such as `disabled` in raw HTML. rw skips comment preservation with a warning and writes the fresh render.
- After an rw upgrade, an S3-backed `@rwdocs/core` site now shows the new version's rendering fixes on every page and diagram, instead of serving what the old version cached. No cache clearing is needed. Old caches stay in the bucket until you delete them; see [Embedding](docs/embedding.md#the-s3-render-cache-is-per-rw-version).
- When a C4 metadata include, such as `!include systems/sys_shared.iuml`, matches sections in several namespaces, rw now always picks the one whose page path sorts first. Before, the pick could change whenever rw reloaded the site. See [ambiguous include names](docs/metadata.md#ambiguous-include-names).
- `sectionAncestry` in `rw serve`'s page and navigation responses and in `@rwdocs/core`'s `renderPage()` and `getNavigation()` now lists its keys in a fixed order, so repeated requests for an unchanged page return identical JSON.
- `RUST_LOG=warn rw serve` now logs page render warnings, such as an unclosed `::::tabs` or an unresolved PlantUML `!include`. Before, rw showed them only with `--verbose`.
- An `rw.toml` that rw cannot read or parse is now named in the error, such as `cannot read ./site/rw.toml: Is a directory (os error 21)` instead of `I/O error: Is a directory (os error 21)`, in the `rw` CLI and `@rwdocs/core`'s `createSite()`.
- `rw serve --host localhost`, or the same `host` in `rw.toml`, now fails naming the host and saying it must be an IP address such as `127.0.0.1` or `::1`, instead of a bare `invalid bind address: invalid IP address syntax`.
- `rw backstage publish` and `@rwdocs/core`'s `mtimeSource: "git"` now give a page with staged but uncommitted edits its filesystem time, as for any other uncommitted edit, instead of its last commit's time.
- Editing a PlantUML `!include` file, or creating one that was missing, now shows in `rw serve` on the next load of every page that includes it. Before, rw served the cached page, even after a restart.
- `rw serve` live reload now refreshes an open page when a PlantUML `!include` file it uses is edited or created. Before, the change showed only after reloading the page by hand.
- `rw serve` live reload now refreshes an open page when you rename or move a folder holding an image, diagram file, or PlantUML `!include` file the page uses. Before, the page kept showing the old image or diagram until reloaded by hand.
- An open page in `rw serve` now refreshes when live reload reconnects, such as after a server restart, picking up edits made while it was down.
- Moving `foo.md` to `foo/index.md` now fixes the page's relative links in `rw serve` and `@rwdocs/core`. Before, rw served the cached page with links resolved from the old location, even after a restart.
- With `source_dir = "."`, a `README.md` homepage no longer also appears as a `/README` page in the sidebar; links to `README.md` open `/` instead. `rw serve` live reload now refreshes the homepage when it changes. See [README.md as Homepage](docs/configuration.md#readmemd-as-homepage).
- A link from a docs page to a project-root `README.md` homepage, such as `../README.md`, now opens the homepage instead of a missing `/README` page.
- On Linux, `rw serve` live reload now keeps picking up edits to a project-root `README.md` homepage that an editor saves by replacing the file. Before, it stopped after the first such save. Live reload also picks up a `README.md` created after `rw serve` starts.
- The viewer now saves a comment on text with an emoji about 32 characters before or after the selection. Before, `rw serve` rejected it, and the viewer showed "Couldn't save your comment".
- A bare `$name` in `rw.toml`'s `server.host` or `diagrams.kroki_url` now stays as written when the value also uses `${VAR}`. Before, rw substituted it or failed with `${name} not set`. Write `$${` for a literal `${`; an unclosed `${` is now an error. See [Environment Variables](docs/configuration.md#environment-variables).
- `rw serve` now loads a page whose URL contains a dot, such as `/release-1.2` or `/v1.0/intro`, when opened directly or refreshed. Before, it answered 404.
- On a page whose path has non-ASCII characters, such as `/гайд/шаг`, the viewer now highlights its sidebar entry and expands the sidebar to it, and `rw serve` live reload refreshes it. Before, none of these happened on such a page.
- `rw serve` now keeps answering other requests while pages wait on a slow Kroki server. Before, as many slow renders as the machine has CPU cores stalled every request, static assets included, until one finished.
- In `rw serve` and `@rwdocs/core`, a link to a `.md` file with a query, such as `guide.md?tab=x`, now opens that page and keeps the query. A `.md` link with a scheme, such as `HTTPS://…/README.md`, `ftp:` or `vscode:`, now stays as written instead of becoming a broken site path.
- `rw backstage publish` now fails, before uploading anything, when the docs have both a homepage and an `_index` page such as `docs/_index.md`. Before, Backstage showed whichever of the two uploaded last at both addresses.
- A blank `title` or `description` (empty, only whitespace, or null) in frontmatter or `meta.yaml` now counts as not set, so the next source applies instead of a blank sidebar label or the filename. rw no longer warns about a null one. See [Fields](docs/metadata.md#fields).
- A `---` block anywhere but at the very start of a page, such as a second one after the frontmatter, now renders as a horizontal rule and a heading, as on GitHub, instead of vanishing, and no longer replaces the page's frontmatter.
- A markdown file starting with a byte order mark (BOM) now gets its frontmatter and opening H1 read. Before, rw rendered the frontmatter as a heading and titled the page after its file name.
- rw now reads frontmatter written with Windows (CRLF) line endings. Before, it ignored every field in it.
- An embedded viewer now sends a link to its section's root page with a `#heading` or query, such as `/catalog/x/docs#intro`, to the host's `onNavigate` as that page. Before, it added the section's own path, as in `/catalog/x/docs/billing#intro`, which showed "Page not found" on reload.
- `destroy()` on a viewer mounted with `mountRw()` now removes the `dark` class it put on the host element.
- Diagram warnings and error figures in `rw serve`, `rw confluence render`, and `@rwdocs/core` now number diagrams from 1 in page order, as in `diagram 2: duplicate id 'flow' (already used by diagram 1)`. Before, rw counted every code block from 0, indented ones included. See [Diagram IDs](docs/diagrams.md#diagram-ids).
- A PlantUML diagram inside a blockquote or list item now stays there in the pages `rw backstage publish` uploads when it has an `!include`, and always in `@rwdocs/core`'s `renderSearchDocument()` text. Before, rw dropped the `>` or indentation from the diagram's lines, ending the blockquote or list early and spilling the diagram out as text.
- `rw backstage publish` now names the page in each PlantUML `!include` warning, as in `/guide: Include file not found: 'parts.iuml'`, as it does for its other page warnings. A broken include several pages share now prints once per page.
- On wide screens, the viewer's table of contents button now stays available while a comment thread or new comment is open. Before, the comments sidebar replaced the outline and left no way to reach it.
- In an embedded viewer whose host scrolls its own content pane, as Backstage does, a comment thread's previous and next buttons now keep the thread under the cursor instead of letting it jump, often off screen, to the next highlight.
- In an embedded viewer whose host scrolls its own content pane, opening another page now starts it at the top instead of at the previous page's scroll position.
- A colon-prefixed word in prose, such as `:id` or a Ruby symbol, now stays text. Before, rw rendered `:status` as an empty badge and warned about any other name, failing `--strict`. An inline directive now needs `[...]` or `{...}` right after its name. See [Status Badges](docs/status-badges.md#syntax).
- `rw comment list` text output now separates its columns, prints the full comment id that `show`, `reply`, and `resolve` take, and indents every line of a multi-line body. In `list` and `show`, a quote that spans lines prints on one line, and ends with `…` only when shortened.
- The viewer's expand button now opens a hand-authored `<figure class="diagram">` whose SVG or image sits between blank lines or inside a link, such as `[![x](a.png)](url)`. Before, it opened an empty viewer.
- In dark mode, an `<svg>` nested inside a hand-authored diagram's SVG now darkens with the rest of the diagram instead of keeping its light colors.
- A PlantUML `!include` of a file the diagram already included now expands to nothing, as in PlantUML, so include cycles render. Before, rw expanded a file that includes itself ten levels deep, which could take seconds per page and log tens of thousands of warnings.
- An `!include` inside an included PlantUML file now finds files next to that file first, then in `include_dirs`, as PlantUML does, so a vendored library's own relative includes work. See [PlantUML Includes](docs/diagrams.md#plantuml-includes).
- A PlantUML `!include` of a URL no longer warns that the file was not found; rw passes it to PlantUML untouched, as it does standard-library includes.
- A backslash now escapes directive syntax: `\:status[Done]` and `\:::note` stay text, and `:status[Phase \] 2]` keeps the `]` in its label instead of ending the badge there. See [Status Badges](docs/status-badges.md#syntax).
- In `rw serve`, the viewer's sidebar and breadcrumb links now open a page whose file name contains `?`, `#` or `%`, such as `c#.md`. Before, clicking one opened a different or missing page.
- `@rwdocs/core`'s `renderSearchDocument()` now leaves every PlantUML preprocessor line, such as `!theme` or `!pragma`, out of a diagram's indexed text. Before, rw dropped only `!include`, `!define`, and `!$` lines.
- A PlantUML `!include` of a file holding `@startuml` diagrams now inlines one diagram's body, the first or the one a selector such as `file.puml!1` or `file.puml!ID` picks, as PlantUML does. Before, rw inlined the whole file, which cut the including diagram short. See [PlantUML Includes](docs/diagrams.md#plantuml-includes).
- rw now resolves PlantUML `!include_many`, `!include_once`, and `!includesub file.puml!NAME` from `include_dirs`, as it does `!include`. Before, rw passed them to Kroki, which cannot read local files.
- In an embedded viewer with a section resolver, such as Backstage, breadcrumbs and section links now reach pages whose names contain `#`, `?`, or `%`. A page's `subpath` and section anchors no longer stop at a `#` in its path, so such pages no longer share comments. Comments left on them earlier stay with the shortened path: those on `c#/intro` stay on `c`.
- A character written as an entity no longer forms directive syntax, as in remark-directive: `&#58;status[Done]` and `:&#58;youtube[x]` stay text. In a quoted attribute value, `&quot;` and `\"` are part of the value, so `{title="a \"b\" c"}` keeps all of it. See [Status Badges](docs/status-badges.md#syntax).
- In an embedded viewer with a section resolver, a link in page content now leads to the docs of the section it points into when that section's folder name contains `#`, `?`, `%`, or another character the link percent-encodes. Before, it resolved through the site root.
- `rw comment add --quote` and quotes sent to `rw serve`'s comments API now anchor on pages in a section whose name contains `#`, such as `sys#1/`. Before, rw reported the page as not found.
- `rw serve --embedded` now shows the viewer in released `rw` binaries. They shipped without the viewer's embed bundle, so the viewer never appeared.
- With a nested `source_dir` such as `site/docs`, a link to `site/docs/guide.md` in a project-root `README.md` homepage now opens `/guide`, and `../../README.md` from `site/docs/guide.md` opens the homepage. See [README.md as Homepage](docs/configuration.md#readmemd-as-homepage).
- `@rwdocs/core`'s `renderSearchDocument()` now gives a current-section wikilink such as `[[::rates]]` its page title instead of the raw `::rates`.
- A backslash escape at the start of a wikilink's display text, as in `[[::page|\*draft*]]`, now works as it does elsewhere: the link reads `*draft*` instead of `\*draft*`.
- With a nested `source_dir` such as `site/docs`, `rw comment add` and `list` now accept `--document site/docs/guide.md` instead of calling it "not a markdown page". See [`rw comment`](docs/comment-cli.md).
- A tab in the viewer that has keyboard focus now shows a focus ring. Before, it looked the same as an unfocused selected tab.
- Screen readers now announce the viewer's sidebar expand buttons with their page, such as "Expand Getting Started", and whether the section is expanded.
- Escape now closes the embedded viewer's mobile navigation menu (`mountRw()`, as in Backstage, and `rw serve --embedded`) while focus is in the viewer, as it already did in the standalone viewer.
- An absolute `source_dir` that reaches the project through a symlink, such as `/tmp` on macOS, now counts as inside the project: `rw comment --document site/docs/guide.md` finds the page, and links in a project-root `README.md` homepage resolve as with a relative `source_dir`. `rw comment --document` also accepts an absolute path through a symlink, such as `$PWD/docs/guide.md` in a symlinked checkout.
- `rw comment --document` now rejects a path with `..`, such as `docs/../docs/guide.md`, as "not a markdown page" instead of filing the comment under a page that does not exist. It also accepts a leading `./`, as in `./docs/guide.md`.
- `rw comment add`, `rw comment reply`, and `rw serve`'s comments API now refuse a comment on a page that doesn't exist (exit 4, HTTP 404) instead of saving one no viewer shows. Virtual pages still take comments. See [`rw comment`](docs/comment-cli.md).
- `rw confluence render` takes the title only from an H1 that opens the page, with nothing but HTML comments above it. Before, an H1 further down became the title, and only the headings after it moved up a level. A blank H1 no longer gives an empty title. See [Title resolution](docs/metadata.md#title-resolution).
- In `rw serve` and `@rwdocs/core`, an H1 below another heading or a horizontal rule no longer titles the page, which falls back to its file name instead. An H1 below only HTML comments, such as `<!-- markdownlint-disable MD041 -->`, now titles it. An H1 that isn't the title now appears in the table of contents.
- A page titled by its H1 now gets the text the H1 shows in the sidebar, breadcrumbs and browser tab: `# Done :status[OK]` gives "Done OK" instead of the directive syntax, and an image's alt text no longer counts. A `[[target]]` gives its target; `[[target|text]]` gives the text. S3 sites change after a republish. See [Title resolution](docs/metadata.md#title-resolution).
- In an embedded viewer (`mountRw()`, as in Backstage), Escape pressed elsewhere in the host page no longer closes the viewer's open table of contents, breadcrumb menu, or comment popover, nor moves focus back into the viewer.
- Other open viewers now show a comment added, resolved, reopened, or deleted in a tab that closed or navigated away mid-save. Before, `rw serve` saved the change but didn't tell them, so it appeared only after a reload.
- `rw serve --verbose` now logs a warning for each diagram that fails to render, with the text its error figure shows, such as `diagram 2: HTTP 400: …`. Before, only `rw confluence render` warned.
- `rw serve` now shows remote `https:` images, such as badges and screenshots. `http:` images stay blocked.
- A C4 diagram box from a metadata include, such as `!include systems/sys_….iuml`, now links to its section's page when the section's folder name contains a space, `#`, `?`, `%`, braces, or brackets. Before, the link opened a different or missing page, or the diagram showed a PlantUML error.
- `rw serve` now logs a page's render warnings, such as an unclosed `::::tabs` or an unresolved PlantUML `!include`, every time it serves the page. Before, rw logged them only when it rendered the page, so a cached page logged nothing, even after a restart.
- In an embedded viewer (`mountRw()`, as in Backstage), the `n`, `p`, and `r` comment shortcuts no longer take the key from a focused host button or link. They still work while focus is in the viewer or on nothing.
- A link that re-enters the docs directory by name, such as `../docs/guide.md` from `docs/intro.md`, now opens `/guide` instead of a missing page.
- A link to a missing page in the docs directory now opens the URL that page would have however it is spelled: `./docs/missing.md` in a project-root `README.md` homepage and `../docs/missing.md` from `docs/guide.md` open `/missing`, as `docs/missing.md` already did.
- In an embedded viewer (`mountRw()`, as in Backstage), opening or closing a comment thread, clicking a table-of-contents entry, and following a same-page `#` link now keep the host router's `history.state`. Before, the viewer replaced it with `null`, erasing React Router's location state.

### Security

- A project-root `README.md` that is a symlink out of the project, such as to `/etc/hosts`, is no longer the homepage. Before, `rw serve` and `@rwdocs/core` served the target file, and `rw backstage publish` uploaded it. A `README.md` linked to a file inside the project still works. See [README.md as Homepage](docs/configuration.md#readmemd-as-homepage).
- PlantUML `!include` now reads only files inside `include_dirs`. rw refuses an absolute path such as `!include /etc/hosts`, or a path leading outside through `..` or a symlink, with a warning. A symlink's target must be in an include directory: one inside the project, or one passed to `rw confluence render -I`. Before, it inlined the file into diagrams sent to Kroki and into `rw backstage publish` bundles. See [PlantUML Includes](docs/diagrams.md#plantuml-includes).

## [0.1.36] - 2026-09-07

### New Features

- **Explicit section names** — Set `name` alongside `kind` to choose section/catalog identities independently of page URLs and titles. See [Page Metadata](docs/metadata.md#name-identity-independent-of-the-documentation-path).
- **Structurizr diagrams** — Render Structurizr DSL workspaces through Kroki using `structurizr` or `kroki-structurizr` fences. See [Diagram Rendering](docs/diagrams.md#structurizr).

### Added

- On pages declaring `kind`, `name` now controls section/catalog refs and PlantUML metadata includes independently of URLs and titles. Names never inherit; old refs/includes are not aliases and comments are not migrated. Upgrade S3 readers before publishing explicit names. See [Page Metadata](docs/metadata.md#name-identity-independent-of-the-documentation-path).
- Structurizr DSL diagrams now render through Kroki from `structurizr` and `kroki-structurizr` fenced code blocks. See [Diagram Rendering](docs/diagrams.md#structurizr).

### Changed

- **Breaking (pre-1.0):** `@rwdocs/viewer` now requires Node `^22.22.2 || >=24.15.0`, up from `^22.13.0 || >=24`. npm warns on engine mismatches or rejects installation with engine enforcement enabled.

### Removed

- **Breaking (pre-1.0):** Page metadata now ignores `type` in frontmatter and YAML sidecars. Rename it to `kind`; pages relying only on `type` still load but no longer register as sections. See [Page Metadata](docs/metadata.md).

### Fixed

- An invalid `namespace` no longer fails the site load. rw warns and uses a valid frontmatter or sidecar declaration, otherwise the inherited namespace (`default` if none). See [Page Metadata](docs/metadata.md#namespace).
- A wrong-typed metadata field no longer discards valid sibling fields. rw warns with the filename and drops only the offending field; invalid frontmatter values fall back to valid sidecar values. See [Page Metadata](docs/metadata.md#diagnostics).
- Malformed YAML and non-mapping metadata sources now make rw log warnings naming the file. Empty or null sources remain silent.
- A project-root `README.md` used as the homepage now applies all of its frontmatter metadata, rather than only its title.
- Starting an inline comment on the first words after a diagram now places the comment form beside the selected sentence instead of beside the diagram.
- Comments created from quotes containing HTML character references now anchor to the same text and offsets the browser sees, instead of appearing orphaned.

### Security

- `rw`'s HTTP/2 server and S3/HTTPS clients, plus `@rwdocs/core`'s S3/HTTPS client, now include the fix for [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html). A malicious peer could exhaust memory or crash a connection. Upgrading is all you need.

## [0.1.35] - 2026-08-07

### New Features

- **Every page in navigation** — the homepage and each section's overview page now appear as the first row of their scope in the sidebar, highlighted while you're there. See [Page Metadata](docs/metadata.md#navigation-ordering).

### Added

- Release archives ship a `THIRD-PARTY.md` covering every bundled Rust and JavaScript dependency; an installer that places only the binary leaves it behind in the archive. `@rwdocs/core` and its platform packages carry the Rust half, `@rwdocs/viewer` the JavaScript half, and the platform packages gain rw's own `LICENSE-MIT` and `LICENSE-APACHE`.
- Pages served by `rw serve` now show the rw icon in the browser tab, and an app icon when the site is saved to an iOS home screen.

### Changed

- `@rwdocs/viewer` now declares Node `^22.13.0 || >=24`, up from `>=22.12.0`. Installing it on Node 22.12 or on any Node 23 release now warns.
- The navigation sidebar now lists the homepage first at the top level, and each section's own page first inside it, highlighted while you're there. See [Page Metadata](docs/metadata.md#navigation-ordering).
- Breadcrumbs now title the root crumb from the homepage's own title, falling back to "Home" only when there is no homepage.
- The navigation sidebar no longer shows rw's "RW" wordmark, on desktop or in the mobile drawer. On desktop the space it occupied stays empty, so navigation items do not move.

### Fixed

- The "On this page" outline now follows the reader. It could leave a middle entry highlighted after you switched documents, and never highlighted a final entry the page could not scroll far enough to reach.
- `@rwdocs/viewer`'s TypeScript types now resolve outside bundlers. A consumer on `moduleResolution: "node16"`, `"nodenext"`, or the older `"node"` either found no types at all or got types that no longer described the JavaScript; only `"bundler"` worked. The runtime bundle is unchanged.
- A comment you just posted no longer vanishes from the page, or appears twice, when a comment-list refresh races the post. The server stored it correctly either way, and a reload showed the truth.
- Opening another page no longer lists the comments of the page you came from beneath it, or highlights their quotes in its text, while the new page's own comments load.
- Deleting a reply no longer takes away the Restore control that undoes it. Any refresh — and live reload triggers one after every comment change, by anyone — used to make the deleted reply disappear with no way back except the API.
- A comment list that fails to load no longer empties the comments on a page you have since opened, or reports an error about the page you left.
- A long unbreakable token — a class name, a URL — in a navigation label, a section heading, or a page-outline entry now wraps instead of sliding out of line with its neighbours and past the column edge.

### Security

- rw's TLS libraries no longer carry the certificate-validation flaws 0.1.34 shipped in `aws-lc-sys` and `rustls-webpki`. They sat on every HTTPS connection rw makes: S3, Kroki, and self-update. `@rwdocs/core` is patched too; upgrading is all you need.

## [0.1.34] - 2026-07-30

### New Features

- **Correct diagram sizing** — Mermaid, GraphViz, Vega, and every other non-PlantUML diagram now render at their intended size, roughly twice as large as before. See [Diagram Rendering](docs/diagrams.md).
- **One page identity** — a page's title, description, and kind resolve once from what the page declares, so the navigation sidebar, the page itself, and search results agree. See [Page Metadata](docs/metadata.md#title-resolution).
- **Project-directory targeting** — `rw serve --project-dir <dir>` points rw at a project you are not in, rooting configuration, docs, and `.rw/` at that directory. See [Configuration](docs/configuration.md).
- **Isolated diagrams** — each diagram renders inside its own shadow root, so diagrams on one page no longer borrow each other's clip paths, gradients, and markers. See [Diagram Rendering](docs/diagrams.md).

### Added

- `--project-dir <dir>` on `rw serve` and `rw backstage publish` points `rw` at a project you are not in, rooting configuration, the docs directory, `.rw/`, and PlantUML includes at `<dir>`. Long-form only, and conflicts with `-c`/`--config`. See [Configuration](docs/configuration.md).
- `rw confluence render` now renders `::::tabs`/`:::tab[Label]` groups as bold-label sections — one per tab, all visible — instead of leaving the syntax as literal text.

### Changed

- **Breaking (pre-1.0):** a tab group is now an outer `::::tabs` container wrapping self-closing `:::tab[Label]` items, replacing the form where several `:::tab` shared one closing `:::`. Migrate `:::tab[A] … :::tab[B] … :::` to `::::tabs` / `:::tab[A] … :::` / `:::tab[B] … :::` / `::::`. Rendered HTML is unchanged.

### Removed

- **Breaking (pre-1.0):** the page metadata `vars` field is gone from `meta.yaml`, `<name>.meta.yaml` sidecars, frontmatter, the page API response, `@rwdocs/core`'s napi bindings, and the viewer's TypeScript types. Nothing ever read it. A file that still sets `vars` keeps loading; rw ignores the key.
- **Breaking (pre-1.0):** the diagram `dpi` setting is gone: `[diagrams] dpi` in `rw.toml`, `--dpi` on `rw confluence render`, and `diagrams.dpi` in `@rwdocs/core`'s `createSite()`. Removing it does not itself change any diagram; the sizing fixes below do. An `rw.toml` that still sets it keeps loading.
- **Breaking (pre-1.0):** `rw serve` no longer sends `ETag` or `Last-Modified` on page responses, and no longer answers `If-None-Match` with `304 Not Modified`. Every page request returns a full `200`. The render cache and `Cache-Control: no-cache` are unchanged; published bundles and S3-served pages are unaffected.
- **Breaking (pre-1.0):** `--source-dir`/`-s` on `rw serve` and `rw backstage publish` is gone; `--project-dir <dir>` replaces it and roots the whole project rather than only the markdown. `-s` is now unused. To move only the markdown, set `[docs] source_dir` in `rw.toml`.
- `[live_reload] watch_patterns` in `rw.toml` is gone. rw parsed it but never read it, so setting it never changed which files live reload watched. A config that still sets it keeps loading.

### Fixed

- A site whose `docs.source_dir` is nested, absolute, or the project root itself (`"."`) now finds its `README.md` homepage at the project root. Sites using the default `source_dir = "docs"` are unaffected.
- `@rwdocs/core`'s `createSite({ projectDir })` now roots configuration and every path derived from it at `projectDir` when that directory has no `rw.toml`, instead of picking up an unrelated project from the process's working directory. A `projectDir` that does not exist now throws.
- A page served from an S3 bundle whose `manifest.json` records an unusable modification time no longer crashes. rw reports the time as the Unix epoch and renders the page. In `@rwdocs/core` the panic could take down the host process.
- A PlantUML `!include` preceded by a blank line no longer double-spaces every line of the included file. An indented `!include` keeps its indentation.
- **Mermaid, GraphViz, Vega, and every other non-PlantUML diagram now render at their intended size — roughly twice as large as before.** rw was shrinking them to half size, leaving labels close to unreadable. PlantUML and C4 diagrams are unchanged. `rw serve` re-renders on upgrade; an S3-served site picks it up as each page is next edited, or at once if you delete its `cache/pages/` prefix. Pages laid out around the smaller size may need revisiting.
- Diagrams written with `{format=png}` no longer render at roughly twice their intended size; they now carry their display width and height, which also stops the page reflowing as they decode. SVG diagrams (the default) were never affected, and rw sizes a cached PNG without re-fetching it from Kroki.
- A page that has both `X.md` and `X/index.md` now uses `X/index.md` everywhere, instead of showing one file's title in the navigation sidebar while serving the other file's content. `rw` still warns when it finds such a pair.
- Editing a `README.md` homepage (a project with no `docs/index.md`) now updates the navigation sidebar to the README's real title on live reload, instead of replacing it with "Home".
- Diagrams on the same page no longer borrow each other's clip paths, gradients, and markers; each one now renders inside its own shadow root. A script or test that reached a diagram's SVG in `@rwdocs/core`'s `renderPage()` output with `querySelector` must now go through the wrapper's `shadowRoot`. An S3-served site picks it up as each page is next edited, or at once if you delete its `cache/pages/` prefix. See [Diagram Rendering](docs/diagrams.md).
- Resolving the inline comment you're navigating on and pressing `n` now steps to the next comment instead of jumping back to the first; `p` steps back instead of jumping to the last.
- `@rwdocs/core`'s `renderSearchDocument()` no longer runs a diagram's last word into the heading or paragraph that follows it, so both are indexed as separate terms and each is findable on its own.
- The `re-anchored` badge on a comment no longer squeezes the author's name onto extra lines. It now sits in the thread header next to the position counter, shortened to `fuzzy`, and carries an accessible name for screen readers.
- rw now reads only a repository's own git config when resolving git-based modification times — used by `rw backstage publish` and `@rwdocs/core`'s `mtimeSource: "git"` — so mtimes no longer depend on the environment the process was launched from.
- A page's title — in the page response, browser tab, and search results — is now its declared title (frontmatter, else `meta.yaml`, else the first heading), matching what navigation already used. See [Page Metadata](docs/metadata.md#title-resolution).
- A page's title is now always present in `rw serve`'s page response and `@rwdocs/core`'s `renderPage()`; previously it was absent for a page with no heading.
- A page whose filename titlecases to nothing (`_.md`, `-.md`, `.md`) now gets a real title — the filename stem, or `Untitled` — instead of an empty one in the navigation sidebar, breadcrumbs, and `[[wikilink]]` text.
- Page responses now report a `description` and `kind` declared in frontmatter; previously only `meta.yaml` reached them, so a frontmatter declaration was silently dropped.
- Search now indexes the text of a page's first heading, instead of consuming it as the title and dropping its words from the indexed text.

## [0.1.33] - 2026-07-12

### Added

- `@rwdocs/core`'s `RwSite.listPages()` entries now also carry `path` (the page's site path — the form `renderPage`/`renderSearchDocument`/`getPageMarkdown` take), `hasContent` (`false` for a virtual directory page, which has no body to render), and `anchors` — every section enclosing the page, innermost first with the root last, each paired with the page's path relative to *that* section. A host indexing a whole site can now find the nearest enclosing catalog entity and a page path relative to it from one call, instead of joining `listPages()` with `listSections()` and re-deriving rw's path semantics. Purely additive: `anchors[0]` is the page's existing `(sectionRef, subpath)` identity.

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
