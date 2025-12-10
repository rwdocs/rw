# RD-006: Structured Documentation and Generated Artifacts

## Problem Statement

Large organizations documenting complex architectural landscapes need to:

1. **Define structure** in their documentation (domains, systems, services, etc.)
2. **Maintain consistency** across hundreds of C4/PlantUML diagrams
3. **Keep metadata synchronized** between diagrams and documentation
4. **Enable navigation** from diagrams back to detailed documentation

### Three-Phase Problem

The problem has three distinct phases:

1. **Metadata Schema** - What fields can entities have? Where is metadata stored?
2. **Entity Discovery** - How does docstage detect typed entities in the docs structure?
3. **Artifact Generation** - How does docstage generate PlantUML includes from entities?

### Use Case Example

A documentation structure with domains, systems, and services:

```
docs/
├── domains/
│   └── billing/
│       ├── info.yaml          # title: "Billing Domain", owner: "billing-team"
│       ├── index.md
│       └── systems/
│           └── invoices/
│               ├── info.yaml  # title: "Invoice Service", tech: ["Python", "PostgreSQL"]
│               └── index.md
```

Should automatically generate PlantUML macros:

```plantuml
!include systems/dmn_billing.iuml
!include systems/sys_invoices.iuml

System_Boundary(billing, "Billing Domain") {
    dmn_billing
    sys_invoices
}
```

---

## Phase 1: Metadata Schema

What fields can entities have? How does docstage know what to expect in `info.yaml`?

### Metadata Proposal 1: Implicit (Convention)

**Concept**: Docstage expects a fixed set of well-known fields. No schema definition
needed.

**Well-known fields**:
```yaml
# info.yaml - all fields optional
title: "Invoice Service"           # Display name
description: "Handles invoices"    # Short description
owner: "billing-team"              # Team/person responsible
tags: ["critical", "pci"]          # Classification tags
tech: ["Python", "PostgreSQL"]     # Technology stack
links:                             # External links
  repo: "https://github.com/..."
  runbook: "https://wiki/..."
```

**Trade-offs**:
- ✅ Zero configuration
- ✅ Immediate productivity: just start writing `info.yaml`
- ✅ Portable: same fields work across projects
- ❌ Fixed: can't add custom fields (or they're ignored)
- ❌ No validation: typos in field names go unnoticed

---

### Metadata Proposal 2: Frontmatter Only

**Concept**: No separate `info.yaml`. All metadata lives in `index.md` frontmatter.

**Example**:
```markdown
---
title: Invoice Service
description: Handles invoice generation and management
owner: billing-team
tech:
  - Python
  - PostgreSQL
---

# Invoice Service

The Invoice Service is responsible for...
```

**Trade-offs**:
- ✅ Single file: no separate `info.yaml` to maintain
- ✅ Standard: frontmatter is widely understood
- ✅ Visible: metadata is right where content is
- ❌ Mixing concerns: metadata and content in same file
- ❌ Complex YAML: nested structures awkward in frontmatter
- ❌ Tooling: some editors don't handle frontmatter well

---

### Metadata Proposal 3: Schema Definition

**Concept**: Define metadata schema per entity type in `docstage.toml`. Docstage
validates metadata against schema.

**Configuration**:
```toml
[schema.domain]
required = ["title"]
optional = ["description", "owner"]

[schema.system]
required = ["title", "owner"]
optional = ["description", "tech", "tags", "links"]

[schema.system.fields.tech]
type = "list"
description = "Technology stack"

[schema.system.fields.links]
type = "dict"
description = "External links (repo, runbook, etc.)"
```

**Trade-offs**:
- ✅ Validation: catch missing/invalid fields early
- ✅ Custom fields: define whatever your org needs
- ✅ Documentation: schema is self-documenting
- ❌ Configuration overhead: must define schema upfront
- ❌ Rigidity: schema changes require updating config
- ❌ Complexity: type system adds learning curve

---

### Metadata Proposal 4: Open Schema with Reserved Fields

**Concept**: Allow any fields in metadata, but reserve a few well-known fields
with special meaning. Custom fields are passed through to generators.

**Reserved fields** (always recognized):
- `title` - Display name (required)
- `description` - Short description
- `type` - Entity type (if using metadata-based discovery)

