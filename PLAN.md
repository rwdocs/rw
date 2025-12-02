# Implementation Plan: md2conf

POC for publishing markdown documents to Confluence with comment preservation.

## Architecture Overview

Based on the adrflow reference project, we'll build a simple CLI tool that:
1. Converts markdown to Confluence storage format (XHTML)
2. Creates/updates Confluence pages via REST API
3. Uses OAuth 1.0 authentication (same as adrflow)

## Dependencies

Core libraries (from adrflow):
- `httpx` - HTTP client for Confluence API
- `authlib` - OAuth 1.0 authentication
- `cryptography` - RSA key handling for OAuth

Additional:
- `markdown` or `mistune` - Markdown to HTML conversion
- Confluence storage format converter (research needed)

## Project Structure

```
md2conf/
├── src/md2conf/
│   ├── __init__.py
│   ├── cli.py              # CLI entry point
│   ├── config.py           # Configuration (OAuth tokens, Confluence URL)
│   ├── oauth.py            # OAuth 1.0 authentication (from adrflow)
│   ├── confluence/
│   │   ├── __init__.py
│   │   ├── client.py       # Confluence REST API client
│   │   └── converter.py    # Markdown to Confluence storage format
│   └── test_document.md    # Test markdown file with 3 paragraphs
├── pyproject.toml
├── config.toml             # OAuth credentials (gitignored)
└── private_key.pem         # OAuth private key (gitignored)
```

## Implementation Steps

### Phase 1: Setup & Authentication
1. Add dependencies to `pyproject.toml`
   - httpx, authlib, cryptography
   - markdown/mistune for conversion
2. Copy OAuth authentication from adrflow
   - `oauth.py` with Confluence endpoints
   - `config.py` for loading credentials from TOML
3. Create `config.toml.example` template
4. Test authentication with simple API call

### Phase 2: Confluence API Client
1. Create `confluence/client.py` with methods:
   - `create_page(space_key, title, body)` - Create new page
   - `get_page(page_id)` - Get page content and version
   - `update_page(page_id, title, body, version)` - Update existing page
   - `get_comments(page_id)` - List page comments (for verification)
2. Use Confluence REST API v1 (same as adrflow)
3. Test with manual API calls

### Phase 3: Markdown to Confluence Conversion
1. Research Confluence storage format
   - It's XHTML-based with specific tags
   - May need library like `md2cf` or custom converter
2. Implement `confluence/converter.py`:
   - Convert markdown to Confluence XHTML
   - Handle common elements: paragraphs, headers, lists, code blocks
3. Create test markdown file:
   ```markdown
   # Test Document

   This is the first paragraph with some text.

   This is the second paragraph with **bold** and *italic* formatting.

   This is the third paragraph with a [link](https://example.com).
   ```

### Phase 4: CLI Implementation
1. Create `cli.py` with commands:
   - `md2conf create <markdown_file> <space_key> <title>` - Create page
   - `md2conf update <markdown_file> <page_id>` - Update page
   - `md2conf get <page_id>` - Show page info
2. Load config from `config.toml`
3. Initialize OAuth client
4. Convert markdown and call API

### Phase 5: POC Testing
Execute the test plan from RD.md:

1. **Initial Creation**
   - Create test markdown with 3 paragraphs
   - Run: `md2conf create test_document.md SPACE "Test Page"`
   - Verify page created in Confluence

2. **Add Comments**
   - Manually add comments in Confluence UI
   - Note comment locations (which paragraphs)

3. **Update Page**
   - Modify test markdown:
     - Add 4th paragraph
     - Change text in 2nd paragraph
   - Run: `md2conf update test_document.md <page_id>`
   - Verify in Confluence:
     - New paragraph appears
     - Changed paragraph updated
     - Comments preserved/resolved behavior

4. **Document Results**
   - Record which comments stayed
   - Record which were marked resolved
   - Document any issues

## Configuration

Example `config.toml`:
```toml
[confluence]
base_url = "https://conf.cian.tech"
access_token = "your_token"
access_secret = "your_secret"
consumer_key = "md2conf"

[test]
space_key = "TEST"
```

## Technical Notes

### Confluence Storage Format
- Uses XHTML with specific tags
- Paragraphs: `<p>...</p>`
- Headers: `<h1>...</h1>`
- Code: `<ac:structured-macro ac:name="code">...</ac:structured-macro>`
- Research library: possibly `atlassian-python-api` or custom

### Comment Preservation
- Confluence comments are anchored to content
- When content changes:
  - Comments may stay if anchor text unchanged
  - Comments may be marked "outdated" if anchor changes
  - This behavior is what we're testing

### Version Management
- Confluence requires version number for updates
- Must GET page first to get current version
- Increment version on update

## Success Criteria

POC is successful if:
1. Can create page from markdown
2. Can update page from modified markdown
3. Comments behavior is documented (stay/resolve patterns)
4. Process is reproducible for future ADR workflow

## Future Considerations (out of scope for POC)

- Batch processing multiple markdown files
- Automatic space/parent page detection
- Comment migration/anchoring preservation
- Integration with git workflow
- Dry-run mode
- Diff preview before update
