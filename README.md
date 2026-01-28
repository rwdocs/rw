# RW

Documentation engine. Convert markdown files to Confluence pages with PlantUML diagram support.

## Setup

```bash
# Build the CLI
cargo build -p rw

# Copy example config
cp rw.toml.example rw.toml

# Install coverage tool (optional, for running `make test`)
cargo install cargo-llvm-cov
```

Edit `rw.toml` with your settings and place `private_key.pem` in the project root.

### Installation

```bash
# Install from source
cargo install --path crates/rw

# Or build release binary with embedded assets
cargo build -p rw --release --features embed-assets
```

## Configuration

RW uses `rw.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

```toml
# rw.toml

[server]
host = "127.0.0.1"      # Server host
port = 8080             # Server port

[docs]
source_dir = "docs"     # Markdown source directory
cache_dir = ".cache"    # Cache directory
cache_enabled = true    # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Required when [diagrams] section is present
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
consumer_key = "rw"

```

### Environment Variables

String configuration values support environment variable expansion:

```toml
[confluence]
base_url = "${CONFLUENCE_URL}"
access_token = "${CONFLUENCE_TOKEN}"
access_secret = "${CONFLUENCE_SECRET}"
consumer_key = "${CONFLUENCE_CONSUMER_KEY:-rw}"  # with default value

[diagrams]
kroki_url = "${KROKI_URL:-https://kroki.io}"
```

Supported syntax:
- `${VAR}` - expands to the value of VAR, errors if unset
- `${VAR:-default}` - expands to VAR if set, otherwise uses default

Expanded fields: `server.host`, `confluence.base_url`, `confluence.access_token`,
`confluence.access_secret`, `confluence.consumer_key`, `diagrams.kroki_url`

CLI options override config file values:

```bash
# Use config file
rw serve

# Override port from config
rw serve --port 9000

# Use explicit config file
rw serve --config /path/to/rw.toml
```

## Usage

```bash
# Start documentation server (with live reload)
rw serve

# Start server without live reload
rw serve --no-live-reload

# Start server without caching (useful for development)
rw serve --no-cache
```

## Confluence Publishing

All Confluence-related commands are grouped under the `confluence` subcommand:

```bash
# Generate OAuth tokens (requires write permissions in Confluence)
rw confluence generate-tokens

# Update an existing page
rw confluence update document.md <page-id> -m "Update message"

# Preview changes without updating (dry run)
rw confluence update document.md <page-id> --dry-run
```

## OAuth Permissions

OAuth tokens inherit the authorizing user's permissions. If you get `500` errors on update:
1. Verify you can edit pages manually in the target space
2. Regenerate tokens with `rw confluence generate-tokens`

## Technical Details

- Native Rust CLI (no Python runtime required)
- OAuth 1.0 RSA-SHA1 authentication
- Confluence Server/Data Center REST API v1
- Rust-based markdown conversion via `rw-confluence`
- PlantUML diagram rendering with automatic width scaling
