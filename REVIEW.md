# Code Review - Pre-Phase 3

## Strengths

- **Solid Rust core**: 35 tests, clean error handling, proper escaping
- **Good architecture**: Builder pattern, iterator adapters, state machines
- **Recent improvements**: Inline formatting in headings, table alignment support

---

## Issues to Address

### 1. No Python Tests

The CLI and config modules have no unit tests. This is a significant gap for a project with 1,480 lines of Python code.

### 2. HTML Renderer Complexity

`html.rs` is 798 lines with multiple boolean state flags (`in_code_block`, `in_image`, `in_first_h1`, `in_heading`, `skip_heading_text`). Could be consolidated into an enum-based state machine for clarity.

### 3. Error Handling Gaps

- Kroki diagram rendering stops on first error instead of collecting all errors
- No cleanup for partial file writes during multi-diagram rendering
- PlantUML syntax not validated before sending to Kroki

### 4. Hardcoded Values

- DPI hardcoded to 192 in `plantuml.rs`
- Include depth limit of 10 is arbitrary
- Consumer key 'adrflow' hardcoded in `oauth.py`

### 5. Missing Integration Tests

No end-to-end tests for the full conversion pipeline (Markdown → Rust → Python → output).

---

## Improvements to Consider

### Before Phase 3

1. **Add Python tests** for CLI commands and config loading
2. **Extract HtmlRenderer state** into a dedicated struct to reduce complexity
3. **Make DPI configurable** via `MarkdownConverter` builder

### For Phase 3 (HTTP API)

1. **Add frontmatter parsing** - Most markdown docs have YAML metadata
2. **Implement caching layer** - Essential for API performance
3. **Add structured logging** - Currently no logging in Rust core
4. **Consider async Kroki calls** - Current `ureq` is sync; may block async API

---

## Documentation Gaps

- No module-level rustdoc comments explaining the state machine designs
- No ADR explaining architectural decisions (e.g., why sync HTTP in Rust)
- TASKS.md should be checked for Phase 3 requirements alignment

---

## Resolution Status

- [x] Issue 1: No Python Tests - Added 16 tests for config and CLI
- [x] Issue 2: HTML Renderer Complexity - Extracted state into `CodeBlockState`, `TableState`, `ImageState`, `HeadingState` structs
- [x] Issue 3: Error Handling Gaps - `render_all` now collects all diagram errors via `RenderError::Multiple`
- [x] Issue 4: Hardcoded Values - DPI now configurable via `MarkdownConverter::dpi()`, exported as `DEFAULT_DPI`
- [ ] Issue 5: Missing Integration Tests - Deferred to Phase 3
