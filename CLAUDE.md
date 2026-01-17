# CLAUDE.md

Tasks are in @TASKS.md.

After code changes:
- Update @CHANGELOG.md
- Check @CLAUDE.md and @README.md for outdated or missing information and fix

## Project Overview

Docstage is a documentation engine for Backstage. It converts CommonMark
documents to HTML and serves them via API. Currently supports Confluence storage
format output with PlantUML diagram support.

**Tagline:** "Where documentation takes the stage"

## Development Commands

```bash
make build                # Build frontend and reinstall Python package
make test                 # Run all tests with coverage (Rust, Python, Frontend)
make format               # Format all code (Rust, Python, Frontend)
make lint                 # Lint all code (clippy, ruff, ty, svelte-check)

# Frontend dev server
cd frontend && npm run dev
```

## Architecture

```
crates/
└── docstage-core/         # Pure Rust library (no PyO3)
    └── src/
        ├── lib.rs                # Module exports
        ├── confluence.rs         # Event-based pulldown-cmark → Confluence XHTML renderer
        ├── html.rs               # Event-based pulldown-cmark → semantic HTML5 with syntect
        ├── converter.rs          # MarkdownConverter with convert() and convert_html() methods
        ├── kroki.rs              # Parallel diagram rendering via Kroki service
        ├── plantuml_filter.rs    # Iterator adapter: extracts PlantUML, returns placeholders
        └── plantuml.rs           # !include resolution, DPI configuration

packages/
├── docstage/              # Python CLI package (Click)
│   └── src/docstage/
│       ├── cli.py                     # Main CLI commands
│       ├── assets.py                  # Bundled frontend asset discovery
│       ├── server.py                  # aiohttp server with SPA fallback
│       ├── static/                    # Bundled frontend (from npm run build:bundle)
│       ├── confluence/client.py       # Async Confluence REST API client
│       ├── confluence/comment_preservation.py  # DOM-based comment preservation
│       └── oauth.py                   # OAuth 1.0 RSA-SHA1 auth
│
└── docstage-core/         # Python package with PyO3 bindings (maturin)
    ├── Cargo.toml
    ├── pyproject.toml
    ├── src/
    │   └── lib.rs                # #[pymodule], wrapper types
    └── python/
        └── docstage_core/
            ├── __init__.py
            └── __init__.pyi

frontend/                  # Svelte 5 SPA (Vite + Tailwind)
├── src/
│   ├── components/        # Svelte components
│   ├── pages/             # Page components
│   ├── stores/            # Svelte stores (router, navigation, page)
│   ├── api/               # API client
│   └── types/             # TypeScript interfaces
└── dist/                  # Production build output
```

**Data flow (Confluence)**: Markdown → Rust (pulldown-cmark parsing, PlantUML
extraction, Confluence rendering, Kroki diagram rendering) → Python (API calls)
→ Confluence

**Data flow (HTML)**: Markdown → Rust (pulldown-cmark parsing, HTML rendering
with syntax highlighting, ToC generation) → Python (API serving) → Backstage

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.91+
- **Python requirements**: 3.14+
- **PlantUML**: Extracted from code blocks, rendered via Kroki, uploaded as attachments