**Custom fields** (passed through as-is):
```yaml
# info.yaml
title: "Invoice Service"
description: "Handles invoices"
# Custom fields below - docstage doesn't interpret these
owner: "billing-team"
tech: ["Python", "PostgreSQL"]
sla: "99.9%"
cost_center: "CC-1234"
```

**Trade-offs**:
- ✅ Flexible: add any fields without config
- ✅ Simple: no schema to define
- ✅ Future-proof: new fields just work
- ❌ No validation: custom field typos undetected
- ❌ Inconsistent: different entities may use different field names
- ❌ Generator coupling: generators must know about custom fields

---

### Metadata Comparison

| Aspect | Implicit | Frontmatter | Schema | Open |
|--------|----------|-------------|--------|------|
| Zero-config | ✅ | ✅ | ❌ | ✅ |
| Custom fields | ❌ | ❌ | ✅ | ✅ |
| Validation | ❌ | ❌ | ✅ | Partial |
| Single file | ❌ | ✅ | ❌ | ❌ |
| Flexibility | Low | Low | High | High |

---

## Phase 2: Entity Discovery

How does docstage know that `docs/domains/billing` is a "domain" and
`docs/domains/billing/systems/invoices` is a "system"?

### Discovery Proposal 1: Convention-Based (Directory Names)

**Concept**: Entity types are determined by parent directory names. A directory
under `domains/` is a domain, under `systems/` is a system, etc.

**How it works**:
- Reserved directory names define entity types: `domains/`, `systems/`, `services/`
- Any subdirectory of a reserved name becomes an entity of that type
- Metadata loaded from `info.yaml` or frontmatter in `index.md`

**Configuration**:
```toml
[entities]
# Map directory names to entity types
domains = "domain"
systems = "system"
services = "service"
```

**Trade-offs**:
- ✅ Zero configuration for standard layouts
- ✅ Familiar: follows common documentation conventions
- ✅ Self-documenting: structure reveals entity types
- ❌ Rigid: forces specific directory naming
- ❌ No custom types without configuration

---

### Discovery Proposal 2: Metadata-Based (Explicit Type)

**Concept**: Each entity declares its type explicitly in metadata file or frontmatter.

**How it works**:
- Every directory with `info.yaml` (or `index.md` with frontmatter) is an entity
- Type declared via `type: domain` or `type: system` in metadata
- No directory naming conventions required

**Example `info.yaml`**:
```yaml
type: system
title: "Invoice Service"
description: "Handles invoice generation and management"
owner: billing-team
```

**Example frontmatter in `index.md`**:
```markdown
---
type: system
title: Invoice Service
---
# Invoice Service
...
```

**Trade-offs**:
- ✅ Flexible: any directory structure works
- ✅ Explicit: type is clear from metadata
- ✅ Extensible: custom types without configuration
- ❌ Verbose: every entity needs type declaration
- ❌ Error-prone: typos in type field

---

### Discovery Proposal 3: Schema-Based (Pattern Matching)

**Concept**: Define entity detection rules via path patterns in configuration.

**How it works**:
- Configuration defines glob patterns that identify entity types
- Path matching determines entity type
- Metadata still loaded from `info.yaml` or frontmatter

**Configuration**:
```toml
[[entities]]
type = "domain"
pattern = "docs/domains/*"

[[entities]]
type = "system"
pattern = "docs/**/systems/*"

[[entities]]
type = "service"
pattern = "docs/**/services/*"

[[entities]]
type = "api"
pattern = "docs/apis/*"
```

**Trade-offs**:
- ✅ Flexible: any structure, any naming
- ✅ Powerful: complex patterns supported
- ✅ Custom types: define your own entity types
- ❌ Configuration required: doesn't work out of the box
- ❌ Pattern complexity: glob patterns can be confusing

---

### Discovery Comparison

| Aspect | Convention | Metadata | Schema |
|--------|------------|----------|--------|
| Zero-config | ✅ | ❌ | ❌ |
| Flexibility | Low | High | High |
| Custom types | Config | Free | Config |
| Verbosity | Low | High | Medium |
| Error-prone | Low | Medium | Low |

---

