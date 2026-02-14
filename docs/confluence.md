# Confluence Publishing

RW can publish markdown documents to Confluence pages. All Confluence-related
commands are grouped under the `confluence` subcommand.

## Setup

Configure the Confluence connection in `rw.toml`:

```toml
[confluence]
base_url = "${CONFLUENCE_URL}"
access_token = "${CONFLUENCE_TOKEN}"
access_secret = "${CONFLUENCE_SECRET}"
consumer_key = "${CONFLUENCE_CONSUMER_KEY:-rw}"
```

Authentication uses OAuth 1.0 RSA-SHA1. Place a `private_key.pem` file in the
project root.

## Generating Tokens

Generate OAuth access tokens with:

```bash
rw confluence generate-tokens
```

This requires write permissions in Confluence.

## Publishing Pages

Update an existing Confluence page from a markdown file:

```bash
# Update an existing page
rw confluence update document.md <page-id> -m "Update message"

# Preview changes without updating (dry run)
rw confluence update document.md <page-id> --dry-run
```

## OAuth Permissions

OAuth tokens inherit the authorizing user's permissions. If you get `500` errors
on update:

1. Verify you can edit pages manually in the target space
2. Regenerate tokens with `rw confluence generate-tokens`

## Technical Details

- OAuth 1.0 RSA-SHA1 authentication
- Confluence Server/Data Center REST API v1
- Comment preservation when updating pages
- PlantUML diagrams uploaded as attachments
