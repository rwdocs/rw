# OAuth 1.0 Setup Guide for md2conf

This guide explains how to set up OAuth 1.0 authentication with write permissions for Confluence.

## Understanding OAuth Permissions

**Important:** OAuth 1.0 in Confluence doesn't have separate read/write scopes. Instead:
- The OAuth consumer gets the **same permissions as the user who authorizes it**
- If the user has write permissions in a space, the OAuth token will have write permissions
- If the user only has read permissions, the OAuth token will only have read permissions

## Current Issue

Your current OAuth tokens were likely authorized by a user with **read-only access** to the ARCH space, which is why create/update operations return `500 Internal Server Error`.

## Solution Options

### Option 1: Re-authorize with a User Who Has Write Access (EASIEST)

If the existing 'adrflow' OAuth consumer is already configured in Confluence:

1. **Check your Confluence permissions:**
   - Go to ARCH space → Space Settings → Permissions
   - Verify your user (my@cian.ru) has "Add" and "Edit" permissions
   - If not, ask the space admin to grant you write permissions

2. **Re-generate OAuth tokens** using the built-in command:

```bash
# Generate new tokens using the existing 'adrflow' consumer
uv run md2conf generate-tokens

# Or specify custom consumer key
uv run md2conf generate-tokens -c md2conf
```

The command will:
- Start a local server on port 8080
- Give you a URL to open in your browser
- Wait for you to authorize the application
- Display the new tokens to add to config.toml

**Important:** Make sure you're logged into Confluence as a user with write permissions before clicking the authorization URL.

### Option 2: Create New OAuth Consumer for md2conf

If you have Confluence admin access:

#### Step 1: Generate RSA Key Pair (if not already done)

```bash
# Generate private key (1024-bit or 2048-bit)
openssl genrsa -out private_key.pem 2048

# Generate public key
openssl rsa -in private_key.pem -pubout -out public_key.pem
```

#### Step 2: Configure OAuth Consumer in Confluence

1. **Access OAuth Administration:**
   - Go to Confluence → Settings (⚙️) → General Configuration
   - In the left sidebar, find "Application Links" or "OAuth"
   - Click "Add Application Link" or "Add OAuth Consumer"

2. **For Application Links (Incoming Link):**
   - Select "External application"
   - Direction: **Incoming** (Confluence is the OAuth provider)
   - Provide redirect URL: `http://localhost:8080/callback`

3. **Configure Consumer Details:**
   - **Consumer Key**: `md2conf` (or `adrflow` to reuse existing)
   - **Consumer Name**: `md2conf - Markdown to Confluence`
   - **Public Key**: Paste content from `public_key.pem`
     - **Important:** Remove the `-----BEGIN PUBLIC KEY-----` and `-----END PUBLIC KEY-----` lines
     - Paste only the base64-encoded key content
     - No leading/trailing spaces or line breaks

4. **Save the consumer**

#### Step 3: Generate OAuth Access Tokens

You'll need to implement or use an OAuth authorization flow. The adrflow project has this in `src/adrflow/cli/auth.py`.

**Using a Python script:**

```python
# This would be similar to adrflow's auth.py
# 1. Request temporary token
# 2. User visits authorization URL
# 3. User approves access
# 4. Exchange verification code for access token
```

**Key requirements:**
- The **authorizing user must have write permissions** in the target spaces
- The tokens will inherit that user's permissions
- Access tokens persist for 5 years unless revoked

#### Step 4: Update config.toml

```toml
[confluence]
base_url = "https://conf.cian.tech"
access_token = "your_new_access_token"
access_secret = "your_new_access_secret"
consumer_key = "md2conf"  # or "adrflow"
```

### Option 3: Use Confluence REST API with Basic Auth (Temporary Testing)

For quick testing, you could use basic authentication instead of OAuth:

⚠️ **Not recommended for production** - only for POC testing

This would require modifying the code to support username/password authentication instead of OAuth.

## Verifying Write Permissions

After setting up OAuth with the correct permissions:

```bash
# Test authentication
uv run md2conf test-auth

# Try creating a page
uv run md2conf create test_document.md "Test Page"

# Try updating a page you own
uv run md2conf update test_document.md <page-id>
```

## Troubleshooting

### Still getting 500 errors?

1. **Check space permissions:**
   ```bash
   # In Confluence UI:
   # Space → Space Settings → Permissions
   # Verify the authorizing user has "Add" and "Edit" permissions
   ```

2. **Check user permissions:**
   - Go to any page in the space
   - Try editing it manually in the UI
   - If you can't edit manually, OAuth won't work either

3. **Check OAuth consumer status:**
   - In Confluence admin → Application Links
   - Find your OAuth consumer
   - Verify it's enabled and configured correctly

### OAuth Token was working before?

If the same token worked previously:
- User permissions may have been revoked
- Space permissions may have changed
- OAuth consumer may have been disabled

## References

- [Atlassian OAuth 1.0 Guide](https://developer.atlassian.com/server/jira/platform/oauth/)
- [Configure Application Links](https://confluence.atlassian.com/doc/configure-an-incoming-link-1115674734.html)
- adrflow project: `src/adrflow/cli/auth.py` for OAuth token generation example
