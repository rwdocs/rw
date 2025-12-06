# RD-001: Docstage Backend

## Overview

Docstage is a documentation engine for Backstage that replaces MkDocs. It converts
CommonMark documents to HTML and serves them via API for consumption by a Backstage
frontend plugin.

**Project evolution:** Docstage evolves from the existing md2conf project, reusing and
extending its Python+Rust architecture.

**Tagline:** "Where documentation takes the stage"

## Problem Statement

Current MkDocs-based documentation site has the following issues:

1. **Slow build times** - Building the entire site takes several minutes, slowing down
   the authoring feedback loop.

2. **Limited layouts** - MkDocs provides a single layout paradigm that doesn't fit
   varied content types (usage guides, ADRs, architecture diagrams, API specs).

3. **Rigid structure** - Difficult to integrate deeply with Backstage's component
   catalog and navigation.

4. **No on-demand rendering** - All pages must be pre-built, even if never viewed.

## Goals

1. On-demand page rendering with file-based caching.
2. Hybrid API response: JSON metadata + rendered HTML content.
3. File system-based navigation structure.
4. Live reload support for local documentation authoring.
5. Container-based deployment.

## Non-Goals (This RD)

- Backstage frontend plugin implementation (separate RD).
- Custom tag system (future extension).
- PDF/export functionality.
- Multi-language support.
- Frontmatter parsing (YAML metadata extraction) - will be added in a later phase.
- Search integration via Backstage collator - will be added after core features are
  complete.

## Architecture

### High-Level Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Docs Source (Git repository)                                               │
│  └── docs/                                                                  │
│      ├── domain-a/                                                          │
│      │   ├── index.md                                                       │
│      │   └── subdomain/                                                     │
│      │       └── guide.md                                                   │
│      └── domain-b/                                                          │
│          └── api.md                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Docstage Backend                                                           │
│                                                                             │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐         │
│  │  aiohttp        │───▶│  Rust Core      │───▶│  File Cache     │         │
│  │  (Python)       │    │  (PyO3)         │    │  (.cache/)      │         │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘         │
│           │                                                                 │
│           ▼                                                                 │
│  ┌─────────────────┐                                                        │
│  │  Navigation     │                                                        │
│  │  Builder        │                                                        │
│  └─────────────────┘                                                        │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Backstage                                                                  │
│                                                                             │
│  ┌─────────────────┐    ┌─────────────────┐                                 │
│  │  Docstage       │    │  Search         │                                 │
│  │  Plugin         │    │  Collator       │                                 │
│  └─────────────────┘    └─────────────────┘                                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

#### Rust Core (docstage-core)

Evolved from md2conf-core. Handles:

- CommonMark parsing via pulldown-cmark
- HTML rendering with syntax highlighting
- Table of contents generation
- PlantUML/diagram support via Kroki (existing functionality)

#### Python Backend (docstage)

Evolved from md2conf. Handles:

- aiohttp-based HTTP API
- File system navigation builder
- File-based caching layer
- WebSocket for live reload

**Dual-mode architecture:** Docstage operates as both:

1. **Standalone server** - Run directly with built-in aiohttp web server for local
   development and simple deployments.
2. **Library** - Import as a package into your microservice SDK, exposing core
   functionality without the web layer.

### Directory Structure (Target)

```
packages/
├── docstage/                    # Python backend (renamed from md2conf)
│   └── src/docstage/
│       ├── __init__.py
│       ├── server.py            # aiohttp application (standalone mode)
│       ├── api/
│       │   ├── __init__.py
│       │   ├── pages.py         # Page rendering endpoints
│       │   └── navigation.py    # Navigation endpoints
│       ├── core/
│       │   ├── __init__.py
│       │   ├── renderer.py      # Rust core wrapper
│       │   ├── cache.py         # File-based cache
│       │   └── navigation.py    # Nav tree builder
│       ├── live/
│       │   ├── __init__.py
│       │   └── reload.py        # WebSocket live reload
│       └── config.py            # Configuration
│
└── docstage-core/               # Rust core (renamed from md2conf-core)
    └── src/
        ├── lib.rs
        ├── confluence.rs        # Confluence storage format (existing)
        ├── html.rs              # HTML rendering (new)
        ├── toc.rs               # Table of contents generation
        ├── kroki.rs             # Diagram rendering (existing)
        └── python.rs            # PyO3 bindings
```

