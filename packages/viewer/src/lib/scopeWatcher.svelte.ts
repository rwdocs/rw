import { untrack } from "svelte";
import type { Page } from "../state/page.svelte";
import type { Navigation } from "../state/navigation.svelte";

/**
 * Creates an effect that watches for page scope changes and reloads navigation.
 * Must be called during component initialization (uses $effect).
 */
export function watchPageScope(page: Page, navigation: Navigation): void {
  $effect(() => {
    if (!page.data) return;

    // page.data is $state.raw, so this triggers on full reassignment (not mutation).
    const pageScope = page.data.meta.navigationScope;
    // Read currentScope without tracking — we only want this effect to re-run
    // when page.data changes, not when currentScope is updated by loadScope().
    const currentScope = untrack(() => navigation.currentScope);

    if (pageScope !== undefined && pageScope !== currentScope) {
      navigation.loadScope(pageScope);
    }
  });
}
