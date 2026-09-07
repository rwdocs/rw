# Page Metadata

Pages can have metadata defined in three ways:

1. **Frontmatter** — YAML block at the top of a markdown file, delimited by `---`
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
- `kind` -- page kind (e.g., `domain`, `guide`). Pages with `kind` are registered as sections.
- `name` -- page-local section/catalog and diagram identifier, overriding the path-derived name when `kind` is set (see below).
- `namespace` -- Backstage catalog namespace for the section (see below).
- `pages` -- ordered list of child page slugs for navigation sidebar ordering (directory-level only)

### Migrating legacy metadata

Earlier versions accepted `type` as an alias for `kind`. Rename that key to
`kind`; `type` is now ignored in frontmatter and sidecars.

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
Refpath links such as `[[system:commerce/payments-api]]`, catalog lookup,
scoped navigation, page listings, ancestry, and reverse path lookup all use
that effective identity. `name` does not change navigation ordering slugs,
page titles (including Confluence titles), or URLs, and does not create a
catalog entity.

Names are **page-local and never inherit**. Only a page declaring `kind`
registers an explicit section: `name` alone is retained and validated but does
not create one or rename the implicit homepage section. With `kind`, an explicit
homepage name replaces `root`; this also works with project README frontmatter,
metadata-only pages, and all sidecar forms described below. Without a name,
sections keep their final URL path segment (or `root` at the homepage). Stored
metadata keeps only the declaration, not that fallback.

Valid names are 1–63 ASCII characters, start and end with a letter or digit,
and otherwise contain only letters, digits, `-`, `_`, or `.`. Case and exact
spelling are preserved: no trimming or normalization. The usual scalar
extraction applies first (numbers, booleans, and tagged scalars stringify).
Empty or whitespace strings, `/`, `:`, invalid characters or lengths, lists,
mappings, and null are discarded with a source-attributed `name` warning;
valid sibling fields survive. Valid frontmatter wins over the selected sidecar;
invalid frontmatter leaves a valid sidecar name in place, otherwise the section
falls back to its path-derived name. Diagnostics retain their existing field
order, with `name` appended after `pages`.

The old full ref and diagram include are **not aliases**. Changing an effective
name changes catalog/refpath targets and [comment keys](embedding.md), with no
automatic comment migration, catalog mutation, or redirects. Duplicate full
refs still warn: all pages remain path-addressable, while ref lookup chooses
the lexicographically first section root path. Matching remains exact and
case-sensitive.

To preserve identity across a directory rename, declare the old derived name
before the move and keep kind, namespace, and page-relative subpaths unchanged.
To roll back, revert/remove the override to restore the derived identity;
coordinate external refs, includes, and comment keys first. No data is deleted
or migrated automatically.

### Rust source compatibility and S3 rollout

Adding `name: Option<String>` to public `rw_meta::Meta` is a **pre-1.0 Rust
struct-literal source break**: existing literals must add `name: None` (or use
`Meta::resolve` to parse declarations). HTTP/NAPI/viewer page metadata shapes
are unchanged; their existing section projections carry the effective name.

The flattened Document/S3 wire adds optional `name`, omitted when absent;
no-name serialization is unchanged. S3 manifest `FORMAT_VERSION` stays **1**.
New readers accept old manifests. Old readers accept but ignore `name`, losing
its identity semantics: this is syntax compatibility, not mixed-version
semantic parity. **Upgrade readers before publishing bundles that set name.**
Publication expands filesystem includes while retaining metadata includes,
including ones nested inside expanded files, for reader-time resolution. There
is no automatic deployment or migration.

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

## Diagnostics

Metadata problems never fail the site. The offending data is dropped, a
warning is logged, and everything else loads:

- A wrong-typed field (`title: [a, b]`, `pages: foo`) drops only that field;
  sibling fields survive. Scalar numbers and booleans coerce to strings
  (`title: 42` → `"42"`), as they always have; lists, mappings, and an
  explicit `null` (`title: null`) count as wrong types — dropped with a
  warning, falling back like any absent value. A `pages` list with any entry
  that is a nested list, mapping, or `null` drops the whole field, so
  ordering is never partially applied.
