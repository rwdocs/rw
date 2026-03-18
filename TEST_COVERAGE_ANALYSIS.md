# Test Coverage Analysis

## Current State

**Rust**: 708 tests across 63/112 source files (56% file coverage)
**Frontend**: 6 unit test files + 6 E2E test files
**Infrastructure**: LLVM coverage, Criterion benchmarks, Playwright E2E, CI on Ubuntu + Windows

### Per-Crate Summary

| Crate | Files Tested | Total Files | Tests | Coverage |
|-------|-------------|-------------|-------|----------|
| rw (CLI) | 0 | 10 | 0 | 0% |
| rw-assets | 1 | 1 | 3 | 100% |
| rw-cache | 3 | 3 | 21 | 100% |
| rw-cache-s3 | 1 | 1 | 2 | 100% |
| rw-config | 2 | 2 | 46 | 100% |
| rw-confluence | 14 | 30 | 72 | 47% |
| rw-diagrams | 8 | 10 | 105 | 80% |
| rw-embedded-preview | 0 | 1 | 0 | 0% |
| rw-napi | 1 | 2 | 9 | 50% |
| rw-renderer | 16 | 20 | 232 | 80% |
| rw-server | 6 | 13 | 12 | 46% |
| rw-site | 3 | 4 | 73 | 75% |
| rw-storage | 4 | 5 | 61 | 80% |
| rw-storage-fs | 5 | 5 | 95 | 100% |
| rw-storage-s3 | 1 | 4 | 7 | 25% |

---

## Recommended Improvements (Priority Order)

### 1. rw-server: Handler and Middleware Tests (High Impact)

**Current state**: 12 tests for 13 files. The HTTP handlers have minimal coverage — most have only 1-2 tests.

**What to add**:
- **Page handler edge cases**: error responses (storage failures, render errors), query parameter handling, path traversal rejection
- **Static file serving**: SPA fallback behavior, MIME type detection, cache headers
- **Security middleware**: verify all security headers (CSP, X-Frame-Options, etc.) are set correctly on various response types
- **Live reload WebSocket**: connection lifecycle, message broadcasting to multiple clients, reconnection after server restart
- **App/router integration**: test the full axum router wiring — request routing, middleware ordering, 404 handling

**Why**: The server is the main user-facing surface. Bugs here (broken routes, missing headers, incorrect content types) directly affect users.

### 2. rw-confluence: Client and Updater Tests (High Impact)

**Current state**: Comment preservation is well-tested (42 tests), but the Confluence API client (`client/`) and page updater (`updater/`) have zero tests.

**What to add**:
- **API client**: Mock HTTP responses to test page CRUD operations, attachment uploads, comment operations, error handling (auth failures, rate limiting, network errors), pagination
- **Page updater**: Test the update workflow — dry run output, page creation vs update decision, attachment handling, error recovery
- **Renderer**: Test Confluence XHTML output for various markdown inputs (tables, code blocks, headings, links), ensuring valid Confluence storage format

**Why**: Confluence publishing is a core feature. The updater orchestrates multiple API calls — a regression here could corrupt live Confluence pages.

### 3. rw-storage-s3: Storage Backend Tests (Medium Impact)

**Current state**: Only `format.rs` is tested (7 tests). The S3 storage implementation, S3 client utilities, and bundle publisher have no tests.

**What to add**:
- **S3Storage**: Mock the AWS SDK client to test document retrieval, scanning, path resolution, and error handling
- **BundlePublisher**: Test bundle creation, manifest generation, upload sequencing
- **S3 client utilities**: Test client configuration, retry behavior

**Why**: S3 storage powers the Backstage integration. Failures here mean broken documentation in deployed Backstage instances.

### 4. Frontend: Utility Library Unit Tests (Medium Impact)

**Current state**: All 5 state modules have unit tests, but 6 utility libraries under `src/lib/` have zero test coverage.

