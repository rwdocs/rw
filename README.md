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

# Test creating a page
uv run md2conf test-create "Test Page Title" --space TEST

# Full create/update workflow - coming soon
```

### Testing the API

Create a test page:
```bash
uv run md2conf test-create "My Test Page"
# Returns page ID and URL
```

Get page info:
```bash
uv run md2conf get-page <page-id>
# Shows title, version, URL, and comment count
```

## Development Status

This is a POC project. See [PLAN.md](PLAN.md) for the implementation plan and [RD.md](RD.md) for requirements.

**Completed phases:**
- Phase 1 - Setup & Authentication ✅
- Phase 2 - Confluence API Client ✅

**Current phase:** Phase 3 - Markdown to Confluence Conversion 
