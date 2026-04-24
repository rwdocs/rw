/**
 * Track the viewport-relative bounding rect of an element across resizes and
 * ancestor scrolls.
 *
 * Returns a reactive object whose `top`/`left`/`width`/`height` values update
 * whenever the element resizes, the window resizes, or any scroll container
 * scrolls. Consumers read the fields inside `$derived` (or directly in markup)
 * to drive positioning — typically for popover panels that need to sit beside
 * an external anchor.
 *
 * The getter form lets the anchor target switch at runtime (e.g. when a parent
 * mounts a different trigger); the internal `$effect` re-subscribes whenever
 * the getter returns a different element.
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
   * False until the first `getBoundingClientRect` read completes. Consumers
   * use this to suppress a panel's initial paint at (0,0) while waiting for
   * the anchor's coordinates — the classic Radix/Floating UI "positioning"
   * state. True in all later states, including after the anchor changes.
   */
  readonly measured: boolean;
}

export function useAnchorOffset(getEl: () => HTMLElement | null): AnchorOffset {
  const rect = $state({ top: 0, left: 0, width: 0, height: 0, measured: false });

  $effect(() => {
    const el = getEl();
    if (!el) return;

    const measure = () => {
      const r = el.getBoundingClientRect();
      rect.top = r.top;
      rect.left = r.left;
      rect.width = r.width;
      rect.height = r.height;
      rect.measured = true;
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    window.addEventListener("scroll", measure, { capture: true, passive: true });
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
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
