# md2conf Usage Guide

Complete guide for using md2conf to publish markdown to Confluence.

## Quick Start

### 1. Generate OAuth Tokens

```bash
# Make sure you have private_key.pem from the adrflow setup
uv run md2conf generate-tokens
```

**What happens:**
1. Command starts local server on http://localhost:8080
2. You'll see a URL like: `https://conf.cian.tech/plugins/servlet/oauth/authorize?oauth_token=...`
3. Open this URL in your browser (make sure you're logged into Confluence)
4. Click "Allow" to authorize md2conf
5. You'll be redirected to localhost (might show "can't connect" - that's OK!)
6. Return to terminal - you'll see your new tokens

**Example output:**
```
======================================================================
✓ OAuth Authorization Successful!
======================================================================

Add these credentials to your config.toml:

[confluence]
base_url = "https://conf.cian.tech"
access_token = "xyz123..."
access_secret = "abc456..."
consumer_key = "adrflow"

======================================================================
Note: These tokens inherit YOUR permissions in Confluence.
If you can create/edit pages, these tokens will have write access.
======================================================================
```

### 2. Update config.toml

Copy the credentials from the output and update your `config.toml`:

```toml
[confluence]
base_url = "https://conf.cian.tech"
access_token = "xyz123..."  # Replace with your token
access_secret = "abc456..."  # Replace with your secret
consumer_key = "adrflow"

[test]
space_key = "ARCH"  # Or your target space
```

### 3. Test Authentication

```bash
uv run md2conf test-auth
```

Should show:
```
Authentication successful!
Authenticated as: your.email@cian.ru
```

### 4. Create Your First Page

```bash
# Preview the conversion first
uv run md2conf convert test_document.md

# Create the page
uv run md2conf create test_document.md "My Test Page"
```

**Output:**
```
Converting test_document.md...
Creating page "My Test Page" in space ARCH...

Page created successfully!
ID: 1234567890
Title: My Test Page
Version: 1
URL: https://conf.cian.tech/pages/viewpage.action?pageId=1234567890
```

### 5. Update the Page

Edit `test_document.md`, then:

```bash
uv run md2conf update test_document.md 1234567890 -m "Updated content"
```

## Common Workflows

### POC Testing (from RD.md)

Following the test plan in RD.md:

**Step 1: Create initial page**
```bash
uv run md2conf create test_document.md "POC Test Page"
# Note the page ID from output
```

**Step 2: Add comments in Confluence**
- Open the page URL in browser
- Add comments to different paragraphs
- Note which paragraphs have comments

**Step 3: Modify markdown**
Edit `test_document.md`:
```markdown
# Test Document

This is the first paragraph with some text. It contains basic content to test how Confluence handles simple paragraphs.

This is the MODIFIED second paragraph with **bold** and *italic* formatting. It also includes a [link to example.com](https://example.com).

This is the third paragraph with `inline code` and more text. This paragraph will help us test comment preservation.

This is the NEW fourth paragraph added to test how new content affects existing comments.
```

**Step 4: Update the page**
```bash
uv run md2conf update test_document.md <page-id> -m "Added 4th paragraph and modified 2nd"
```

**Step 5: Check results**
- Open page in Confluence
- Verify new paragraph appears
- Check which comments are preserved vs resolved

### Converting Existing Markdown Docs

```bash
# Convert a single file
uv run md2conf create docs/architecture/decision-001.md "ADR-001: Use PostgreSQL"

# Preview before creating
uv run md2conf convert docs/architecture/decision-001.md | less

# Update existing page
uv run md2conf update docs/architecture/decision-001.md 1234567890
```

### Batch Operations

For multiple files, use a simple shell script:

```bash
#!/bin/bash
# create_adrs.sh

for file in docs/adrs/*.md; do
    title=$(grep "^# " "$file" | head -1 | sed 's/^# //')
    echo "Creating: $title"
    uv run md2conf create "$file" "$title" --space ARCH
    sleep 2  # Be nice to the server
done
```

## Troubleshooting

### "500 Internal Server Error" on create/update

**Cause:** OAuth tokens don't have write permissions

**Solution:**
1. Check if you can manually create/edit pages in the space
2. If yes: regenerate tokens with `uv run md2conf generate-tokens`
3. If no: ask space admin for write permissions, then regenerate tokens

### "401 Unauthorized"

**Cause:** Invalid or expired OAuth tokens

**Solution:**
```bash
# Test auth first
uv run md2conf test-auth

# If fails, regenerate tokens
uv run md2conf generate-tokens
```

### "Page not found" errors

**Cause:** Invalid page ID

**Solution:**
```bash
# Verify page exists
uv run md2conf get-page <page-id>

# Check the page URL - ID is in the pageId parameter
# https://conf.cian.tech/pages/viewpage.action?pageId=1234567890
#                                                       ^^^^^^^^^^
```

### Conversion issues

**Problem:** Markdown not rendering correctly

**Debug:**
```bash
# Preview conversion
uv run md2conf convert your-file.md

# Check the HTML output
# If it looks wrong, the issue is in the markdown conversion
```

**Common fixes:**
- Make sure lists have blank lines before/after
- Ensure code blocks use triple backticks
- Check that links are in `[text](url)` format

## Advanced Usage

### Custom consumer key

If you created a separate OAuth consumer:

```bash
uv run md2conf generate-tokens -c my-custom-consumer
```

### Different Confluence instance

```bash
uv run md2conf generate-tokens -u https://different-confluence.example.com
```

### Specify space for creation

```bash
# Override config.toml space setting
uv run md2conf create file.md "Title" --space CUSTOM
```

## Command Reference

```bash
# Authentication
uv run md2conf test-auth                    # Test current credentials
uv run md2conf generate-tokens              # Generate new OAuth tokens

# Read operations
uv run md2conf get-page <page-id>           # Get page info
uv run md2conf convert <markdown-file>      # Preview conversion

# Write operations
uv run md2conf create <file> "Title"        # Create new page
uv run md2conf update <file> <page-id>      # Update existing page

# Options
--space, -s SPACE                           # Target space key
--message, -m MSG                           # Version message (update only)
--config, -c FILE                           # Config file (default: config.toml)
--key-file, -k FILE                         # Private key (default: private_key.pem)
```

## Tips

1. **Always preview first:** Use `convert` to check output before creating pages
2. **Test with test space:** Create pages in a test space first
3. **Use version messages:** Add meaningful messages when updating (`-m "Fixed typos"`)
4. **Keep page IDs:** Note the page ID from create command for future updates
5. **Check comments regularly:** Monitor comment preservation during POC testing
