# Changelog

## [Unreleased]

### 2025-12-06
- Implement Phase 2: Rust Core - HTML Renderer
  - Add `HtmlRenderer` module producing semantic HTML5
  - Generate heading IDs for anchor links
  - Create `TocEntry` struct for table of contents
  - Add `MarkdownConverter::convert_html()` method
  - Update Python bindings with `HtmlConvertResult` and `TocEntry` classes
  - Preserve inline formatting in headings (code, emphasis, strong, links)
  - Add table column alignment support via inline styles
  - Code blocks output `language-*` class for client-side highlighting
- Create RD-001: Docstage Backend requirements document
- Define API design for page rendering and navigation endpoints
- Plan implementation phases for HTML renderer, core library, HTTP API, and live reload

### 2025-12-05
- Rename project from md2conf to Docstage ("Where documentation takes the stage")
- Rename packages: md2conf → docstage, md2conf-core → docstage-core
- Update CLI entrypoint: `md2conf` → `docstage`
- Restructure Rust code into Cargo workspace:
  - `crates/docstage-core`: Pure Rust library (no PyO3)
  - `packages/docstage-core`: Python package with PyO3 bindings (maturin)
- Move conversion logic from PyO3 bindings to `docstage-core::MarkdownConverter`

### 2025-12-04
- Merge `convert_with_diagrams` into `convert` method (kroki_url/output_dir now required)
- Move Kroki diagram rendering from Python to Rust with parallel requests (rayon + ureq)

### 2025-12-01
- Comment preservation for inline comments on page updates
- OAuth signature fix: `force_include_body=True` for POST/PUT
- Markdown to Confluence converter via Rust core
- Confluence REST API client with OAuth 1.0 RSA-SHA1
- CLI: `convert`, `create`, `update`, `get-page`, `generate-tokens`, `test-auth`
