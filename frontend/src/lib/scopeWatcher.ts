import { get } from "svelte/store";
import type { Readable } from "svelte/store";
import type { PageResponse } from "../types";
import { navigation } from "../stores/navigation";

interface PageState {
  data: PageResponse | null;
}

/**
 * Creates a subscription that watches for page scope changes and reloads navigation.
 * Returns an unsubscribe function that should be called in onDestroy.
 */
export function watchPageScope(pageStore: Readable<PageState>): () => void {
  return pageStore.subscribe((state) => {
    if (!state.data) return;

    const pageScope = state.data.meta.navigationScope;
    const currentScope = get(navigation).currentScope;

    // Only update navigation if we have scope information from the page.
    // Skip if navigationScope is undefined (e.g., from cached response).
    if (pageScope !== undefined && pageScope !== currentScope) {
      navigation.loadScope(pageScope);
    }
  });
}
