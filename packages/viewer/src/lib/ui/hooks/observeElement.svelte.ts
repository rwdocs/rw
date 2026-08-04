/**
 * Run `onMeasure(el)` once on subscription and again whenever the element
 * resizes. Re-subscribes automatically when the getter starts returning a
 * different element, and tears down the observer (and any window listeners)
 * on cleanup.
 *
 * `trackWindow`: also re-measure on window scroll (capture phase, so ancestor
 * scroll containers count) and window resize. Use this for hooks that report
 * viewport-relative coordinates; skip it for size-only hooks, since size is
 * scroll-invariant and listening would only produce redundant work.
 *
 * This observes the element's own box only. Position changes caused by content
 * *above* it reflowing (which leave its box unchanged) are the job of
 * `observeMove`; consumers that anchor to an element use both.
 *
 * Implementation detail of `useElementSize` / `useAnchorOffset` — not exported
 * outside the hooks/ layer.
 */
import { on } from "svelte/events";

export function observeElement(
  getEl: () => HTMLElement | null,
  onMeasure: (el: HTMLElement) => void,
  { trackWindow = false }: { trackWindow?: boolean } = {},
): void {
  $effect(() => {
    const el = getEl();
    if (!el) return;

    const measure = () => onMeasure(el);

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    const offScroll = trackWindow
      ? on(window, "scroll", measure, { capture: true, passive: true })
      : undefined;
    const offResize = trackWindow ? on(window, "resize", measure) : undefined;
    return () => {
      observer.disconnect();
      offScroll?.();
      offResize?.();
    };
  });
}
