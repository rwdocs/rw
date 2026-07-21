# RW

A documentation engine with no build step. Point it at a directory of markdown files
and start serving — pages are rendered when requested, not ahead of time.
One file changed? Only that page is re-rendered. Your site can have 10 pages or
10,000; startup time is the same.

Publish the same markdown to Confluence pages or embed in Backstage with native plugins.

## Features

- **CommonMark** — standard markdown via pulldown-cmark
- **Live reload** — edit markdown, see changes instantly in the browser
- **Diagram rendering** — PlantUML, Mermaid, GraphViz, and 14+ formats via Kroki
- **Tabbed content** — group related content with `:::tab` syntax
- **Status badges** — inline colored pill labels with Confluence status-macro parity
- **GitHub-style alerts** — `[!NOTE]`, `[!TIP]`, `[!WARNING]`, and more
- **Navigation and TOC** — automatic sidebar, breadcrumbs, and table of contents
- **Page metadata** — YAML frontmatter or sidecar files for titles, descriptions, and navigation order
- **Confluence rendering** — produce publish-ready bundles (XHTML + diagrams) for any Confluence publishing tool
- **Backstage integration** — embed docs with native Backstage plugins

## Quickstart

```bash
# macOS (Homebrew)
brew install rwdocs/tap/rw

# macOS / Linux (shell)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rwdocs/rw/releases/latest/download/rw-installer.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rwdocs/rw/releases/latest/download/rw-installer.ps1 | iex"
```

Then serve your docs:

```bash
rw serve
```

RW looks for markdown files in `docs/` by default. If `docs/` has no `index.md`, the project root `README.md` is used as the homepage.

Open [http://localhost:7979](http://localhost:7979) to see your site.

## Updating

If you installed via the shell or PowerShell script, upgrade in place with:

```bash
rw update
```

Use `rw update --check` to see whether a newer release is available without
installing it. Homebrew users upgrade with `brew upgrade rw` instead.

## Configuration

RW uses `rw.toml` for configuration, automatically discovered in the current directory or any parent directory.

```toml
[docs]
source_dir = "docs"

[diagrams]
kroki_url = "https://kroki.io"
```

See the [configuration guide](docs/configuration.md) for all options.

## Commands

| Command | Description |
|---------|-------------|
| `rw serve` | Start documentation server with live reload |
| `rw backstage publish` | Publish documentation bundles to S3 for Backstage |
| `rw confluence render` | Render markdown into a Confluence-publishable bundle (XHTML + diagrams) |
| `rw comment` | Read and write inline comments on project docs (for scripts and LLM agents) |
| `rw update` | Update rw to the latest release (self-update) |

## Documentation

- [Configuration](docs/configuration.md)
- [Page Metadata](docs/metadata.md)
- [Confluence Rendering](docs/confluence.md)
- [Diagram Rendering](docs/diagrams.md)
- [Status Badges](docs/status-badges.md)
- [Comment CLI](docs/comment-cli.md)
- [Embedding](docs/embedding.md)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option.
