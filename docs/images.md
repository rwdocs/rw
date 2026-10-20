# Images

A markdown image can show a file from your repository:

```markdown
![Request flow](img/flow.png)
![Logo](/.github/assets/logo.svg)
```

`rw serve` serves the file and the page shows it. Backstage and Confluence
don't show repository images yet; see [Where images work](#where-images-work).

## Paths

Paths resolve the way GitHub resolves them in a README:

- A relative path starts from the folder of the page's markdown file. On
  `docs/guide/index.md`, `img/flow.png` is `docs/guide/img/flow.png`, and
  `../shared/flow.png` is `docs/shared/flow.png`.
- A path that starts with `/` starts from the project root, where `rw.toml`
  is. `/.github/assets/logo.svg` names the same file from every page.

The file can live anywhere in the project, including folders outside `docs/`
and dot folders such as `.github/assets/`. A path that leaves the project is
not resolved.

Write a space in a file name as `%20` (`img/my%20flow.png`), or wrap the whole
path in angle brackets (`![Flow](<img/my flow.png>)`). A `?query` or
`#fragment` stays on the image's URL and does not change which file is shown.

Images with an `http://`, `https://`, `//host/` or `data:` source are left as
written.

## Supported types

| Extension         | Type |
|-------------------|------|
| `.png`            | PNG  |
| `.jpg`, `.jpeg`   | JPEG |
| `.gif`            | GIF  |
| `.webp`           | WebP |
| `.avif`           | AVIF |
| `.svg`            | SVG  |

Extensions are case-insensitive. RW serves no other file from the project, so
`.env`, `.git/` and other private files stay private. Files larger than 32 MiB
are not served either.

## Warnings

When an image can't be shown, the page renders with a broken image, and
`rw serve --verbose` logs a warning naming it:

```text
image `../../logo.png` is outside the project
image `diagram.bmp` is not a supported image type
image `docs/img/flow.png` not found
```

Live reload doesn't watch images yet: after editing one, reload the page.

## Raw HTML

Images in raw HTML, such as `<img src="img/flow.png">` or `<picture>`, pass
through unchanged, and their paths are not resolved. Use markdown image syntax
for repository files.

## Where images work

- **`rw serve`**: yes, including `--embedded`.
- **Backstage**: not yet. Pages show broken images.
- **Confluence**: not yet. `rw confluence render` warns about each local image,
  and you upload the file yourself; see
  [Confluence Rendering](confluence.md#images-and-raw-html).
