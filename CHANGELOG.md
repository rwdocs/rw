# Changelog

## [Unreleased]

### 2025-12-05
- Rename project from md2conf to Docstage ("Where documentation takes the stage")
- Rename packages: md2conf → docstage, md2conf-core → docstage-core
- Update CLI entrypoint: `md2conf` → `docstage`

### 2025-12-04
- Merge `convert_with_diagrams` into `convert` method (kroki_url/output_dir now required)
- Move Kroki diagram rendering from Python to Rust with parallel requests (rayon + ureq)

### 2025-12-01
- Comment preservation for inline comments on page updates
- OAuth signature fix: `force_include_body=True` for POST/PUT
- Markdown to Confluence converter via Rust core
- Confluence REST API client with OAuth 1.0 RSA-SHA1
- CLI: `convert`, `create`, `update`, `get-page`, `generate-tokens`, `test-auth`
