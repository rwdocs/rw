# md2conf

Convert markdown files to Confluence pages with PlantUML diagram support.

## Setup

```bash
uv sync
cp config.toml.example config.toml
```

Edit `config.toml` with your OAuth credentials and place `private_key.pem` in the project root.

## Usage

```bash
# Generate OAuth tokens (requires write permissions in Confluence)
uv run md2conf generate-tokens

# Test authentication
uv run md2conf test-auth

# Preview markdown conversion
uv run md2conf convert document.md

# Create a new page
uv run md2conf create document.md "Page Title" --space ARCH

# Update an existing page
uv run md2conf update document.md <page-id> -m "Update message"

# Get page info
uv run md2conf get-page <page-id>
```

## OAuth Permissions

OAuth tokens inherit the authorizing user's permissions. If you get `500` errors on create/update:
1. Verify you can create/edit pages manually in the target space
2. Regenerate tokens with `uv run md2conf generate-tokens`

## Technical Details

- OAuth 1.0 RSA-SHA1 authentication
- Confluence Server/Data Center REST API v1
- Rust-based markdown conversion via `md2conf-core`
- PlantUML diagram rendering with automatic width scaling