**What to add**:
- **`tabs.ts`**: Tab content initialization, tab switching, keyboard navigation, state persistence
- **`dismissible.ts`**: Click-outside detection, cleanup on unmount, edge cases (nested elements, SVG clicks)
- **`navigation.ts`**: Navigation utility functions, path matching, active state calculation
- **`scopeWatcher.svelte.ts`**: Scope change detection, callback invocation

**Why**: These utilities contain logic shared across components. A bug in `dismissible.ts` could break every dropdown and popover in the app.

### 5. rw-site: Page Rendering Edge Cases (Medium Impact)

**Current state**: 73 tests, good coverage of site structure. `page.rs` has only 7 tests.

**What to add**:
- **Page rendering errors**: Test behavior when markdown contains invalid syntax, missing code block references, broken directive syntax
- **TOC generation**: Edge cases — duplicate heading text, deeply nested headings, headings with inline code/links, empty headings
- **Breadcrumb generation**: Deep nesting, special characters in paths, root page breadcrumbs
- **Cache interaction**: Verify cached vs uncached rendering produces identical output, cache invalidation on content change

**Why**: Page rendering is the core data flow. The existing 7 tests likely cover happy paths — edge cases in TOC and breadcrumb generation are common sources of bugs.

### 6. E2E: Dark Mode and Theme Switching (Low-Medium Impact)

**Current state**: E2E tests cover navigation, content rendering, mobile, and embedded mode. No tests for dark mode.

**What to add**:
- **Dark mode rendering**: Verify code syntax highlighting colors, diagram backgrounds, alert styling in dark mode
- **Theme toggle in embedded mode**: Test `setColorScheme()` transitions between light/dark/auto
- **OS preference following**: Test `prefers-color-scheme` media query behavior

**Why**: Dark mode was added in v0.1.11 and touches every visual element. Theme bugs are easy to introduce and hard to catch without automated tests.

### 7. E2E: Live Reload (Low-Medium Impact)

**Current state**: Live reload has unit tests for the WebSocket client, but no E2E test verifying the full flow.

**What to add**:
- **Content update**: Modify a markdown file on disk, verify the browser updates without manual reload
- **Navigation update**: Add/remove a page, verify the sidebar updates
- **Error recovery**: Kill and restart the server, verify the client reconnects

**Why**: Live reload is a headline feature. An E2E test would catch regressions in the full pipeline (file watcher → WebSocket → DOM update).

### 8. rw-renderer: Backend Trait and Integration (Low Impact)

**Current state**: 232 tests — the most comprehensive suite. The `backend.rs` trait definition and `util.rs` have no tests.

**What to add**:
- **Cross-backend consistency**: Render the same markdown through both `HtmlBackend` and `ConfluenceBackend`, verify structural equivalence (same headings, same link targets, same code blocks)
- **Directive edge cases**: Malformed directive syntax, nested directives, directives in code blocks (should be ignored)

**Why**: Low priority since existing coverage is strong, but cross-backend tests would catch divergence between HTML and Confluence output.

---

## Testing Gaps NOT Worth Pursuing

- **CLI binary (`rw/`)**: Commands are thin wrappers over library crates. Testing the libraries is sufficient; CLI integration tests would be slow and brittle.
- **`lib.rs` module files**: These are just re-exports (`pub mod`, `pub use`). No logic to test.
- **`rw-embedded-preview`**: Development-only preview shell with minimal logic.
- **`rw-napi/types.rs`**: Pure type definitions with serde derives.

---

## Quick Wins

1. **Add `#[test]` for `rw-server` security middleware** — verify all security headers in one test function. Currently only 1 test exists.
2. **Add unit tests for `tabs.ts`** — tab initialization logic has several code paths and is used on many pages.
3. **Add a Playwright test for 404/NotFound page** — no test currently verifies the error page renders correctly.
4. **Test S3Storage with mocked AWS client** — the mock pattern from `rw-storage/mock.rs` can be adapted.
