import { observeElement } from "./observeElement.svelte";

/**
 * Track the viewport-relative bounding rect of a `Range` across scrolls and
 * resizes. Sibling of `useAnchorOffset`, but for `Range` targets — Ranges
 * can't be fed to a `ResizeObserver`, so re-measurement relies on the window
 * scroll (capture phase) and resize listeners that `observeElement` sets up
 * under `trackWindow`.
 */
export interface RangeRect {
  readonly top: number;
  readonly left: number;
  readonly width: number;
  readonly height: number;
  /**
   * False until the first `getBoundingClientRect` read completes, and again
   * whenever the target Range becomes `null`. Consumers gate rendering on this
   * to avoid a flash at (0, 0) before the first measurement (or after the
   * Range disappears).
   */
  readonly measured: boolean;
}

export function useRangeRect(getRange: () => Range | null): RangeRect {
  const rect = $state({ top: 0, left: 0, width: 0, height: 0, measured: false });

  $effect(() => {
    if (!getRange()) rect.measured = false;
  });

  observeElement(
    getRange,
    (range) => {
      const r = range.getBoundingClientRect();
      rect.top = r.top;
      rect.left = r.left;
      rect.width = r.width;
      rect.height = r.height;
      rect.measured = true;
    },
    { trackWindow: true },
  );

  return {
    get top() {
      return rect.top;
    },
    get left() {
      return rect.left;
    },
    get width() {
      return rect.width;
    },
    get height() {
      return rect.height;
    },
    get measured() {
      return rect.measured;
    },
  };
}
