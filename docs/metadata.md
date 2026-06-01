# Page Metadata

Pages can have metadata defined in three ways:

1. **Frontmatter** — YAML block at the top of a markdown file, delimited by `---`
2. **Directory sidecar** — `meta.yaml` in a directory, applying to that directory's page
3. **Named sidecar** — `<name>.meta.yaml`, applying to the page at `<name>` (a sibling `<name>.md`, or a content-less entity with no markdown)

When both a markdown file and its sidecar exist, frontmatter values override the sidecar. Variables (`vars`) are deep-merged at the key level.

## Examples

### Frontmatter

```markdown
---
title: "My Domain"
description: "Domain overview"
kind: domain
vars:
  owner: team-a
---

# Page content starts here
```

### Sidecar file

```yaml
# docs/domain-a/meta.yaml
title: "My Domain"
description: "Domain overview"
kind: domain
vars:
  owner: team-a
  priority: 1
```

## Fields

These fields are available in both frontmatter and meta.yaml:

- `title` -- custom page title (overrides H1 extraction)
- `description` -- page description for display
- `kind` -- page kind (e.g., `domain`, `guide`). Pages with `kind` are registered as sections. Also accepts `type` as an alias.
- `namespace` -- Backstage catalog namespace for the section (see below).
- `vars` -- custom variables (key-value pairs)
- `pages` -- ordered list of child page slugs for navigation sidebar ordering (directory-level only)

### `namespace`

The Backstage catalog namespace this section belongs to. Used to build section
ref strings (`kind:namespace/name`) that map to catalog entities.

Unlike `kind`, `namespace` is **inherited**: set it once in a directory's
`meta.yaml` (or frontmatter) and every page below inherits it. A subtree can
override it with its own `namespace`. When unset, the namespace is `default`.

Valid namespaces are 1–63 characters, start and end with a letter or digit, and
otherwise contain only letters, digits, `-`, `_`, or `.` (the Backstage
namespace charset). An invalid value fails the site load with an error naming
the offending file.

```yaml
# docs/meta.yaml — applies to the whole site
namespace: payments
```

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

Metadata is inherited from parent directories:

- `title` -- never inherited
- `description` -- never inherited
- `kind` -- never inherited
- `namespace` -- inherited (child can override)
- `pages` -- never inherited
- `vars` -- deep merged (child values override parent keys)

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

Named sidecars are **leaf-only**: unlike a directory `meta.yaml`, a
`<name>.meta.yaml` does not cascade `vars` to any descendant pages.

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

| Type | Regular | External |
|------|---------|----------|
| Domain | `systems/dmn_{name}.iuml` | `systems/ext/dmn_{name}.iuml` |
| System | `systems/sys_{name}.iuml` | `systems/ext/sys_{name}.iuml` |
| Service | `systems/svc_{name}.iuml` | `systems/ext/svc_{name}.iuml` |

The `{name}` is derived from the directory name with hyphens replaced by underscores (e.g., `payment-gateway` becomes `payment_gateway`).

Regular includes generate `System()` macros; external includes generate `System_Ext()` macros. Both include the entity's title, description, and a link to its documentation page.
