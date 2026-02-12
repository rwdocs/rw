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
# Note: Frontend assets are automatically built by build.rs
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
cache_enabled = true    # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Required when [diagrams] section is present
include_dirs = ["."]            # PlantUML !include search paths
dpi = 192                       # DPI for diagrams (retina)

[live_reload]
enabled = true                  # Enable live reload (default: true)
watch_patterns = ["**/*.md"]    # Patterns to watch

[metadata]
name = "meta.yaml"              # Metadata file name (default: meta.yaml)

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

## README.md as Homepage

If your `docs/` directory doesn't have an `index.md`, RW automatically uses `README.md` from the project root as the homepage. No configuration needed.

- `docs/index.md` exists: used as homepage (normal behavior)
- `docs/index.md` missing + `README.md` exists: README.md serves as homepage
- Live reload works for README.md changes too

## TechDocs (Backstage Integration)

Build and publish documentation sites compatible with Backstage TechDocs:

```bash
# Build static site
rw techdocs build --site-name "My Docs" --output-dir ./site

# Publish to S3
rw techdocs publish \
  --entity default/Component/my-service \
  --bucket my-techdocs-bucket \
  --endpoint https://storage.yandexcloud.net \
  --region ru-central1
```

S3 credentials use standard `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` environment variables.

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

## Page Metadata

Pages can have metadata defined in YAML sidecar files (default: `meta.yaml` in the same directory as `index.md`).

```yaml
# docs/domain-a/meta.yaml
title: "My Domain"
description: "Domain overview"
type: domain
vars:
  owner: team-a
  priority: 1
```

### Metadata Fields

- `title` - Custom page title (overrides H1 extraction)
- `description` - Page description for display
- `type` - Page type (e.g., "domain", "guide"). Pages with `type` are registered as sections
- `vars` - Custom variables (key-value pairs)

### Inheritance

Metadata is inherited from parent directories:
- `title` - Never inherited
- `description` - Never inherited
- `type` - Never inherited
- `vars` - Deep merged (child values override parent keys)

### Virtual Pages

Directories with `meta.yaml` but no `index.md` become virtual pages:
- Appear in navigation with their metadata title
- Render h1 with title only (no content body)
- Support nested virtual pages for organizing content hierarchies

Example structure:
```
docs/
├── index.md           # Home page
├── domains/
│   ├── meta.yaml      # Virtual page: "Domains"
│   ├── billing/
│   │   ├── meta.yaml  # Virtual page: "Billing"
│   │   └── api.md     # Real page under Billing
│   └── users/
│       └── index.md   # Real page (has index.md)
```

### Diagram Includes

Pages with `type` set to `domain`, `system`, or `service` automatically generate PlantUML C4 model includes. Use them in PlantUML diagrams:

```plantuml
!include systems/sys_payment_gateway.iuml
!include systems/ext/sys_yookassa.iuml

Rel(sys_payment_gateway, sys_yookassa, "Processes payments")
```

**Include paths by type:**

| Type | Regular | External |
|------|---------|----------|
| Domain | `systems/dmn_{name}.iuml` | `systems/ext/dmn_{name}.iuml` |
| System | `systems/sys_{name}.iuml` | `systems/ext/sys_{name}.iuml` |
| Service | `systems/svc_{name}.iuml` | `systems/ext/svc_{name}.iuml` |

The `{name}` is derived from the directory name with hyphens replaced by underscores (e.g., `payment-gateway` → `payment_gateway`).

Regular includes generate `System()` macros; external includes generate `System_Ext()` macros. Both include the entity's title, description, and a link to its documentation page.

## Technical Details

- Native Rust CLI (no Python runtime required)
- OAuth 1.0 RSA-SHA1 authentication
- Confluence Server/Data Center REST API v1
- Rust-based markdown conversion via `rw-confluence`
- PlantUML diagram rendering with automatic width scaling
