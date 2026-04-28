import { observeElement } from "./observeElement.svelte";

/**
 * Track an element's content-box size across resizes.
 *
 * Returns a reactive object with the latest `width`/`height` and a `version`
 * counter that increments on every measurement (mount + each ResizeObserver
 * firing). Consumers that only need a "size changed" signal — typically to
 * recompute a derived rect (e.g. a Range's client rects) when the container's
 * layout shifts — should subscribe via `void size.version`; that fires on any
 * dimension change, not just width or height.
 *
 * Use `useAnchorOffset` instead when the consumer needs the element's
 * viewport-relative position. This hook deliberately omits scroll/resize
 * listeners on `window` — its values are scroll-invariant by construction, so
 * tracking scroll would only produce redundant work. That matters when the
 * observed element is on the main scroll path (e.g. the article body):
 * `useAnchorOffset` would fire its consumer effect on every scroll frame,
 * which is precisely what this hook avoids.
 *
 * The getter form lets the target switch at runtime; the internal `$effect`
 * re-subscribes whenever the getter returns a different element.
 */
export interface ElementSize {
  readonly width: number;
  readonly height: number;
  readonly version: number;
}

export function useElementSize(getEl: () => HTMLElement | null): ElementSize {
  const size = $state({ width: 0, height: 0, version: 0 });
  // Plain `let`, not `$state`: `size.version += 1` would *read* `version`
  // inside the same `$effect` that triggered the measurement, creating a
  // self-dependency that re-fires the effect on its own write.
  let counter = 0;

  observeElement(getEl, (el) => {
    const r = el.getBoundingClientRect();
    size.width = r.width;
    size.height = r.height;
    size.version = ++counter;
  });

  return {
    get width() {
      return size.width;
    },
    get height() {
      return size.height;
    },
    get version() {
      return size.version;
    },
  };
}
