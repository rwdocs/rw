import { observeMove } from "./observeMove.svelte";

/**
 * Track an element's *position changes* as a monotonic `version` counter.
 *
 * Returns a reactive object whose `version` increments whenever the element
 * moves — including when content above it reflows (web-font swap, late image /
 * diagram load, an expanding block) and the element slides without its own box
 * resizing. Consumers subscribe via `void move.version` to recompute a
 * position-derived value that a `ResizeObserver` would miss.
 *
 * Companion to `useElementSize` (which tracks size). The getter form lets the
 * target switch at runtime; the internal effect re-subscribes when the getter
 * returns a different element.
 */
export interface ElementMove {
  readonly version: number;
}

export function useElementMove(getEl: () => HTMLElement | null): ElementMove {
  const state = $state({ version: 0 });
  // Plain `let`, not `$state`: incrementing inside the observer callback must
  // not create a self-dependency (mirrors useElementSize).
  let counter = 0;

  observeMove(getEl, () => {
    state.version = ++counter;
  });

  return {
    get version() {
      return state.version;
    },
  };
}