## API Design

### GET /api/pages/{path}

Returns rendered page with metadata.

**Request:**

```
GET /api/pages/domain-a/subdomain/guide
```

**Response:**

```json
{
    "meta": {
        "title": "Setup Guide",
        "path": "/domain-a/subdomain/guide",
        "source_file": "docs/domain-a/subdomain/guide.md",
        "last_modified": "2025-12-05T10:30:00Z"
    },
    "breadcrumbs": [
        {"title": "Domain A", "path": "/domain-a"},
        {"title": "Subdomain", "path": "/domain-a/subdomain"},
        {"title": "Setup Guide", "path": "/domain-a/subdomain/guide"}
    ],
    "toc": [
        {"level": 2, "title": "Prerequisites", "id": "prerequisites"},
        {"level": 2, "title": "Installation", "id": "installation"},
        {"level": 3, "title": "Docker", "id": "docker"}
    ],
    "content": "<article><h1>Setup Guide</h1><p>...</p></article>"
}
```

**Title extraction:** The page title is extracted from the first `<h1>` element in the
markdown content. This is already implemented in `docstage-core` via the
`extract_title` option.

### GET /api/navigation

Returns full navigation tree.

**Response:**

```json
{
    "items": [
        {
            "title": "Domain A",
            "path": "/domain-a",
            "children": [
                {
                    "title": "Overview",
                    "path": "/domain-a"
                },
                {
                    "title": "Subdomain",
                    "path": "/domain-a/subdomain",
                    "children": [...]
                }
            ]
        }
    ]
}
```

### GET /api/navigation/{path}

Returns navigation subtree for a specific section.

### WebSocket /ws/live-reload

For local development. Notifies connected clients when source files change.

**Message (server → client):**

```json
{
    "type": "reload",
    "path": "/domain-a/subdomain/guide"
}
```

## Caching Strategy

### File-Based Cache

```
.cache/
├── pages/
│   └── domain-a/
│       └── subdomain/
│           └── guide.html       # Rendered HTML
├── meta/
│   └── domain-a/
│       └── subdomain/
│           └── guide.json       # Extracted metadata
└── navigation.json              # Full nav tree
```

### Cache Invalidation

1. **On request:** Compare source file mtime with cache file mtime.
2. **On file change (dev mode):** File watcher invalidates specific cache entries.
3. **Full rebuild:** DELETE /api/cache endpoint for manual invalidation.

### Cache Headers

API responses include:

- `ETag` based on source file hash
- `Last-Modified` from source file mtime
- `Cache-Control: private, max-age=60` (configurable)

## Configuration

```toml
# docstage.toml

[server]
host = "0.0.0.0"
port = 8080

[docs]
source_dir = "./docs"
cache_dir = "./.cache"

[rendering]
syntax_theme = "github-dark"
kroki_url = "https://kroki.io"

[live_reload]
enabled = true  # Disable in production
watch_patterns = ["**/*.md", "**/*.png", "**/*.svg"]
```

## Implementation Plan

### Phase 1: Project Restructure (Done)

1. ~~Rename `md2conf` package to `docstage`.~~
2. ~~Rename `md2conf-core` crate to `docstage-core`.~~
3. ~~Update all imports, pyproject.toml, Cargo.toml.~~
4. ~~Verify existing functionality still works.~~

### Phase 2: Rust Core - HTML Renderer

Add HTML renderer alongside existing Confluence renderer.

1. Create `HtmlRenderer` module (new, parallel to `ConfluenceRenderer`):
    - Produce semantic HTML5 (`<article>`, `<section>`, `<pre><code>`)
    - Add syntax highlighting via syntect
    - Generate heading IDs for anchor links

2. Create `HtmlConvertResult` with table of contents:
    - Return `Vec<TocEntry>` with `{level, title, id}`
    - Extract during rendering pass (single traversal)

3. Add `MarkdownConverter::convert_html()` method (keep existing `convert()` for
   Confluence output).

