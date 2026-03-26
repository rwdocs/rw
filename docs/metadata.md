# Page Metadata

Pages can have metadata defined in two ways:

1. **Frontmatter** — YAML block at the top of a markdown file, delimited by `---`
2. **Sidecar file** — `meta.yaml` in the same directory as `index.md`

When both exist, frontmatter values override meta.yaml. Variables (`vars`) are deep-merged at the key level.

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
- `vars` -- custom variables (key-value pairs)

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
- `vars` -- deep merged (child values override parent keys)

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
