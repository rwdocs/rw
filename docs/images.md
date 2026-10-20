# Images

A markdown image can show a file stored in your project:

```markdown
![Request flow](img/flow.png)
![Logo](/.github/assets/logo.svg)
```

In `rw serve`, the page shows the file. Backstage pages show these images as
broken for now. `rw confluence render` warns about each local image and leaves
uploading the file to you; see
[Confluence Rendering](confluence.md#images-and-raw-html).

## Paths

Paths resolve the way GitHub resolves them in a README:

- A relative path starts from the folder of the page's markdown file. On
  `docs/guide/index.md`, `img/flow.png` is `docs/guide/img/flow.png`, and
  `../shared/flow.png` is `docs/shared/flow.png`.
- A path that starts with `/` starts from the project root (see
  [Configuration](configuration.md)). `/.github/assets/logo.svg` names the
  same file from every page.

When `rw.toml` is in a subfolder of the repository, `/` is that subfolder, and
files above it are outside the project. Unlike MkDocs, `/` is not the docs
folder: write `/docs/img/x.png`, or a relative path.

The file can live anywhere in the project, including folders outside the docs
folder and dot folders such as `.github/assets/`. A path that leaves the
project is not resolved.

Write a space in a file name as `%20` (`img/my%20flow.png`), or wrap the whole
path in angle brackets (`![Flow](<img/my flow.png>)`). A `?query` or
`#fragment` is kept on the URL.

An image with an `https://` source loads from that site, and a `data:` source
shows as written. `rw serve` blocks plain `http://` images, so they show as
broken.

## Supported types

RW serves PNG, JPEG (`.jpg`, `.jpeg`), GIF, WebP, AVIF and SVG files, matching
the extension case-insensitively, and no other file from the project, so
`.env`, `.git/` and other private files stay private. Files larger than 32 MiB
are not served either, and warn as not found.

## Warnings

When an image can't be shown, the page renders with a broken image, and
`rw serve --verbose` logs a warning naming it:

```text
image `../../logo.png` is outside the project
image `diagram.bmp` is not a supported image type
image `docs/img/flow.png` not found
```

The "not found" warning names the file's path in the project; the others name
the image source as written. Only a missing file warns on every load. The other
warnings appear when rw renders the page fresh, so run
`rw serve --verbose --no-cache` to see them for a page you haven't changed.

## Live reload

When you add, edit or delete an image in the docs folder, open pages that show
it update. Live reload doesn't watch the rest of the project, so after changing
an image elsewhere, such as `/.github/assets/logo.svg`, reload the page.

## Raw HTML

Images in raw HTML pass through unchanged, and their paths are not resolved.
That includes the centred logo many READMEs open with,
`<p align="center"><img src=".github/assets/logo.png"></p>`, and `<picture>`.
Use markdown image syntax for project files.