## Phase 3: Artifact Generation

Once entities are discovered with their metadata, how does docstage generate
PlantUML includes (or other artifacts)?

### Generation Proposal A: Plugin System

**Concept**: Python plugins receive discovered entities and generate output files.

**How it works**:
- Plugins registered in `docstage.toml`
- Each plugin receives list of typed entities with metadata
- Plugins write generated files to configured output directory

**Configuration**:
```toml
[plugins.c4-includes]
module = "docstage.plugins.c4"
output_dir = "gen/includes"
site_url = "http://localhost:8080"
```

**Plugin receives**:
```python
entities = [
    Entity(type="domain", name="billing", path="domains/billing",
           metadata={"title": "Billing Domain", "owner": "billing-team"}),
    Entity(type="system", name="invoices", path="domains/billing/systems/invoices",
           metadata={"title": "Invoice Service", "tech": ["Python", "PostgreSQL"]}),
]
```

**Trade-offs**:
- ✅ Maximum flexibility: any generation logic
- ✅ Reusable: plugins can be shared
- ✅ Typed API: plugins get structured entity data
- ❌ Complexity: requires Python code
- ❌ Learning curve: plugin API to learn

---

### Generation Proposal B: Template Engine

**Concept**: Define templates per entity type in configuration.

**How it works**:
- Templates defined in config, keyed by entity type
- Each template specifies output path and content patterns
- Simple variable substitution: `{name}`, `{title}`, `{path}`, etc.

**Configuration**:
```toml
[templates.domain]
output = "gen/includes/dmn_{name}.iuml"
content = '''
System(dmn_{name}, "{title}", "{description}", $tags="domain", $link="{url}")
'''

[templates.system]
output = "gen/includes/sys_{name}.iuml"
content = '''
System(sys_{name}, "{title}", "{description}", $link="{url}")
'''

[templates.service]
output = "gen/includes/svc_{name}.iuml"
content = '''
System(svc_{name}, "{title}", "{description}", $tags="service", $link="{url}")
'''
```

**Trade-offs**:
- ✅ Declarative: no code required
- ✅ Transparent: templates visible in config
- ✅ Simple: just variable substitution
- ❌ Limited: complex logic not possible
- ❌ No conditionals: can't handle optional fields elegantly

---

### Generation Proposal C: Built-in Generators

**Concept**: Docstage ships with built-in generators for common use cases.
Enable via configuration.

**How it works**:
- Built-in generators for: C4 PlantUML, Mermaid, JSON export
- Enable and configure via `docstage.toml`
- Opinionated defaults that cover most use cases

**Configuration**:
```toml
[generators.c4]
enabled = true
output_dir = "gen/includes"
site_url = "http://localhost:8080"
# Optional: customize prefix per type
prefixes = { domain = "dmn", system = "sys", service = "svc" }

[generators.json]
enabled = true
output = "gen/entities.json"
```

**Trade-offs**:
- ✅ Zero configuration for common cases
- ✅ Well-tested: built-in code is maintained
- ✅ Simple: just enable what you need
- ❌ Limited: only predefined generators
- ❌ Opinionated: may not fit all naming conventions

---

### Generation Comparison

| Aspect | A: Plugins | B: Templates | C: Built-in |
|--------|-----------|--------------|-------------|
| Flexibility | High | Medium | Low |
| Zero-config | ❌ | ❌ | ✅ |
| Code required | Yes | No | No |
| Custom formats | ✅ | ✅ | ❌ |
| Maintenance | User | User | Docstage |

---

## Recommended Combinations

| Use Case | Metadata | Discovery | Generation |
|----------|----------|-----------|------------|
| Quick start | Implicit | Convention | Built-in |
| Custom structure | Open | Schema | Built-in |
| Custom output | Open | Schema | Templates |
| Maximum flexibility | Schema | Metadata | Plugins |
| Single-file docs | Frontmatter | Metadata | Templates |

## Open Questions

1. Should entity relationships (parent/child) be inferred from path hierarchy?
2. Should generation run once at startup, or watch for changes?
3. Should generated files be committed or gitignored?
4. How to handle metadata inheritance (child inherits parent's owner)?
5. Should there be a CLI command to validate metadata against schema?
