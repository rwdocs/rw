# Images and files

A markdown image can show a file stored in your project, and a link can lead
to one:

```markdown
![Request flow](img/flow.png)
![Logo](/.github/assets/logo.svg)
[API spec](spec.pdf)
```

In `rw serve`, the page shows the file or leads to it. Backstage pages show
these images and links as broken for now. `rw confluence render` copies each one into its bundle as an
attachment; see [Confluence Rendering](confluence.md#images-and-raw-html).

An image naming a diagram source file, such as `![Flow](flow.puml)`, renders
as that diagram; see [Diagrams from files](diagrams.md#diagrams-from-files).

## Paths

Image and link paths resolve the way GitHub resolves them in a README:

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

Paths are case-sensitive on every OS, as on Linux, Git and S3. macOS and
Windows would open `img/logo.png` for `img/Logo.PNG`, but rw treats that file
as missing, so `rw serve` on a Mac breaks the image just as the published site
would, and the warning names the file on disk:

```text
image `docs/img/Logo.PNG` not found; the file on disk is `docs/img/logo.png`
```

Write a space in a file name as `%20` (`img/my%20flow.png`), or wrap the whole
path in angle brackets (`![Flow](<img/my flow.png>)`). A `?query` or
`#fragment` is kept on the URL.

An image with an `https://` source loads from that site, and a `data:` source
shows as written. `rw serve` blocks plain `http://` images, so they show as
broken.

## Supported types

An image shows PNG, JPEG (`.jpg`, `.jpeg`), GIF, WebP, AVIF and SVG files. A
link also leads to PDF, DOCX, XLSX, PPTX, CSV and ZIP files. RW matches the
extension case-insensitively and serves no other file from the project, so
`.env`, `.git/` and other private files stay private. Files larger than 32 MiB
are not served either.

## Links to files

A link to a supported file, such as `[API spec](spec.pdf)` or
`[export](/data/export.csv)`, leads to the file. A PDF or an image opens in a
new tab; DOCX, XLSX, PPTX, CSV and ZIP files download. A link to a markdown file
leads to its page instead, as before. The path resolves as an image's does,
case included (see [Paths](#paths)), so `[spec](Spec.pdf)` doesn't find
`spec.pdf`:

```text
linked file `docs/Spec.pdf` not found; the file on disk is `docs/spec.pdf`
```

`rw serve` serves a PDF, DOCX, XLSX, PPTX, CSV or ZIP file only while some page
links to it, so a stray `customers.csv` in the project stays private.
`rw backstage publish` uploads the files pages link to, as it uploads images.
`rw confluence render` leaves these links as written and warns, since it
doesn't attach the files; see
[Confluence Rendering](confluence.md#images-and-raw-html).

A relative link to a file of another type, such as `[config](config.yaml)`,
stays as written and warns, since rw doesn't serve it. A link without an
extension, such as `[guide](../guide)`, a link to a page with a dot in its
name, such as `[release](../v1.2)`, and a link from `/` to another type, such
as `/api/openapi.yaml`, are left alone without a warning.

## Warnings

When an image can't be shown, the page renders with a broken image, and
`rw serve` logs a warning naming it. A link to a file rw refuses stays as
written and warns too; a link to a missing or oversized file still leads to
the file's URL, which answers "not found", and warns:

```text
image `../../logo.png` is outside the project
image `diagram.bmp` is not a supported image type
image `docs/img/flow.png` not found
link `config.yaml`: rw does not serve `.yaml` files
linked file `docs/spec.pdf` is larger than 32 MiB
```

The "not found" and "larger than" warnings name the file's path in the
project; the others name the source as written. All of them warn on every
load, even when the page comes from the cache. `rw backstage publish` lists
them all, so `--strict` fails on them.

## Live reload

When you add, edit or delete an image in the docs folder, open pages that show
it update. Live reload doesn't watch the rest of the project, so after changing
an image elsewhere, such as `/.github/assets/logo.svg`, reload the page.

## Raw HTML

Images in raw HTML pass through unchanged, and their paths are not resolved.
That includes the centred logo many READMEs open with,
`<p align="center"><img src=".github/assets/logo.png"></p>`, and `<picture>`.
Use markdown image syntax for project files.
