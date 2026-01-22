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
make build                # Build frontend and CLI
make test                 # Run all tests with coverage (Rust, Frontend)
make format               # Format all code (Rust, Frontend)
make lint                 # Lint all code (clippy, svelte-check)

# Run the CLI
cargo build -p docstage && ./target/debug/docstage serve

# Frontend dev server
cd frontend && npm run dev
```

## Architecture

```
crates/
├── docstage/              # CLI binary (clap)
│   └── src/
│       ├── main.rs           # Entry point, CLI setup
│       ├── error.rs          # CLI error types
│       ├── output.rs         # Colored terminal output
│       └── commands/
│           ├── mod.rs        # Command module exports
│           ├── serve.rs      # `serve` command
│           └── confluence/
│               ├── mod.rs         # `confluence` subcommand group
│               ├── update.rs      # `confluence update` command
│               └── generate_tokens.rs  # `confluence generate-tokens` command
│
├── docstage-renderer/     # Reusable markdown renderer library
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── renderer.rs       # Generic MarkdownRenderer<B: RenderBackend>
│       ├── backend.rs        # RenderBackend trait definition
│       ├── code_block.rs     # CodeBlockProcessor trait for extensible code block handling
│       ├── state.rs          # Shared state structs (CodeBlockState, TableState, etc.)
│       ├── html.rs           # HtmlBackend implementation
│       └── util.rs           # heading_level_to_num()
│
├── docstage-confluence/       # Confluence integration (renderer, comment preservation)
│   └── src/
│       ├── lib.rs            # ConfluenceBackend, preserve_comments()
│       ├── error.rs          # CommentPreservationError
│       ├── client/           # Confluence REST API client
│       │   ├── mod.rs        # ConfluenceClient
│       │   ├── pages.rs      # Page operations
│       │   ├── comments.rs   # Comment operations
│       │   └── attachments.rs # Attachment operations
│       ├── oauth/            # OAuth 1.0 RSA-SHA1 authentication
│       │   ├── mod.rs        # OAuth1Auth
│       │   ├── key.rs        # RSA key loading
│       │   ├── signature.rs  # Signature generation
│       │   └── token_generator.rs  # Three-legged OAuth flow
│       └── comment_preservation/  # Comment preservation module
│           ├── mod.rs        # Public API (preserve_comments, PreserveResult)
│           ├── tree.rs       # TreeNode with text_signature, marker detection
│           ├── parser.rs     # XML parser with namespace handling
│           ├── matcher.rs    # Tree matching (80% similarity threshold)
│           ├── transfer.rs   # Marker transfer with global fallback
│           ├── serializer.rs # XML serializer with CDATA support
│           └── entities.rs   # HTML entity conversion
│
├── docstage-diagrams/     # Diagram rendering via Kroki
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── language.rs       # DiagramLanguage, DiagramFormat, ExtractedDiagram
│       ├── processor.rs      # DiagramProcessor (implements CodeBlockProcessor)
│       ├── output.rs         # DiagramOutput, DiagramTagGenerator, tag generators
│       ├── kroki.rs          # Parallel Kroki HTTP rendering
│       ├── plantuml.rs       # !include resolution, DPI configuration
│       └── html_embed.rs     # SVG scaling, placeholder replacement
│
├── docstage-site/         # Site structure and page rendering
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── site.rs           # Site, SiteBuilder, Page, NavItem, BreadcrumbItem
│       ├── site_cache.rs     # SiteCache trait, FileSiteCache, NullSiteCache
│       ├── site_loader.rs    # SiteLoader, SiteLoaderConfig
│       ├── renderer.rs       # PageRenderer, PageRendererConfig, PageRenderResult
│       └── page_cache.rs     # PageCache trait, FilePageCache, NullPageCache
│
├── docstage-core/         # Confluence integration
│   └── src/
│       ├── lib.rs            # Module exports
│       ├── converter.rs      # MarkdownConverter for Confluence XHTML output
│       ├── confluence_tags.rs # ConfluenceTagGenerator (internal)
│       └── updater/          # Confluence page updater
│           ├── mod.rs        # PageUpdater, UpdateConfig
│           ├── executor.rs   # Update workflow implementation
│           ├── result.rs     # UpdateResult, DryRunResult
│           └── error.rs      # UpdateError
│
├── docstage-config/       # Configuration parsing
│   └── src/
│       └── lib.rs            # Config, CliSettings, ConfigError
│
└── docstage-server/       # Native HTTP server (axum)
    └── src/
        ├── lib.rs            # Server configuration and entry point
        ├── handlers/         # API endpoints (config, pages, navigation)
        ├── live_reload/      # File watching and WebSocket broadcasting
        └── static_files.rs   # Static file serving with SPA fallback

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
extraction, Confluence rendering, Kroki diagram rendering, API calls) → Confluence

**Data flow (HTML)**: Markdown → Rust (pulldown-cmark parsing, HTML rendering
with syntax highlighting, ToC generation, HTTP serving) → Backstage

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.91+
- **PlantUML**: Extracted from code blocks, rendered via Kroki, uploaded as attachments
