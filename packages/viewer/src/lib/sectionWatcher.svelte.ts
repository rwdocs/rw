import { untrack } from "svelte";
import type { Page } from "../state/page.svelte";
import type { Navigation } from "../state/navigation.svelte";

/**
 * Creates an effect that watches for page section changes and reloads navigation.
 * Must be called during component initialization (uses $effect).
 */
export function watchPageSection(page: Page, navigation: Navigation): void {
  $effect(() => {
    if (!page.data) return;

    // page.data is $state.raw, so this triggers on full reassignment (not mutation).
    const sectionRef = page.data.meta.sectionRef;
    // Read currentSectionRef without tracking — we only want this effect to re-run
    // when page.data changes, not when currentSectionRef is updated by loadSection().
    const current = untrack(() => navigation.currentSectionRef);

    if (sectionRef !== current) {
      navigation.loadSection(sectionRef);
    }
  });
}
