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
 * Implementation detail of `useElementSize` / `useAnchorOffset` — not exported
 * outside the hooks/ layer.
 */
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
    if (trackWindow) {
      window.addEventListener("scroll", measure, { capture: true, passive: true });
      window.addEventListener("resize", measure);
    }
    return () => {
      observer.disconnect();
      if (trackWindow) {
        window.removeEventListener("scroll", measure, { capture: true });
        window.removeEventListener("resize", measure);
      }
    };
  });
}
