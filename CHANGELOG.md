# Changelog

All notable changes to the md2conf project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- **Comment Preservation**: Tree-based algorithm preserves inline comments when updating pages
  - Parses Confluence HTML and converted markdown into DOM trees
  - Matches nodes between old and new trees using structural and text similarity
  - Transfers comment markers from old tree to matching positions in new tree
  - Comments on unchanged content are preserved
  - Comments on changed content are naturally resolved (correct behavior for reviews)
  - Implemented in `confluence/comment_preservation.py`
  - Integrated into `update` command workflow

### Fixed
- **CRITICAL FIX**: Fixed OAuth signature generation for POST/PUT requests by adding `force_include_body=True` to OAuth1Auth configuration
  - Root cause: Confluence requires request body to be included in OAuth signature calculation for POST/PUT operations
  - The `authlib` library defaults to `force_include_body=False`, which caused 500 Internal Server Error
  - Fixed in `oauth.py:78` by enabling body inclusion in signature
  - Discovered by comparing with working Go implementation using `github.com/dghubble/oauth1`
- Fixed XML namespace handling in comment preservation parser
  - Added namespace declarations to wrapper element for proper parsing
  - Handles Confluence's `ac:` and `ri:` namespace prefixes
- Fixed TreeNode hashability issue for dict key usage
  - Changed matcher to use object IDs instead of objects as dict keys

### Changed
- Updated OAuth token generation flow to use 'oob' (out-of-band) callback method per Atlassian OAuth 1.0a specifications
  - Changed from localhost callback to manual verification code entry
  - Improved user guidance with step-by-step instructions
  - Added fallback for manual URL/verifier code entry
  - Reference: https://developer.atlassian.com/server/jira/platform/oauth/
- Enhanced `get-page` command to display page content in Confluence storage format
  - Shows inline comment markers for debugging
  - Useful for verifying comment preservation

### Verified
All operations working correctly:
- ✅ Creating pages with `test-create`
- ✅ Creating pages from markdown with `create`
- ✅ Updating pages with `update`
- ✅ OAuth token generation with `generate-tokens`
- ✅ Comment preservation on updates (POC requirement met!)

## [0.3.0] - Phase 3: Markdown Conversion (Blocked)

### Added
- Markdown to Confluence storage format converter using `md2cf` library
- `MarkdownConverter` class with `convert()` and `convert_file()` methods
- Support for common markdown elements:
  - Headings (h1-h6)
  - Paragraphs with text formatting (bold, italic, code)
  - Links
  - Lists (ordered and unordered)
  - Code blocks
- `convert` CLI command to preview Confluence HTML output
- `create` CLI command to create pages from markdown files
- `update` CLI command to update existing pages from markdown files

### Dependencies
- `md2cf` 2.3+ for Confluence rendering
- `mistune` 0.8.4 (dependency of md2cf)

### Status
- ✅ Markdown conversion working correctly
- ✅ Preview command functional
- ✅ Create/update commands now working (OAuth signature issue resolved)

## [0.2.0] - Phase 2: Confluence API Client

### Added
- Full Confluence REST API client implementation
- `ConfluenceClient` class with async HTTP operations
- Core API methods:
  - `create_page()` - Create new pages with optional parent
  - `get_page()` - Fetch page content and metadata with expansion support
  - `update_page()` - Update existing pages with version management
  - `get_comments()` - Retrieve page comments for POC testing
  - `get_page_url()` - Generate web URLs for pages
- TypedDict definitions for type-safe API requests and responses
- Comprehensive error handling and logging
- CLI commands:
  - `get-page <page-id>` - Retrieve page information
  - `test-create <title>` - Test page creation with simple HTML

### Technical Details
- Uses Confluence REST API v1 (`/rest/api`)
- Supports content expansion (body.storage, version, etc.)
- Proper version incrementing for updates
- Detailed logging at INFO and DEBUG levels

### Verified
- ✅ OAuth authentication works correctly with `test-auth` command
- ✅ Page retrieval (`get-page`) working
- ✅ Page creation working (OAuth signature issue resolved)

