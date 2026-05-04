import { observeTarget } from "./observeTarget.svelte";

/**
 * Track the viewport-relative bounding rect of a target across resizes and
 * ancestor scrolls. The target may be an `Element` (observed via
 * `ResizeObserver` for size changes) or a `Range` (re-measured only on
 * window scroll/resize, since Ranges aren't observable).
 *
 * Returns a reactive object whose `top`/`left`/`width`/`height` values update
 * whenever the target resizes, the window resizes, or any scroll container
 * scrolls. Consumers read the fields inside `$derived` (or directly in markup)
 * to drive positioning — typically for popover panels that need to sit beside
 * an external anchor or a selected text passage.
 *
 * The getter form lets the anchor target switch at runtime (e.g. when a parent
 * mounts a different trigger); the internal `$effect` re-subscribes whenever
 * the getter returns a different target.
 *
 * Scroll tracking uses a capture-phase listener on `window`, which catches
 * scrolls from any ancestor scroll container without walking the DOM — this
 * matters when the viewer is embedded in a host app that owns the viewport
 * (e.g. Backstage) and scroll happens in an arbitrary ancestor element.
 */
export interface AnchorOffset {
  readonly top: number;
  readonly left: number;
  readonly width: number;
  readonly height: number;
  /**
   * False until the first `getBoundingClientRect` read completes, and again
   * whenever the target getter returns `null`. Consumers use this to suppress
   * a panel's initial paint at (0,0) while waiting for coordinates — the
   * classic Radix/Floating UI "positioning" state — and to hide the panel
   * once the anchor disappears.
   */
  readonly measured: boolean;
}

export function useAnchorOffset<T extends Element | Range>(
  getTarget: () => T | null,
): AnchorOffset {
  const rect = $state({ top: 0, left: 0, width: 0, height: 0, measured: false });

  observeTarget(
    getTarget,
    (target) => {
      const r = target.getBoundingClientRect();
      rect.top = r.top;
      rect.left = r.left;
      rect.width = r.width;
      rect.height = r.height;
      rect.measured = true;
    },
    {
      trackWindow: true,
      onLost: () => {
        rect.measured = false;
      },
    },
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
