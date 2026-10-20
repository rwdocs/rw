# Confluence Rendering

`rw confluence render` converts markdown into a Confluence-publishable bundle:
the storage-format XHTML body, one PNG per diagram, and a copy of each image
stored in your project. **It does not talk to the Confluence REST API.** Use
any Confluence CLI you like for the actual `fetch`/`update`/`upload` calls.

## Usage

```
rw confluence render <markdown_file> --out <dir|->
```

| Flag | Default | Purpose |
|---|---|---|
| `<markdown_file>` (positional) | required | Markdown source. |
| `--out <dir>` or `--out -` | required | Bundle directory, or `-` for stdout-only mode. |
| `--kroki-url <url>` | from `[diagrams]` config | Override Kroki server. |
| `-I, --include-dir <path>` | from `[diagrams]` config, repeatable | PlantUML `!include` search path. |
| `--no-extract-title` | off (title extracted by default) | Skip extracting the title from the H1 that opens the page, as in [Title resolution](metadata.md#title-resolution) (no `title:` line on stderr). With extraction, that H1 is left out of `page.xhtml` and the headings after it move up one level. |
| `--no-toc` | TOC prepended by default | Skip the `<ac:structured-macro name="toc">`. |
| `-c, --config <path>` | auto-discover `rw.toml` | Pick up `[diagrams]` defaults; the file's directory is the project root. |
| `--project-dir <dir>` | auto-discover `rw.toml` | Root the project at `<dir>`, reading `<dir>/rw.toml` if present. Local images resolve against the project root. |
| `--strict` | off | Exit non-zero if any warning was emitted or if any comment could not be re-anchored. |

Stdin handling:

- **TTY** or **empty pipe** → no preservation, renders as a fresh page.
- **Pipe with bytes** → treat as the current page's storage XHTML body,
  carry over `<ac:inline-comment-marker>` tags into the rendered output.

Exit codes:

- `0` — success.
- `1` — render/IO error, or `--strict` with warnings present.
- `2` — usage error: an unknown, missing, or malformed argument.
- `3` — `--out -` with diagrams in the markdown (local images don't count; see
  [Images and raw HTML](#images-and-raw-html)), or a discovered project root
  with no docs.
- `4` — the `--project-dir` directory or `--config` file does not exist.

rw uses the nearest ancestor containing `rw.toml` as the project root, or the
current directory if there is none; a command fails if that root has no docs
directory (or `README.md` homepage); pass `--project-dir` to choose the root.
A root you name with `--project-dir` or `--config` needs no docs, so
`--project-dir .` renders a markdown file that belongs to no project.

## Bundle format

```
<out>/
  page.xhtml              # body only — what the publisher PUTs into body.storage.value
  diagram_<hash>.png      # one file per diagram, named what page.xhtml references
  image_<hash>.<ext>      # one file per local image, named what page.xhtml references
```

The bundle is self-describing — `page.xhtml` is the body the publisher uploads
to `body.storage.value`, and every other file is an attachment to upload.
`<ext>` is the image's own extension, lowercased: `png`, `jpg`, `jpeg`, `gif`,
`webp`, `avif` or `svg`.

Each render first deletes the `diagram_*.png` and `image_<hash>.<ext>` files in
`<out>`, so an attachment from an earlier render doesn't publish again. It
leaves other files alone, so keep the directory for the bundle only.

Diagnostics from the render run (extracted title, renderer warnings, comments
that could not be re-anchored) go to **stderr** as plain text:

```
title: Page title from the opening H1
warning: diagram 1: HTTP request failed: io: Connection refused (os error 61)
1 comment(s) could not be placed:
  - [comment-ref] "original text the marker was wrapping"
```

A warning that spans several lines, such as a Kroki error message, continues
on lines indented by two spaces.

Publishers scripting against the output can capture stderr alongside the
bundle directory.

## Images and raw HTML

An image whose source has a scheme or a host, such as `https://`, `ftp://`,
`data:`, or protocol-relative `//`, becomes an external image (`ri:url`).

Any other image source is a file in your project, found as in `rw serve` (see
[Images](images.md#paths)). The render copies it into the bundle as
`image_<hash>.<ext>`, named after the file's content, and the page references
that attachment. An image used twice, or two files with the same content,
become one attachment, while two different `logo.png` files in different
folders stay two.

The bundle leaves an image out when the file is missing, outside the project,
not a [supported type](images.md#supported-types), or larger than 32 MiB, when
the markdown file itself is outside the project, and with `--out -`, which has
no directory to copy into. The page then references an attachment named after
the file (`images/a%20b.png` becomes `a b.png`), the render adds a warning,
and you upload the file to the page yourself.

Avoid raw HTML in pages you publish to Confluence. Storage format is XML, and
raw HTML passes through as written, except that void elements such as `<br>`,
`<hr>`, and `<img>` are self-closed when the tag fits on one line. Anything
else that is not well-formed XML makes Confluence reject the page at upload.

If you must use raw HTML, write it as XHTML: close every element, quote every
attribute value, give boolean attributes a value, and escape `&` as `&amp;`.

```html
<p>Terms &amp; conditions</p>
<input type="checkbox" checked="checked" disabled="disabled" />
```

## Recipes

> **Verify your publisher's flag set.** The snippets below cover the
> happy-path of two open-source Confluence CLIs. These tools evolve
> independently — confirm the exact flag names before wiring them into CI.

**As of 2026-05-28, Atlassian's official `acli` does not support Confluence
pages.** The two viable open-source page-CRUD CLIs are documented below.

### `pchuri/confluence-cli` (Node.js)

Project: <https://github.com/pchuri/confluence-cli>

Bundle mode:

```sh
# Render the bundle and capture stderr for diagnostics.
confluence read $ID --format storage \
  | rw confluence render docs/page.md --out dist/ 2> render.log

# Upload the body, then every other file as an attachment.
TITLE=$(grep '^title: ' render.log | head -1 | sed 's/^title: //')
confluence update $ID --file dist/page.xhtml --format storage \
  ${TITLE:+--title "$TITLE"}
for file in dist/*; do
  [ "$file" = dist/page.xhtml ] && continue
  confluence attachment-upload $ID --file "$file" --replace
done
```

Stdout mode (no diagrams, no local images):

```sh
confluence read $ID --format storage \
  | rw confluence render docs/page.md --out - \
  | confluence update $ID --file - --format storage
```

### `open-cli-collective/cfl` (Go)

Project: <https://github.com/open-cli-collective/atlassian-cli>

Bundle mode:

```sh
# Render the bundle and capture stderr for diagnostics.
cfl page view $ID --raw --content-only \
  | rw confluence render docs/page.md --out dist/ 2> render.log

# Upload the body, then every other file as an attachment.
TITLE=$(grep '^title: ' render.log | head -1 | sed 's/^title: //')
cfl page edit $ID --storage --file dist/page.xhtml \
  ${TITLE:+--title "$TITLE"}
for file in dist/*; do
  [ "$file" = dist/page.xhtml ] && continue
  cfl attachment upload --page $ID --file "$file"
done
```

## What the publisher owns

`rw confluence render` is intentionally narrow. The following are out of
scope:

- Auth (PAT, OAuth, basic, etc.)
- Fetching the current page and reading its version
- Optimistic-concurrency conflict handling
- Retries on transient HTTP errors
- Setting each attachment's MIME type from its file extension: `png` is
  `image/png`, `jpg` and `jpeg` are `image/jpeg`, `gif` is `image/gif`, `webp`
  is `image/webp`, `avif` is `image/avif`, and `svg` is `image/svg+xml`
- Comment authorship, version messages, page labels

The publisher CLI you choose handles all of those.
