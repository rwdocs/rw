# Page Metadata

Pages can have metadata defined in YAML sidecar files (default: `meta.yaml` in the same directory as `index.md`).

## Example

```yaml
# docs/domain-a/meta.yaml
title: "My Domain"
description: "Domain overview"
type: domain
vars:
  owner: team-a
  priority: 1
```

## Fields

- `title` -- custom page title (overrides H1 extraction)
- `description` -- page description for display
- `type` -- page type (e.g., `domain`, `guide`). Pages with `type` are registered as sections
- `vars` -- custom variables (key-value pairs)

## Inheritance

Metadata is inherited from parent directories:

- `title` -- never inherited
- `description` -- never inherited
- `type` -- never inherited
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

Pages with `type` set to `domain`, `system`, or `service` automatically generate PlantUML C4 model includes. Use them in PlantUML diagrams:

````plantuml
!include systems/sys_payment_gateway.iuml
!include systems/ext/sys_yookassa.iuml

Rel(sys_payment_gateway, sys_yookassa, "Processes payments")
````

### Include paths by type

| Type | Regular | External |
|------|---------|----------|
| Domain | `systems/dmn_{name}.iuml` | `systems/ext/dmn_{name}.iuml` |
| System | `systems/sys_{name}.iuml` | `systems/ext/sys_{name}.iuml` |
| Service | `systems/svc_{name}.iuml` | `systems/ext/svc_{name}.iuml` |

The `{name}` is derived from the directory name with hyphens replaced by underscores (e.g., `payment-gateway` becomes `payment_gateway`).

Regular includes generate `System()` macros; external includes generate `System_Ext()` macros. Both include the entity's title, description, and a link to its documentation page.
