# md2conf

Convert markdown files to Confluence pages.

POC for testing markdown-to-Confluence publishing with comment preservation.

## Setup

### 1. Install dependencies

```bash
uv sync
```

### 2. Configure OAuth credentials

Copy the example configuration:

```bash
cp config.toml.example config.toml
```

Edit `config.toml` and add your Confluence OAuth credentials:
- `access_token` - Your OAuth access token
- `access_secret` - Your OAuth access token secret

Place your OAuth private key as `private_key.pem` in the project root.

### 3. Test authentication

```bash
uv run md2conf test-auth
```

If authentication is successful, you'll see:
```
Authentication successful!
Authenticated as: <your-username>
```

## Usage

### Available Commands

```bash
# Test authentication
uv run md2conf test-auth

# Get page information
uv run md2conf get-page <page-id>

# Convert markdown to Confluence storage format (preview)
uv run md2conf convert <markdown-file>

# Create a page from markdown
uv run md2conf create <markdown-file> "Page Title" --space SPACE

# Update an existing page from markdown
uv run md2conf update <markdown-file> <page-id> -m "Update message"
```

### Example Workflow

1. **Preview conversion:**
```bash
uv run md2conf convert your-file.md
```

2. **Create a page:**
```bash
uv run md2conf create your-file.md "My Test Page"
# Returns page ID and URL
```

3. **Update the page:**
```bash
uv run md2conf update your-file.md <page-id> -m "Updated content"
# Shows new version and comment count
```

### OAuth Permissions Note

**IMPORTANT:** The OAuth token must have **write permissions** to create or update pages. Read-only tokens will result in `500 Internal Server Error` from Confluence.

#### Generating OAuth Tokens with Write Access

If you have write permissions in Confluence, generate new tokens:

```bash
uv run md2conf generate-tokens
```

This will:
1. Start an interactive OAuth flow
2. Open a browser for authorization
3. Generate tokens that inherit YOUR Confluence permissions
4. Display credentials to add to `config.toml`

The generated tokens will inherit your Confluence user permissions.

## Development Status

This is a POC project for testing markdown-to-Confluence publishing with comment preservation.

**Completed phases:**
- Phase 1 - Setup & Authentication ✅
- Phase 2 - Confluence API Client ✅
- Phase 3 - Markdown to Confluence Conversion ✅

**Current status:** All core functionality working. Ready for comment preservation testing.

## Technical Details

- Uses OAuth 1.0 RSA-SHA1 authentication
- Supports Confluence Server/Data Center REST API v1
- Markdown conversion via `md2cf` library
- Async HTTP operations with `httpx`

## Quick Commands Reference

```bash
# Generate OAuth tokens (needed for write access)
uv run md2conf generate-tokens

# Test authentication
uv run md2conf test-auth

# Preview conversion
uv run md2conf convert your-file.md

# Create page from markdown
uv run md2conf create your-file.md "My Page Title"

# Update existing page
uv run md2conf update your-file.md <page-id> -m "Update message"

# Get page info
uv run md2conf get-page <page-id>
``` 
