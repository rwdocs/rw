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

Coming soon - create and update commands.

## Development Status

This is a POC project. See [PLAN.md](PLAN.md) for the implementation plan and [RD.md](RD.md) for requirements.

Current phase: **Phase 1 - Setup & Authentication** 
