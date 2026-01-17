# TASKS

## Current Task

None

## Completed Tasks

### Fix navigation link click issue (2026-01-17)

**Problem:** Clicking navigation links didn't always update page content. First click worked, but subsequent clicks showed stale content.

**Root Cause:** Race condition between `{#key $path}` in App.svelte and the store subscription in Page.svelte. When path changed:
1. Old Page subscription fired, called `page.load()`
2. `{#key}` destroyed old Page, created new Page
3. New Page subscription fired, called `page.load()` again
4. Two concurrent API calls for same path caused race condition

**Fix:** Remove `{#key $path}` from App.svelte. Page.svelte now solely handles path changes via store subscription:
- `page.clear()` resets state before loading
- `page.load()` fetches new content
- Single Page component instance handles all navigation
