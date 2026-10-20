# Confluence Rendering

`rw confluence render` converts markdown into a Confluence-publishable bundle:
the storage-format XHTML body and one PNG per diagram. **It does not talk to
the Confluence REST API.** Use any Confluence CLI you like for the actual
`fetch`/`update`/`upload` calls.

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
| `--config <path>` | auto-discover `rw.toml` | Pick up `[diagrams]` defaults. |
| `--strict` | off | Exit non-zero if any warning was emitted or if any comment could not be re-anchored. |

Stdin handling:

- **TTY** or **empty pipe** → no preservation, renders as a fresh page.
- **Pipe with bytes** → treat as the current page's storage XHTML body,
  carry over `<ac:inline-comment-marker>` tags into the rendered output.

Exit codes:

- `0` — success.
- `1` — render/IO error, or `--strict` with warnings present.
- `2` — usage error: an unknown, missing, or malformed argument.
- `3` — `--out -` with diagrams in the markdown.

## Bundle format

```
<out>/
  page.xhtml              # body only — what the publisher PUTs into body.storage.value
  diagram_<hash>.png      # one file per diagram, named what page.xhtml references
```

The bundle is self-describing — `page.xhtml` is the body the publisher uploads
to `body.storage.value`, and the PNGs are the attachments to upload.
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

An image whose source is an `http://`, `https://`, protocol-relative `//`, or
`data:` URL becomes an external image (`ri:url`).

Any other image source is a local file. The page references it as an
attachment named after the file (`images/a%20b.png` becomes `a b.png`), but the
bundle does not include it. Each one adds a warning, and you upload the file to
the page yourself. Copying it into the bundle directory does not work: the
render deletes every `*.png` there first.

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
  | rw confluence render docs/page.md --out dist/ 2> dist/stderr.log

# Upload body and diagrams.
TITLE=$(grep '^title: ' dist/stderr.log | head -1 | sed 's/^title: //')
confluence update $ID --file dist/page.xhtml --format storage \
  ${TITLE:+--title "$TITLE"}
for png in dist/*.png; do
  confluence attachment-upload $ID --file "$png" --replace
done
```

Stdout mode (no diagrams):

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
  | rw confluence render docs/page.md --out dist/ 2> dist/stderr.log

# Upload body and diagrams.
TITLE=$(grep '^title: ' dist/stderr.log | head -1 | sed 's/^title: //')
cfl page edit $ID --storage --file dist/page.xhtml \
  ${TITLE:+--title "$TITLE"}
for png in dist/*.png; do
  cfl attachment upload --page $ID --file "$png"
done
```

## What the publisher owns

`rw confluence render` is intentionally narrow. The following are out of
scope:

- Auth (PAT, OAuth, basic, etc.)
- Fetching the current page and reading its version
- Optimistic-concurrency conflict handling
- Retries on transient HTTP errors
- Setting the attachment MIME type (today's bundle is PNG-only; the
  publisher can hard-code `image/png`)
- Comment authorship, version messages, page labels

The publisher CLI you choose handles all of those.