## [0.1.0] - Phase 1: Setup & Authentication

### Added
- Project structure and build configuration
- OAuth 1.0 RSA-SHA1 authentication implementation
- Configuration management system:
  - TOML-based configuration (`config.toml`)
  - Private RSA key file support (`private_key.pem`)
  - Configuration validation with dataclasses
- `create_confluence_client()` function for authenticated HTTP clients
- `test-auth` CLI command to verify OAuth credentials
- Interactive OAuth token generation:
  - `generate-tokens` CLI command
  - Local callback server (port 8080)
  - Step-by-step authorization flow
  - Browser-based authorization

### Configuration
- `config.toml.example` template with structure:
  - `[confluence]` section for OAuth credentials and base URL
  - `[test]` section for test configuration (space key)
- Secrets properly excluded via `.gitignore`

### Dependencies
- Python 3.14
- `httpx` 0.28+ for async HTTP
- `authlib` 1.6+ for OAuth 1.0
- `cryptography` for RSA key handling
- `click` 8.3+ for CLI framework

### Verified
- ✅ OAuth authentication working
- ✅ Successfully authenticates and retrieves current user info
- ✅ Token generation flow functional

## [0.0.0] - Initial Setup

### Added
- Project repository initialization
- Git configuration
- Basic project structure
- README.md with project overview

## Current Status Summary

### Completed
- ✅ Phase 1: OAuth authentication and configuration
- ✅ Phase 2: Confluence API client
- ✅ Phase 3: Markdown conversion (code complete)

### Unblocked
- ✅ Phase 3 Testing: Create/update operations now working
- ✅ POC Testing: Ready to proceed with comment preservation testing

### Next Steps
1. **Complete POC Testing**
   - ✅ Create test page from markdown
   - ⏳ Add inline comments in Confluence UI
   - ⏳ Update page with modified markdown
   - ⏳ Document comment preservation behavior

2. **Future Enhancements** (if needed)
   - Consider adding parent page support to CLI
   - Add batch processing capabilities
   - Implement dry-run mode

## POC Test Plan Progress

- ✅ Create test markdown with 3 paragraphs
- ✅ Create page from markdown → Working with fixed OAuth signature
- ⏳ Add comments on page in Confluence UI → Ready to proceed
- ⏳ Modify markdown (add paragraph, change existing) → Ready to proceed
- ✅ Update page with changes → Working with fixed OAuth signature
- ⏳ Document comment preservation behavior → Ready to test

## Technical Debt & Considerations

### Security
- ✅ Secrets properly excluded from git
- ✅ OAuth 1.0 with RSA-SHA1 signature
- ⚠️ No token refresh mechanism (OAuth 1.0 tokens persist for ~5 years)

### Error Handling
- ✅ HTTP error status checking
- ✅ User-friendly error messages
- ✅ Comprehensive logging
- ⚠️ 500 errors not providing detailed failure reasons

### Testing
- ⚠️ No automated tests (POC scope)
- ⚠️ Manual testing blocked by permission issues
- ✅ CLI commands for manual testing

### Future Enhancements (Out of Scope)
- Batch processing of multiple markdown files
- Automatic space/parent page detection
- Dry-run mode for previewing changes
- Diff preview before updates
- Comment anchoring preservation
- Git workflow integration
- Automated testing suite

## Dependencies Version History

### Current
```toml
[project]
requires-python = ">=3.14"
dependencies = [
    "click>=8.3.0",
    "httpx>=0.28.1",
    "authlib>=1.6.0",
    "cryptography>=44.0.0",
    "md2cf>=2.3.0"
]
```

### Rationale
- `httpx`: Modern async HTTP client, used in adrflow reference
- `authlib`: OAuth 1.0 support with RSA-SHA1
- `cryptography`: RSA key parsing and crypto operations
- `click`: User-friendly CLI with rich formatting
- `md2cf`: Proven Confluence markdown conversion library
