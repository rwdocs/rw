# Wikilinks

A wikilink names its target by section rather than by file path:
`[[domain:billing::overview]]` is the `overview` page of the `billing` domain,
wherever its directory lives. Moving the section's directory doesn't break the
link, as long as the section's kind, namespace and name stay the same (see
[`name`](metadata.md#name-identity-independent-of-the-documentation-path)). To
link a file by its path, use an ordinary relative link such as
`[Guide](guide.md)`.

A section is a page whose metadata sets [`kind`](metadata.md#fields) (a label
you choose, such as `domain`), together with the pages below it. The implicit
root section holds the homepage and every page outside other sections. Its kind
is literally `section` and its name `root`, unless the homepage sets its own
`kind` or `name`.

## Syntax

For a site where `docs/domains/billing/index.md` declares `kind: domain`,
`rw serve` renders these hrefs:

| Form | Links to | Example | Href |
|------|----------|---------|------|
| `[[kind:name::page]]` | A page in a section | `[[domain:billing::overview]]` | `/domains/billing/overview` |
| `[[kind:name]]` | A section's own page | `[[domain:billing]]` | `/domains/billing` |
| `[[kind:namespace/name::page]]` | A section in a given namespace | `[[domain:default/billing::overview]]` | `/domains/billing/overview` |
| `[[name]]`, `[[name::page]]` | A section of kind `section` | `[[root::guide]]` | `/guide` |
| `[[::page]]` | A page in the current page's section | `[[::overview]]` on a billing page | `/domains/billing/overview` |
| `[[::]]` | The current section's own page | `[[::]]` on a billing page | `/domains/billing` |
| `[[#heading]]` | A heading on this page | `[[#pricing]]` | `#pricing` |
| `[[target\|text]]` | Any of the above, with your link text | `[[domain:billing\|Billing]]` | `/domains/billing` |

- `page` is the page's path inside the section: `overview` for
  `domains/billing/overview.md`, `api/auth` for `domains/billing/api/auth.md`.
- Add `#heading` to any form to link a heading on the target page:
  `[[domain:billing::overview#pricing]]`, or `[[::#pricing]]` for the current
  section's own page.
- To link a root-section page such as `guide.md`, write `[[root::guide]]`, or
  `[[::guide]]` from another root-section page, not `[[guide]]`. Without
  `kind:`, the kind is `section`, so `[[guide]]` looks for a section of kind
  `section` named `guide`, and `[[billing]]` does not find the
  `domain:billing` section either.
- Without `namespace/`, the namespace is the current page's (see
  [`namespace`](metadata.md#namespace)).

## Link text

Without `|text`, the link shows the target page's title. A `[[#heading]]` link shows the heading id with hyphens turned into spaces
(`[[#rate-limits]]` shows `rate limits`).

Text after `|` can use inline markdown: `[[::overview|**the** overview]]`.

Inside a table cell, escape the pipe so the table doesn't split the cell:

```markdown
| Area    | Docs                                 |
|---------|--------------------------------------|
| Billing | [[domain:billing::overview\|Overview]] |
```

## Broken links

A wikilink is broken when its section doesn't exist — unknown kind, name or
namespace — or the section has no such page, as in
`[[domain:billing::missing]]`. Kinds, names and pages match exactly, case
included. A broken link renders as `<a href="#" class="rw-broken-link">` and
shows its `|text`, or else the target as written (`domain:nope::x`). The viewer
draws it in red with a wavy underline.

Each broken link adds a page warning:

```text
wikilink `[[::missing]]`: no page `missing` in section `section:default/root`
wikilink `[[domain:nope::x]]`: no such section
```

`rw serve` logs these warnings, `rw backstage publish` lists them, and
`rw backstage publish --strict` fails on them.

## Where wikilinks work

Wikilinks render in `rw serve` and in `@rwdocs/core`, which renders sites
published with `rw backstage publish`. In an embedded viewer, a link that
leaves the current entity goes where your
[`resolveSectionRefs`](embedding.md#resolvesectionrefs-must-map-the-site-root-ref)
sends it. `rw confluence render` leaves `[[...]]` in the page as plain text.
