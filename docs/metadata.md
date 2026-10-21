# Page Metadata

Pages can have metadata defined in three ways:

1. **Frontmatter** — YAML block delimited by `---`. It must be the first thing in the markdown file, with no blank line or HTML comment above it; put such a comment below it. A later `---` block is ordinary markdown
2. **Directory sidecar** — `meta.yaml` in a directory, applying to that directory's page
3. **Named sidecar** — `<name>.meta.yaml`, applying to the page at `<name>` (a sibling `<name>.md`, or a content-less entity with no markdown)

When both a markdown file and its sidecar exist, frontmatter values override the sidecar. A project-root `README.md` used as the homepage is a regular markdown metadata source, so all of its frontmatter fields apply.

## Examples

### Frontmatter

```markdown
---
title: "My Domain"
description: "Domain overview"
kind: domain
---

# Page content starts here
```

### Sidecar file

```yaml
# docs/domain-a/meta.yaml
title: "My Domain"
description: "Domain overview"
kind: domain
```

## Fields

These fields are available in both frontmatter and meta.yaml:

- `title` -- custom page title (overrides H1 extraction)
- `description` -- page description for display
- `kind` -- page kind (e.g., `domain`, `guide`). Pages with `kind` are registered as sections. It must be non-empty and contain no `:`, since it starts the section ref (`kind:namespace/name`); an invalid kind is dropped with a warning (see [Diagnostics](#diagnostics)).
- `name` -- page-local section/catalog and diagram identifier, overriding the path-derived name when `kind` is set (see below).
- `namespace` -- Backstage catalog namespace for the section (see below).
- `attrs` -- opaque, page-local JSON-compatible attributes for integrations (see below)
- `pages` -- ordered list of child page slugs for navigation sidebar ordering (directory-level only)

A `title` or `description` that is empty, only whitespace, or null counts as not set, so the next source applies.

### `name`: identity independent of the documentation path

For `docs/systems/payments-guide/meta.yaml`:

```yaml
kind: system
name: payments-api
namespace: commerce
title: Payments documentation
```

The section ref is `system:commerce/payments-api`, its URL remains
`/systems/payments-guide`, and its title remains `Payments documentation`.
[Wikilinks](wikilinks.md) such as `[[system:commerce/payments-api]]`, catalog
lookup, scoped navigation, page listings, ancestry, and reverse path lookup all
use that effective identity. `name` does not change navigation ordering slugs,
page titles (including Confluence titles), or URLs, and does not create a
catalog entity.

Names are **page-local and never inherit**. Only a page declaring `kind`
registers an explicit section: `name` alone is validated but does
not create one or rename the implicit homepage section. With `kind`, an explicit
homepage name replaces `root`; this also works with project README frontmatter,
metadata-only pages, and all sidecar forms described below. Without a name,
sections keep their final URL path segment (or `root` at the homepage).

Valid names are 1–63 ASCII characters, start and end with a letter or digit,
and otherwise contain only letters, digits, `-`, `_`, or `.`. Case and exact
spelling are preserved: no trimming or normalization. As with other fields,
numbers, booleans, and tagged scalars become strings first. Empty or
whitespace strings, `/`, `:`, invalid characters or lengths, lists, mappings,
and null are dropped with a `name` warning naming the file; valid sibling
fields survive. Valid frontmatter wins over the sidecar;
invalid frontmatter leaves a valid sidecar name in place, otherwise the section
falls back to its path-derived name.

Changing an effective name changes the full ref, so wikilinks, catalog
lookups, diagram includes, and
[comment keys](embedding.md#what-it-does-and-does-not-survive) that use the
previous name stop matching. RW adds no aliases or redirects. Duplicate full refs
warn: all pages remain path-addressable, while ref lookup chooses the
lexicographically first section root path. `rw backstage publish` lists the
warning, so `--strict` fails on it. Matching is exact and case-sensitive.

To keep identity across a directory rename, declare the old derived name
before the move and keep kind, namespace, and each page's subpath unchanged.

### `namespace`

The Backstage catalog namespace this section belongs to. Used to build section
ref strings (`kind:namespace/name`) that map to catalog entities.

Unlike `kind`, `namespace` is **inherited**: set it once in a directory's
`meta.yaml` (or frontmatter) and every page below inherits it. A subtree can
override it with its own `namespace`. When unset, the namespace is `default`.

Valid namespaces are 1–63 characters, start and end with a letter or digit, and
otherwise contain only letters, digits, `-`, `_`, or `.` (the Backstage
namespace charset). An invalid value is dropped with a warning naming the
file; the section falls back to the inherited namespace (`default` if none).

```yaml
# docs/meta.yaml — applies to the whole site
namespace: payments
```

### `attrs`: page-local integration data

Declare attributes in frontmatter or the sidecar, including on the
README homepage and metadata-only pages; `kind` is not required:

```yaml
attrs:
  owner: payments
  audiences: [operators]
  support: {url: https://example.org/support}
  reviewDate: null
```

- **No inheritance.** Attributes belong only to the declaring page, not its
  children. They do not change section identity, namespace, titles, or URLs.
- **Top-level overlay.** Start with the sidecar's attrs; frontmatter overrides
  matching keys. A nested object or array replaces the previous value
  **whole**, without recursive merging.
- **Null is data.** `reviewDate: null` keeps the key with a null value and
  replaces any sidecar value. There is no deletion syntax: to drop a key,
  remove it from every source that declares it.
- **Whole-field validation.** Attrs must be a mapping with string keys and
  JSON-compatible values: strings, booleans, finite numbers, nulls, arrays, and
  objects. Types are preserved, not coerced to strings; keys are
  case-sensitive. Each value may nest arrays and objects at most 123 levels
  deep; every array or object counts as a level, even an empty one. A custom
  YAML tag, a non-string key, a non-finite number, or deeper nesting drops that
  source's **entire `attrs` field** with a warning; its other fields and the
  other source's attrs still apply. A whole-field `attrs: null`, unlike a null
  value inside attrs, is dropped the same way. Nesting past the YAML parser's
  own limit fails the whole source, like any invalid YAML.

Only `@rwdocs/core`'s `renderPage()` exposes resolved attrs, as `meta.attrs`,
omitted when empty (see [Consuming page attrs](embedding.md#consuming-page-attrs)).
The HTTP API, viewer, navigation, search documents, Confluence output, and
rendered HTML ignore them, but `getPageMarkdown()` returns frontmatter, attrs
included, verbatim. Attrs are visible metadata, **not a secret store or an
authorization policy**. JavaScript receives ordinary numbers, so use **strings
for identifiers that need exact integers beyond `Number.MAX_SAFE_INTEGER`**
(`9007199254740991`).

## Diagnostics

Metadata problems never fail the site. The offending data is dropped, a
warning is logged, and everything else loads:

- A wrong-typed field (`title: [a, b]`, `pages: foo`) drops only that field;
  sibling fields survive. Scalar numbers and booleans coerce to strings
  (`title: 42` → `"42"`); lists and mappings count as wrong types — dropped
  with a warning, falling back like any absent value.
  So does an explicit `null` in `kind`, `namespace`, or `name`; a null
  `title` or `description` is simply not set. A `pages` list with any entry
  that is a nested list, mapping, or `null` drops the whole field, so
  ordering is never partially applied.
- A source that fails to parse — invalid YAML, or a root that is not a
  mapping — contributes none of its fields; the other source still applies.
- An invalid value in one source falls back to a valid value in the other
  (a wrong-typed frontmatter `title` leaves the sidecar `title` in place),
  then to the usual fallbacks: `title` → H1 → titlecased name (see
  [Title resolution](#title-resolution)) → that name verbatim →
  `"Untitled"`; `namespace` → the inherited one, else
  `default`.

Each problem logs one warning line when it appears — at `rw serve` startup,
or on the rescan after the edit that causes it — prefixed with the file that
caused it. Frontmatter problems name the markdown file; sidecar problems name
the sidecar file:

```
docs/index.md: frontmatter `title`: expected a string, found a list
docs/guides/meta.yaml: sidecar `pages`: expected a list, found a string
```

`rw backstage publish` lists them with its other warnings, so `--strict`
fails on them.

Diagnostics never reach public responses or published bundles. Unknown top-level
metadata keys are silently ignored; keys inside `attrs` are kept as data.

## Navigation ordering

By default, pages in the navigation sidebar are sorted by slug (the file or directory name), not by title. Use `pages` to control the order:

```yaml
# docs/guides/meta.yaml
title: Guides
pages:
  - getting-started
  - configuration
  - advanced-topics
```

Entries are bare slugs matching a child file (`getting-started.md`) or subdirectory (`getting-started/`). Listed pages appear first in declared order, unlisted pages appear after, sorted by slug. Every page always appears in navigation — `pages` controls order, not visibility.

A file or folder whose name starts with `_`, such as `docs/_partials/warning.md` or `docs/guides/_note.md`, is not a page: it holds text meant to be included in other pages. It has no URL and no navigation or search entry, and a `meta.yaml` inside a `_` folder or a `_<name>.meta.yaml` makes no page either. A slug in `pages` naming one has no matching child. Names starting with `.` are left out the same way.

A page in a directory with no page of its own (`docs/guides/old/faq.md` with no `docs/guides/old/index.md` or other `guides/old` page) is listed under the nearest page above it. Its slug there is the rest of its path, `old/faq`, both for sorting and in `pages`.

A section's own page is the first entry of that section's navigation. The same holds at the top level: the homepage (`docs/index.md`, or `README.md`) is the first entry in the sidebar. An ordinary directory with no `kind` (like `docs/guides/` above) has no navigation of its own — it stays a parent node in the enclosing tree.

Rules:
- Slug with no matching child: warned and skipped
- Slug matching a section directory (has `kind`): warned and skipped
- Duplicate slugs: warned, first occurrence used
- `pages` in frontmatter overrides `pages` in meta.yaml

Each of these warnings is logged once, when it appears, prefixed with the
page whose `pages` names the slug, like
[metadata problems](#diagnostics). `rw backstage publish` lists them too, so
`--strict` fails on them:

```
/guides: slug `old` in `pages` has no matching child, skipping
```

## Title resolution

The page title is resolved in this order:

1. `title` from frontmatter
2. `title` from meta.yaml
3. The H1 heading that opens the markdown content. HTML comments above it,
   such as `<!-- markdownlint-disable MD041 -->`, don't count; an H1 below
   anything else is an ordinary heading. The title is the heading's plain
   text: a `:status[OK]` badge gives its label, `OK`, an image gives nothing,
   and a [wikilink](wikilinks.md) `[[target]]` gives the target as written, because the linked page's
   title isn't known yet. Write `[[target|text]]` to choose the text
4. Title-cased name: the filename for a standalone page (`setup-guide.md`
   becomes "Setup Guide"), the directory name for `index.md`, `README.md`, and
   virtual pages (`setup-guide/index.md` also becomes "Setup Guide"), and "Home" for the
   homepage

A `title` that is empty, only whitespace, or null counts as not set, and so
does a blank H1; resolution moves on to the next step.

## Inheritance

Metadata does not inherit from parent directories: `title`, `description`,
`kind`, `name`, `attrs`, and `pages` apply only to the page or directory that
declares them, not to anything beneath it. `namespace` is the one exception — it inherits down
the tree, as described above.

## Named sidecar files (`<name>.meta.yaml`)

A `<name>.meta.yaml` file declares metadata for the page at `<name>`, the same
way `<name>.md` declares content there. Two uses:

- **Content-less entities** — register a Backstage component or system that
  exists only to build catalog relations, without creating a subfolder:

  ```
  systems/
    payments.meta.yaml   # kind: component — no subfolder, no markdown
    billing.meta.yaml
  ```

- **Sidecar for a standalone page** — attach metadata to an existing
  `guide.md` by placing `guide.meta.yaml` beside it.

**Precedence.** If both a directory `meta.yaml` and a sibling `<name>.meta.yaml`
resolve to the same page, the directory form wins.

**`index.meta.yaml`.** A file named `index.meta.yaml` is treated as the
directory's metadata (identical to a plain `meta.yaml` in that directory), not as
a named sidecar for a page called `index`. It is honored but logs a warning
suggesting you rename it to `meta.yaml`.

## Virtual Pages

Directories with `meta.yaml` but no `index.md` or `README.md` become virtual pages:

- Appear in navigation with their metadata title
- Render h1 with title only (no content body)
- Support nested virtual pages for organizing content hierarchies

Example structure:

```
docs/
├── index.md           # Home page
├── domains/
│   ├── meta.yaml      # Virtual page: "Domains"
│   ├── billing/
│   │   ├── meta.yaml  # Virtual page: "Billing"
│   │   └── api.md     # Real page under Billing
│   └── users/
│       └── index.md   # Real page (has index.md)
```

## Diagram Includes

Pages with `kind` set to `domain`, `system`, or `service` automatically generate PlantUML C4 model includes. Use them in PlantUML diagrams:

````plantuml
!include systems/sys_payment_gateway.iuml
!include systems/ext/sys_yookassa.iuml

Rel(sys_payment_gateway, sys_yookassa, "Processes payments")
````

### Include paths by kind

| Kind | Regular | External |
|------|---------|----------|
| Domain | `systems/dmn_{name}.iuml` | `systems/ext/dmn_{name}.iuml` |
| System | `systems/sys_{name}.iuml` | `systems/ext/sys_{name}.iuml` |
| Service | `systems/svc_{name}.iuml` | `systems/ext/svc_{name}.iuml` |

The `{name}` is the effective section name: declared `name`, otherwise the
path-derived fallback, with hyphens replaced by underscores. The
[`name` example](#name-identity-independent-of-the-documentation-path) is
included as `systems/sys_payments_api.iuml`, not
`systems/sys_payments_guide.iuml`. An explicitly named homepage with `kind` is
eligible too; unnamed or implicit homepages are not.

A real file at the include path, found as [other includes](diagrams.md#plantuml-includes)
are, wins over metadata: the include expands from metadata only when no such
file exists.

**Underscore limitation:** literal underscores in an entity name are not
addressable through these includes, because the provider translates every
underscore back to a hyphen. Declare hyphenated names.

Regular includes generate `System()` macros; external includes generate `System_Ext()` macros. Domain/system labels use the page title; service labels use the effective entity name. Descriptions come from `description`, and links point to the page's URL (metadata-only entities have no link).

`rw confluence render` expands them too, for a file in the docs directory or
the `README.md` homepage. Any other file keeps them as written.

### Ambiguous include names

Metadata includes look up **kind and name without a namespace**. For example,
`system:billing/shared` and `system:shipping/shared` both match
`!include systems/sys_shared.iuml`. RW picks the entity whose document path
sorts first, so adding or moving a page can change which one appears. Give these
entities **distinct explicit `name` values** and update their includes;
namespace-qualified includes are not supported.

When a lookup finds matches across namespaces, RW logs one WARN listing every
matching full ref and logical document path (`/` for the homepage), ordered by
path; the first one listed is the one selected. Duplicates within one
namespace get only the duplicate-full-ref warning, not an include warning. A
group spanning namespaces lists all matching paths, including any duplicate
full refs.

This warning is logged each time an include is resolved, not when the site
loads, so one page can log it more than once. A page served from the render
cache resolves no includes and logs nothing. `renderSearchDocument()` in
`@rwdocs/core` also resolves includes and can log it.

Because cached pages hide it, the absence of a warning does not prove names
are unique.
