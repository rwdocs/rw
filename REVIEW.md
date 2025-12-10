# Docstage Codebase Review

**Date:** 2025-12-09

## Executive Summary

The Docstage codebase is **well-architected** with clean separation between Rust core, Python backend, and Svelte frontend. Tests are comprehensive (114 Python tests, 75 Rust tests - all passing). Code quality is generally high with proper error handling and good documentation.

---

## REQUIRED Issues (Must Fix)

### 1. Dead Code - Unused Function

**File:** `crates/docstage-core/src/plantuml.rs:173`

```
warning: function `load_config_file_with_warning` is never used
```

Either use this function or remove it. If intended for future use, add `#[allow(dead_code)]` with a comment explaining why.

### 2. Clippy Warning - Suboptimal Pattern

**File:** `crates/docstage-core/src/plantuml.rs:301`

```rust
// Current
let result = prepare_diagram_source(source, &[temp_dir.clone()], None, DEFAULT_DPI);

// Suggested
let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), None, DEFAULT_DPI);
```

Avoids unnecessary clone.

---

## SUGGESTED Improvements

### 1. Code Duplication - SVG Scaling Logic

**Files:**
- `crates/docstage-core/src/converter.rs:90-132` (Rust)
- `packages/docstage/src/docstage/core/diagrams.py:227-279` (Python)

The SVG dimension scaling logic is duplicated between Rust and Python. While there's a valid reason (Python caching uses scaled dimensions), consider:
- Adding a comment in both files explaining why duplication exists
- Or extracting to a shared utility exposed via PyO3

### 2. Exception Handling - Broad Catch

**File:** `packages/docstage/src/docstage/cli.py`

Multiple functions catch broad `Exception`:

```python
# Lines 539, 589, 651, 733, 833, 989, 1117, 1292
except Exception as e:
    click.echo(click.style(f'Error: {e}', fg='red'), err=True)
```

Per style guide, prefer specific exceptions. Consider catching:
- `httpx.HTTPError` for HTTP failures
- `FileNotFoundError`, `PermissionError` for file operations
- `tomllib.TOMLDecodeError` for config parsing

### 3. Docstring Alignment

**File:** `crates/docstage-core/src/converter.rs:378`

```
warning: doc list item overindented
```

```rust
// Current
///                 When provided, relative `.md` links are transformed to absolute `/docs/...` paths.

// Fix - use 2 spaces for list item continuation
///   When provided, relative `.md` links are transformed to absolute `/docs/...` paths.
```

### 4. Frontend Warning - SVG Path Element

**Files:**
- `frontend/src/components/NavItem.svelte:41`
- `frontend/src/components/MobileDrawer.svelte:53`

```
Warn: `<path>` will be treated as an HTML element unless it begins with a capital letter
```

This is a Svelte warning about inline SVG. While functional, consider extracting SVG icons to a separate component or using an icon library.

### 5. Missing Type Annotations

**File:** `packages/docstage/src/docstage/core/renderer.py:219`

```python
toc: list  # Should be list[TocEntryProtocol] or similar
```

---

## CONSIDER Enhancements

### 1. Error Recovery in Diagram Rendering

**File:** `packages/docstage/src/docstage/core/diagrams.py:127-129`

Currently, diagram errors are handled per-diagram which is good. Consider adding a summary log when multiple diagrams fail in a single page render.

### 2. Configuration Validation

**File:** `packages/docstage/src/docstage/config.py`

The config parsing is thorough but could benefit from:
- A `validate()` method that checks semantic validity (e.g., port range, valid URL format for kroki_url)
- Using Pydantic for automatic validation (though current approach is acceptable)

### 3. Frontend State Management

**File:** `frontend/src/stores/navigation.ts`

The `expandOnlyTo` optimization at line 88-91 is good but the comment could be clearer about the "different branches" case.

### 4. Caching Strategy Documentation

The caching approach (mtime-based for pages, content-hash for diagrams) is solid but not documented in CLAUDE.md or architecture docs. Consider adding a section explaining:
- Why mtime for pages (fast, simple)
- Why content-hash for diagrams (source can change without file mtime)

---

## Architecture Observations (Positive)

1. **Clean Separation**: Rust handles all markdown parsing/rendering, Python handles HTTP/caching/orchestration, Svelte handles UI
2. **Good Use of Protocols**: `TocEntryProtocol` in Python allows compatibility between Rust types and cached entries
3. **Proper Async**: aiohttp with proper startup/cleanup hooks for live reload
4. **Content-Based Caching**: Diagram caching by SHA-256 hash is efficient and correct
5. **Error Boundaries**: Diagrams fail individually without breaking the whole page

---

## Test Coverage Assessment

- **Rust**: 75 tests covering core rendering, diagram extraction, link resolution
- **Python**: 114 tests covering API, caching, config, navigation, CLI
- **Frontend**: No automated tests (CONSIDER adding Vitest/Playwright)

---

## Summary

| Category | Count |
|----------|-------|
| REQUIRED | 2 |
| SUGGESTED | 5 |
| CONSIDER | 4 |

The codebase is production-quality with minor issues. The two REQUIRED items are from compiler/linter warnings and should be addressed before release.
