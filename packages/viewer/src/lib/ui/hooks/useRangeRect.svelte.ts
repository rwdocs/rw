/**
 * Track the viewport-relative bounding rect of a `Range` across scrolls and
 * resizes. Mirrors `useAnchorOffset`, but for `Range` targets, which can't be
 * fed to a `ResizeObserver` — re-measurement is driven by window scroll
 * (capture phase, so ancestor scroll containers count) and window resize. The
 * underlying `$state` already skips no-op writes, so callers don't need to
 * guard against scroll frames where the rect is unchanged.
 *
 * The getter form lets the target Range switch at runtime (or go to `null`
 * when the selection is dropped); the internal `$effect` re-subscribes on
 * change and tears down listeners when the Range goes away.
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
    const range = getRange();
    if (!range) {
      rect.measured = false;
      return;
    }

    const measure = () => {
      const r = range.getBoundingClientRect();
      rect.top = r.top;
      rect.left = r.left;
      rect.width = r.width;
      rect.height = r.height;
      rect.measured = true;
    };

    measure();
    window.addEventListener("scroll", measure, { capture: true, passive: true });
    window.addEventListener("resize", measure);
    return () => {
      window.removeEventListener("scroll", measure, { capture: true });
      window.removeEventListener("resize", measure);
    };
  });

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
