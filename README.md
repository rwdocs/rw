# Docstage

Documentation engine for Backstage. Convert markdown files to Confluence pages with PlantUML diagram support.

*"Where documentation takes the stage"*

## Setup

```bash
uv sync
cp docstage.toml.example docstage.toml
```

Edit `docstage.toml` with your settings and place `private_key.pem` in the project root.

## Configuration

Docstage uses `docstage.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

```toml
# docstage.toml

[server]
host = "127.0.0.1"      # Server host
port = 8080             # Server port

[docs]
source_dir = "docs"     # Markdown source directory
cache_dir = ".cache"    # Cache directory

[diagrams]
kroki_url = "https://kroki.io"  # Enables diagram rendering
include_dirs = ["."]            # PlantUML !include search paths
config_file = "config.iuml"     # PlantUML config file
dpi = 192                       # DPI for diagrams (retina)

[live_reload]
enabled = true                  # Enable live reload (default: true)
watch_patterns = ["**/*.md"]    # Patterns to watch

[confluence]
base_url = "https://confluence.example.com"
access_token = "your-token"
access_secret = "your-secret"
consumer_key = "docstage"

[confluence.test]
space_key = "DOCS"
```

CLI options override config file values:

```bash
# Use config file
docstage serve

# Override port from config
docstage serve --port 9000

# Use explicit config file
docstage serve --config /path/to/docstage.toml
```

## Usage

```bash
# Start documentation server (with live reload)
uv run docstage serve

# Start server without live reload
uv run docstage serve --no-live-reload

# Generate OAuth tokens (requires write permissions in Confluence)
uv run docstage generate-tokens

# Test authentication
uv run docstage test-auth

# Preview markdown conversion
uv run docstage convert document.md

# Create a new page
uv run docstage create document.md "Page Title" --space ARCH

# Update an existing page
uv run docstage update document.md <page-id> -m "Update message"

# Get page info
uv run docstage get-page <page-id>
```

## OAuth Permissions

OAuth tokens inherit the authorizing user's permissions. If you get `500` errors on create/update:
1. Verify you can create/edit pages manually in the target space
2. Regenerate tokens with `uv run docstage generate-tokens`

## Technical Details

- OAuth 1.0 RSA-SHA1 authentication
- Confluence Server/Data Center REST API v1
- Rust-based markdown conversion via `docstage-core`
- PlantUML diagram rendering with automatic width scaling
