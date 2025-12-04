# CLAUDE.md

Tasks are in @TASKS.md.

After code changes:
- Update @CHANGELOG.md
- Check @CLAUDE.md and @README.md for outdated or missing information and fix

## Project Overview

md2conf is a markdown-to-Confluence converter with PlantUML diagram support. It
converts markdown documents to Confluence storage format (XHTML) and
creates/updates pages in Confluence Server/Data Center.

## Development Commands

```bash
uv sync --reinstall                           # Rebuild Rust extension
cd packages/md2conf-core && cargo test --lib  # Run Rust unit tests
```

## Architecture

```
packages/
├── md2conf/           # Python CLI package (Click)
│   └── src/md2conf/
│       ├── cli.py                     # Main CLI commands
│       ├── confluence/client.py       # Async Confluence REST API client
│       ├── confluence/comment_preservation.py  # DOM-based comment preservation
│       └── oauth.py                   # OAuth 1.0 RSA-SHA1 auth
│
└── md2conf-core/      # Rust core library (PyO3)
    └── src/
        ├── lib.rs                # Module exports
        ├── confluence.rs         # Event-based pulldown-cmark → Confluence XHTML renderer
        ├── kroki.rs              # Parallel diagram rendering via Kroki service
        ├── plantuml_filter.rs    # Iterator adapter: extracts PlantUML, returns placeholders
        ├── plantuml.rs           # !include resolution, DPI configuration
        └── python.rs             # PyO3 bindings exposing MarkdownConverter class
```

**Data flow**: Markdown → Rust (pulldown-cmark parsing, PlantUML extraction,
Confluence rendering, Kroki diagram rendering) → Python (API calls) → Confluence

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.85+
- **Python requirements**: 3.14+
- **PlantUML**: Extracted from code blocks, rendered via Kroki, uploaded as attachments