- A source that fails to parse — invalid YAML, or a root that is not a
  mapping — contributes none of its fields; the other source still applies.
- An invalid value in one source falls back to a valid value in the other
  (a wrong-typed frontmatter `title` leaves the sidecar `title` in place),
  then to the usual fallbacks: `title` → H1 → titlecased filename → filename
  stem verbatim → `"Untitled"`; `namespace` → the inherited one, else
  `default`.

Each problem logs one warning line when a page's metadata is freshly
resolved — `rw serve` startup, or a rescan after the file changes, not every
request — prefixed with the file that caused it. Frontmatter problems name
the markdown file; sidecar problems name the sidecar file:

```
docs/index.md: frontmatter `title`: expected a string, found a list
docs/guides/meta.yaml: sidecar `pages`: expected a list, found a string
```

Warnings log at WARN level, so run `rw serve --verbose` (or set
`RUST_LOG=warn`) to see them — the default verbosity hides them.

Diagnostics never reach HTTP responses or published bundles. Unknown keys
remain silently ignored.

## Navigation ordering

By default, pages in the navigation sidebar are sorted alphabetically. Use `pages` to control the order:

```yaml
# docs/guides/meta.yaml
title: Guides
pages:
  - getting-started
  - configuration
  - advanced-topics
```

Entries are bare slugs matching a child file (`getting-started.md`) or subdirectory (`getting-started/`). Listed pages appear first in declared order, unlisted pages appear after sorted alphabetically. Every page always appears in navigation — `pages` controls order, not visibility.

A section's own page is the first entry of that section's navigation. The same holds at the top level: the homepage (`docs/index.md`, or `README.md`) is the first entry in the sidebar. An ordinary directory with no `kind` (like `docs/guides/` above) has no navigation of its own — it stays a parent node in the enclosing tree.

Rules:
- Slug with no matching child: warned and skipped
- Slug matching a section directory (has `kind`): warned and skipped
- Duplicate slugs: warned, first occurrence used
- `pages` in frontmatter overrides `pages` in meta.yaml

## Title resolution

The page title is resolved in this order:

1. `title` from frontmatter
2. `title` from meta.yaml
3. First H1 heading in the markdown content
4. Title-cased filename (e.g., `setup-guide.md` becomes "Setup Guide")

## Inheritance

Metadata does not inherit from parent directories: `title`, `description`,
`kind`, `name`, and `pages` apply only to the page or directory that declares them, not
to anything beneath it. `namespace` is the one exception — it inherits down
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

The suffix follows the configured metadata filename: with the default it is
`<name>.meta.yaml`; if you configure a custom filename such as `config.yml`, the
named form is `<name>.config.yml`.

**Precedence.** If both a directory `meta.yaml` and a sibling `<name>.meta.yaml`
resolve to the same page, the directory form wins.

**`index.<meta_filename>`.** A file named `index.meta.yaml` is treated as the
directory's metadata (identical to a plain `meta.yaml` in that directory), not as
a named sidecar for a page called `index`. It is honored but logs a warning
suggesting you rename it to `meta.yaml`.

## Virtual Pages

Directories with `meta.yaml` but no `index.md` become virtual pages:

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
path-derived fallback, with hyphens replaced by underscores. The example above
uses `systems/sys_payments_api.iuml`; the old
`systems/sys_payments_guide.iuml` no longer resolves to it. An explicitly named
homepage with `kind` is eligible too; unnamed or implicit homepages remain
excluded.

**Existing underscore limitation:** literal underscores in an entity name are
not addressable through these includes, because the provider translates every
underscore back to a hyphen. C4 include users should declare hyphenated names.
This feature adds neither a new encoding nor namespace-qualified include syntax.

Regular includes generate `System()` macros; external includes generate `System_Ext()` macros. Domain/system labels use the page title; service labels use the effective entity name. Descriptions and links to the actual documentation path are unchanged (metadata-only entities have no page link).
