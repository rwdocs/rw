# RW

A documentation engine with no build step. Point it at a directory of markdown files
and start serving — pages are rendered when requested, not ahead of time.
One file changed? Only that page is re-rendered. Your site can have 10 pages or
10,000; startup time is the same.

Publish the same markdown to Confluence pages or build static sites for Backstage TechDocs.

## Features

- **CommonMark** — standard markdown via pulldown-cmark
- **Live reload** — edit markdown, see changes instantly in the browser
- **Diagram rendering** — PlantUML, Mermaid, GraphViz, and 14+ formats via Kroki
- **Tabbed content** — group related content with `:::tab` syntax
- **GitHub-style alerts** — `[!NOTE]`, `[!TIP]`, `[!WARNING]`, and more
- **Navigation and TOC** — automatic sidebar, breadcrumbs, and table of contents
- **Page metadata** — YAML sidecar files for titles, descriptions, and custom variables
- **Confluence publishing** — update pages via REST API with OAuth authentication
- **TechDocs output** — build static sites compatible with Backstage

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

## Configuration

RW uses `rw.toml` for configuration, automatically discovered in the current directory or any parent directory.

```toml
[docs]
source_dir = "docs"

[diagrams]
kroki_url = "https://kroki.io"

[confluence]
base_url = "${CONFLUENCE_URL}"
access_token = "${CONFLUENCE_TOKEN}"
access_secret = "${CONFLUENCE_SECRET}"
```

See the [configuration guide](docs/configuration.md) for all options.

## Commands

| Command | Description |
|---------|-------------|
| `rw serve` | Start documentation server with live reload |
| `rw confluence update` | Publish markdown to a Confluence page |
| `rw confluence generate-tokens` | Generate OAuth access tokens |
| `rw techdocs build` | Build a static site for Backstage TechDocs |
| `rw techdocs publish` | Upload static site to S3 |

## Documentation

- [Configuration](docs/configuration.md)
- [Page Metadata](docs/metadata.md)
- [Confluence Publishing](docs/confluence.md)
- [TechDocs](docs/techdocs.md)
- [Diagram Rendering](docs/diagrams.md)
