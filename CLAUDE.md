# CLAUDE.md

Tasks are in @TASKS.md.

Before committing:
- Run `make format` to format all code

After code changes:
- Update @CHANGELOG.md — only add user-facing changes (new features, behavior changes, bug fixes users would notice). Skip internal refactors, code quality fixes, and clippy cleanups.
- Check @CLAUDE.md and @README.md for outdated or missing information and fix

## Project Overview

RW is a documentation engine with no build step. It renders CommonMark documents
on demand — pages are rendered when requested, not ahead of time. Also publishes
to Confluence and builds static sites for Backstage TechDocs.

## Development Commands

```bash
make build                # Build frontend and CLI
make test                 # Run all tests with coverage including doctests (Rust, Frontend)
make format               # Format all code (Rust, Frontend)
make lint                 # Lint all code (clippy, svelte-check)

# Run the CLI
cargo build -p rw && ./target/debug/rw serve

# Frontend dev server
cd frontend && npm run dev
```

## Architecture

```
crates/
├── rw/                    # CLI binary (clap)
│   └── src/
│       ├── main.rs           # Entry point, CLI setup
│       ├── error.rs          # CLI error types
│       ├── output.rs         # Colored terminal output
│       └── commands/
│           ├── mod.rs        # Command module exports
│           ├── serve.rs      # `serve` command
│           ├── confluence/
│           │   ├── mod.rs         # `confluence` subcommand group
│           │   ├── update.rs      # `confluence update` command
│           │   └── generate_tokens.rs  # `confluence generate-tokens` command
│           ├── backstage/
│           │   ├── mod.rs         # `backstage` subcommand group
│           │   └── publish.rs     # `backstage publish` command
│           └── techdocs/
│               ├── mod.rs         # `techdocs` subcommand group
│               ├── build.rs       # `techdocs build` command
│               └── publish.rs     # `techdocs publish` command
│
├── rw-backstage/          # Backstage integration (S3 bundle publish and storage)
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── format.rs         # Bundle format types (Manifest, PageBundle)
│       ├── s3.rs             # Shared S3 client utilities
│       ├── storage.rs        # S3Storage (Storage trait implementation)
│       └── publisher.rs      # BackstagePublisher (feature = "publish")
│
├── rw-assets/             # Frontend asset serving (embedded + filesystem)
│   └── src/
│       └── lib.rs            # get(), iter(), mime_for() API
│
├── rw-renderer/           # Reusable markdown renderer library
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── renderer.rs       # Generic MarkdownRenderer<B: RenderBackend>
│       ├── backend.rs        # RenderBackend trait definition
│       ├── code_block.rs     # CodeBlockProcessor trait for extensible code block handling
│       ├── bundle.rs         # bundle_markdown() for resolving code block references
│       ├── state.rs          # Shared state structs (CodeBlockState, TableState, etc.)
│       ├── html.rs           # HtmlBackend implementation
│       ├── directive/        # Pluggable directives API (CommonMark syntax)
│       │   ├── mod.rs        # Module exports
│       │   ├── args.rs       # DirectiveArgs parsing ([content]{attrs})
│       │   ├── context.rs    # DirectiveContext (file system access)
│       │   ├── output.rs     # DirectiveOutput (Html/Markdown/Skip)
│       │   ├── replacements.rs  # Single-pass string replacement
│       │   ├── inline.rs     # InlineDirective trait (:name)
│       │   ├── leaf.rs       # LeafDirective trait (::name)
│       │   ├── container.rs  # ContainerDirective trait (:::name)
│       │   ├── parser.rs     # Directive syntax parsing
│       │   └── processor.rs  # DirectiveProcessor coordination
│       ├── tabs/             # Tabbed content blocks
│       │   ├── mod.rs        # Module exports
│       │   ├── directive.rs  # TabsDirective (ContainerDirective impl)
│       │   ├── fence.rs      # FenceTracker for code fence state
│       │   ├── preprocessor.rs  # TabsPreprocessor (legacy API)
│       │   └── processor.rs  # TabsProcessor (legacy API)
│       └── util.rs           # heading_level_to_num()
│
├── rw-confluence/         # Confluence integration
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── backend.rs        # ConfluenceBackend (RenderBackend implementation)
│       ├── renderer.rs       # PageRenderer for Confluence XHTML output
│       ├── tags.rs           # ConfluenceTagGenerator for diagram macros
│       ├── error.rs          # ConfluenceError
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
│       ├── comment_preservation/  # Comment preservation module
│       │   ├── mod.rs        # Public API (preserve_comments, PreserveResult)
│       │   ├── tree.rs       # TreeNode with text_signature, marker detection
│       │   ├── parser.rs     # XML parser with namespace handling
│       │   ├── matcher.rs    # Tree matching (80% similarity threshold)
│       │   ├── transfer.rs   # Marker transfer with global fallback
│       │   ├── serializer.rs # XML serializer with CDATA support
│       │   └── entities.rs   # HTML entity conversion
│       └── updater/          # Confluence page updater
│           ├── mod.rs        # PageUpdater, UpdateConfig
│           ├── executor.rs   # Update workflow implementation
│           ├── result.rs     # UpdateResult, DryRunResult
│           └── error.rs      # UpdateError
│
├── rw-techdocs/           # TechDocs static site generation and S3 publishing
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── builder.rs        # StaticSiteBuilder, BuildConfig, BuildError
│       ├── template.rs       # HTML page template (mirrors Svelte frontend)
│       └── publisher.rs      # S3Publisher, PublishConfig, PublishError
│
├── rw-diagrams/           # Diagram rendering via Kroki
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── language.rs       # DiagramLanguage, DiagramFormat, ExtractedDiagram
│       ├── processor.rs      # DiagramProcessor (implements CodeBlockProcessor)
│       ├── output.rs         # DiagramOutput, DiagramTagGenerator, tag generators
│       ├── kroki.rs          # Parallel Kroki HTTP rendering
│       ├── plantuml.rs       # !include resolution, DPI configuration
│       ├── meta_includes.rs  # MetaIncludeSource trait, C4 macro generation from metadata
│       └── html_embed.rs     # SVG scaling, placeholder replacement
│
├── rw-cache/              # Cache abstraction layer
│   └── src/
│       ├── lib.rs            # Cache/CacheBucket traits, NullCache
│       ├── ext.rs            # CacheBucketExt (typed get_json/set_json/get_string/set_string)
│       └── file.rs           # FileCache (file-based impl with version validation)
│
├── rw-site/               # Site structure and page rendering
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── site.rs           # Site (state management + reload), SiteSnapshot
│       ├── site_state.rs     # SiteState (pure data), NavItem, SectionInfo
│       └── page.rs            # Page, BreadcrumbItem, PageRenderer, PageRendererConfig, PageRenderResult, RenderError
│
├── rw-storage/            # Storage abstraction layer (core traits)
│   └── src/
│       ├── lib.rs            # Public API exports
│       ├── storage.rs        # Storage trait, Document, ScanResult, StorageError
│       ├── event.rs          # StorageEvent, StorageEventKind, WatchHandle, StorageEventReceiver
│       ├── metadata.rs       # Metadata struct (data types only)
│       └── mock.rs           # MockStorage (feature = "mock", for testing)
│
├── rw-storage-fs/         # Filesystem storage backend
│   └── src/
│       ├── lib.rs            # FsStorage implementation, build_document()
│       ├── scanner.rs        # Scanner for document discovery (stack-based iteration)
│       ├── source.rs         # SourceFile, SourceKind (file classification)
│       ├── debouncer.rs      # EventDebouncer for file system events
│       ├── inheritance.rs    # Metadata inheritance (build_ancestor_chain, merge_metadata)
│       └── yaml.rs           # YAML parsing helpers
│
├── rw-napi/               # Node.js native addon (napi-rs bindings)
│   └── src/
│       ├── lib.rs            # RwSite, create_site, render_page, get_navigation
│       └── types.rs          # Napi-compatible response types
│
├── rw-config/             # Configuration parsing
│   └── src/
│       └── lib.rs            # Config, CliSettings, MetadataConfig, ConfigError
│
└── rw-server/             # Native HTTP server (axum)
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
│   ├── lib/               # Utility libraries (tabs.ts)
│   ├── styles/            # Shared CSS (content.css: prose, diagrams, alerts, tabs)
│   ├── types/             # TypeScript interfaces
│   ├── techdocs.ts        # TechDocs CSS entry point (Vite second input)
│   └── techdocs.css       # Self-contained TechDocs styles (td-* classes)
└── dist/                  # Production build output
```

**Data flow (Confluence)**: Markdown → Rust (pulldown-cmark parsing, PlantUML
extraction, Confluence rendering, Kroki diagram rendering, API calls) → Confluence

**Data flow (HTML)**: Markdown → Rust (pulldown-cmark parsing, HTML rendering
with syntax highlighting, ToC generation, HTTP serving) → Browser

**Data flow (TechDocs)**: Markdown → Rust (Site rendering, static HTML generation,
S3 upload) → Backstage TechDocs

**Data flow (NAPI)**: Node.js → rw-napi (napi-rs bindings) → rw-site, rw-renderer,
rw-diagrams (Rust) → Node.js objects

## Key Technical Details

- **Rust requirements**: Edition 2024, Rust 1.91+
- **PlantUML**: Extracted from code blocks, rendered via Kroki, uploaded as attachments
