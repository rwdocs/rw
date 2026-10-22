# Includes

An `::include` line shows another markdown file of the project in its place,
so text that several pages share lives in one file:

```markdown
Before you start:

::include[_partials/warning.md]

::include[/CONTRIBUTING.md]
```

Includes work in `rw serve`, in `rw confluence render` and in Backstage, from
`projectDir` or from pages `rw backstage publish` puts in S3. Backstage search
indexes included text as part of the page that includes it; see
[Where includes work](#where-includes-work).

## Syntax

`::include[path]` stands on a line of its own. It works at the top level of a
page, in a [tab](tabs.md), in a blockquote, and in a loose list item when it is
indented to the item's text. The same blank-line rules as for tabs apply; see
[When a blank line is needed](tabs.md#when-a-blank-line-is-needed).

The line takes no attributes and no `#section`: `::include[x.md]{shift=1}` and
`::include[x.md#install]` warn and show as written.

## Paths

Paths resolve as [image paths](images.md#paths) do:

- A relative path starts from the folder of the page's markdown file. On
  `docs/guide/index.md`, `_note.md` is `docs/guide/_note.md`.
- A path that starts with `/` starts from the project root:
  `/CONTRIBUTING.md` is the `CONTRIBUTING.md` next to `rw.toml`.

The path is taken literally: write a space as a space, not `%20`. It must name
a `.md` file (in any case) inside the project. A symlink that leads out of the
project or to a file that isn't markdown is refused.

## Partials: `_` files

A file or folder whose name starts with `_`, such as `docs/_partials/warning.md`
or `docs/guide/_note.md`, is not a page: it has no URL and no navigation or
search entry (see [Page Metadata](metadata.md)). Keep text that exists only to
be included there. Any other `.md` file in the project can be included too,
pages and the root `README.md` included.

## How included text renders

The included file renders as if its text were written in place of the line,
with these differences:

- Its front matter is ignored: it sets no title and adds no metadata to the
  page.
- Its headings are ordinary headings, as written. A leading `#` heading does
  not become the page title. The included headings appear in the page's table
  of contents.
- Its links to `.md` files and to [files rw serves](images.md#links-to-files),
  its images, its [diagram files](diagrams.md#diagrams-from-files) and its own
  `::include` lines resolve from the included file's folder, the way GitHub
  shows that file on its own.
  Other relative links, such as `sub/`, are left as written, so the browser
  resolves them from the page's URL.
- [Wikilinks](wikilinks.md) in it resolve as if they were written on the
  including page: `[[::overview]]` is in the including page's section.
- It is a markdown document of its own: its link reference definitions don't
  reach the page, and a `:::` container it opens ends with it.
- Its own `::include` lines are followed too. A file that includes itself,
  directly or through other files, warns and that line shows as written.

Comments left on included text belong to the including page. A partial
included on three pages has three separate sets of comments.

## Warnings

When a file can't be included, the line shows as written, `rw serve` logs a
warning and `rw confluence render` prints one, failing `--strict`:

```text
include `../../x.md` is outside the project
include `notes.txt` is not a markdown file
include `docs/_partials/missing.md` not found
include `docs/_partials/note.md` is past the page's limit of 64 includes
docs/b.md: include `docs/a.md` includes itself through `docs/b.md`
```

A path refused before reading is named as written; the others name the file's
path in the project. A warning raised while rendering an included file starts
with that file's path, as in the last line, or in
`docs/_partials/warning.md: unknown leaf directive '::foo'`.

## Limits

A page shows at most 64 includes, counting a file each time it is included,
and at most 32 MiB of included text in total. rw renders pages when they are
requested, so a page must not cost unbounded work. An include past either limit
warns and shows as written. Includes can nest to any depth.

## Live reload

When you edit, create or delete an included file in the docs folder, open
pages that include it update. Live reload doesn't watch the rest of the
project, so after changing an included file elsewhere, such as
`/CONTRIBUTING.md`, reload the page. A PDF, DOCX, XLSX, PPTX, CSV or ZIP file
that such a file newly links to can answer "not found" until you edit a
markdown file in the docs folder or restart `rw serve`.

## Where includes work

| Where                       | Includes                              |
| --------------------------- | ------------------------------------- |
| `rw serve`                  | Rendered                              |
| Backstage with `projectDir` | Rendered                              |
| Backstage pages from S3     | Rendered (see below)                  |
| `rw confluence render`      | Rendered                              |
| Backstage search            | Indexed as part of the including page |

`rw backstage publish` puts each page's included files, at any depth, in the
page's bundle, so Backstage shows the files as they were when you published.
Republish after editing an included file. `rw backstage publish` warns about an
include that fails, and `--strict` fails on it. Backstage needs `@rwdocs/core`
from the release that added includes or a later one; an older one shows the
`::include` line as written, without a warning.