4. Update Python bindings to expose both renderers.

### Phase 3: Python Backend - Core Library

Build the library layer independent of web framework.

1. Create `docstage.core.renderer` module:
    - Wrap Rust converter with caching logic
    - Handle file reading and mtime tracking

2. Create `docstage.core.navigation` module:
    - Build navigation tree from directory structure
    - Support `index.md` as section landing page
    - Extract titles from first H1 of each document

3. Create `docstage.core.cache` module:
    - File-based cache with mtime invalidation
    - Separate caches for rendered HTML and metadata

### Phase 4: Python Backend - HTTP API

Add aiohttp server layer for standalone mode.

1. Create `docstage.server` module:
    - aiohttp application factory
    - Route registration

2. Implement `/api/pages/{path}` endpoint:
    - Call renderer, return JSON response
    - Set appropriate cache headers

3. Implement `/api/navigation` endpoints:
    - Full tree and subtree variants

4. Keep Click CLI for local commands (`convert`, `serve`).

### Phase 5: Live Reload

Add development-time live reload support.

1. Add WebSocket endpoint `/ws/live-reload`.
2. Integrate `watchfiles` for file system monitoring.
3. Broadcast reload events on markdown file changes.

### Phase 6: Containerization

1. Create Dockerfile with multi-stage build.
2. Add health check endpoint (`/health`).
3. Create docker-compose.yml for local development.
4. Document deployment to Kubernetes.

## Technical Decisions

### Why aiohttp?

- Already used in existing codebase - consistency with team's stack.
- Native async support for file I/O and WebSocket.
- Mature, battle-tested library.
- Future microservice wrapper will use internal SDK tooling; aiohttp integrates cleanly.
- Lightweight - no unnecessary abstractions for this use case.

### Why File-Based Cache over Redis/In-Memory?

- Simpler deployment (no external dependencies).
- Survives container restarts.
- Easy to inspect and debug.
- Sufficient for documentation workloads.

### Why File System Navigation over Config File?

- Zero configuration for authors.
- Directory structure is intuitive.
- Supports `index.md` convention for section landing pages.
- Can be enhanced with frontmatter overrides later.

### Dual Renderer Architecture

Docstage supports two output formats via separate renderers:

1. **ConfluenceRenderer** (existing) - Produces Confluence storage format XHTML for
   publishing to Confluence via REST API.

2. **HtmlRenderer** (new) - Produces semantic HTML5 for the Backstage documentation
   engine.

Both renderers share:

- CommonMark parsing via pulldown-cmark
- PlantUML extraction and Kroki diagram rendering
- Title extraction from first H1

This allows continued use of the CLI for Confluence publishing while adding the new
Backstage-focused workflow.

## Dependencies

### Python

- aiohttp (HTTP server)
- click (CLI)
- watchfiles (live reload)
- docstage-core (Rust extension)

### Rust

- pulldown-cmark (CommonMark parsing)
- syntect (syntax highlighting)
- pyo3 (Python bindings)
- ureq, rayon (Kroki requests - existing)

## Success Metrics

1. **Page render time:** < 100ms for cache miss, < 10ms for cache hit.
2. **Cold start time:** < 5 seconds (vs minutes for MkDocs full build).
3. **Memory usage:** < 256MB for 1000+ page documentation site.

## Open Questions

1. **Authentication:** Should the API require authentication, or rely on network-level
   security (internal service)?

2. **Versioning:** How to handle documentation versions (git branches, subdirectories)?

3. **Assets:** How to serve static assets (images, downloads) - through Docstage or
   separate static file server?

## Future Extensions

These features are explicitly deferred to keep the initial scope focused:

1. **Frontmatter parsing** - YAML metadata extraction for custom page properties
   (description, tags, custom layouts).

2. **Search integration** - Backstage search collator endpoint with plain text
   extraction for indexing.

3. **Custom layouts** - Support for different page templates based on content type.

## References

- [Backstage TechDocs](https://backstage.io/docs/features/techdocs/)
- [Markdoc](https://markdoc.dev/)
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
- [aiohttp](https://docs.aiohttp.org/)
